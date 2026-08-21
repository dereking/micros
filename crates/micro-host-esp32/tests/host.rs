use std::collections::BTreeMap;

use micro_compiler::compile_source;
use micro_host_esp32::{
    MicroAction, MicroActionKind, MicroAppId, MicroBacklight, MicroErrorCode, MicroEvent,
    MicroEventKind, MicroFailureReason, MicroResult, MicroState, MicroWifiFailure, OsHost,
    RuntimeHost, decode_action_batch, encode_action_batch, validate_region_length,
    write_diagnostic,
};
use micro_ir::{FunctionId, NodeId, encode};
use micro_lvgl::NativeUi;
use micro_os_core::{
    Action, AppId, AppSessionId, Backlight, ConfirmationId, Event as OsEvent, FailureReason,
    WifiFailure, WifiOperationId,
};

#[derive(Default)]
struct FakeNativeUi {
    nodes: BTreeMap<NodeId, String>,
    activations: Vec<FunctionId>,
    destroyed: usize,
}

impl NativeUi for FakeNativeUi {
    fn create_column(&mut self, node: NodeId, _parent: Option<NodeId>) -> Result<(), String> {
        self.nodes.insert(node, String::new());
        Ok(())
    }

    fn create_label(
        &mut self,
        node: NodeId,
        _parent: Option<NodeId>,
        text: &str,
    ) -> Result<(), String> {
        self.nodes.insert(node, text.to_owned());
        Ok(())
    }

    fn create_button(
        &mut self,
        node: NodeId,
        _parent: Option<NodeId>,
        text: &str,
        handler: FunctionId,
    ) -> Result<(), String> {
        self.nodes.insert(node, text.to_owned());
        self.activations.push(handler);
        Ok(())
    }

    fn set_label_text(&mut self, node: NodeId, text: &str) -> Result<(), String> {
        self.nodes.insert(node, text.to_owned());
        Ok(())
    }

    fn destroy_app_root(&mut self) -> Result<(), String> {
        self.nodes.clear();
        self.destroyed += 1;
        Ok(())
    }
}

fn counter_bytes() -> Vec<u8> {
    let source = include_str!("../../../apps/counter/app.ts");
    let image = compile_source("apps/counter/app.ts", source).expect("Counter compiles");
    encode(&image).expect("Counter encodes")
}

#[test]
fn diagnostic_truncation_preserves_utf8_boundaries_and_always_terminates() {
    let cases = [
        (0, "hello", vec![]),
        (1, "hello", vec![0]),
        (6, "hello", b"hello\0".to_vec()),
        (5, "中文", vec![0xE4, 0xB8, 0xAD, 0, 0]),
        (5, "😀x", vec![0xF0, 0x9F, 0x98, 0x80, 0]),
        (4, "😀", vec![0, 0, 0, 0]),
    ];
    for (length, message, expected) in cases {
        let mut buffer = vec![0xAA; length];
        write_diagnostic(&mut buffer, message);
        assert_eq!(buffer, expected);
    }
}

#[test]
fn owned_mbc_constructor_accepts_the_single_owned_copy() {
    let host = RuntimeHost::from_owned_mbc(counter_bytes(), FakeNativeUi::default(), 10_000);
    assert!(host.is_ok());
}

#[test]
fn ffi_region_lengths_reject_isize_and_element_size_overflow_before_slicing() {
    assert_eq!(validate_region_length(0, 1), Ok(()));
    assert_eq!(validate_region_length(isize::MAX as usize, 1), Ok(()));
    assert_eq!(
        validate_region_length(isize::MAX as usize + 1, 1),
        Err(MicroErrorCode::InvalidArgument)
    );
    assert_eq!(
        validate_region_length(isize::MAX as usize, 2),
        Err(MicroErrorCode::InvalidArgument)
    );
    assert_eq!(
        validate_region_length(usize::MAX, usize::MAX),
        Err(MicroErrorCode::InvalidArgument)
    );
}

#[test]
fn activations_are_fifo_and_two_clicks_render_count_two() {
    let mut host = RuntimeHost::new(&counter_bytes(), FakeNativeUi::default(), 10_000).unwrap();
    let handler = host.bridge().activations[0];

    host.activate(handler).unwrap();
    host.activate(FunctionId(u32::MAX)).unwrap();
    assert!(host.tick().unwrap());
    assert!(host.bridge().nodes.values().any(|text| text == "Count: 1"));
    assert_eq!(host.tick().unwrap_err().code(), MicroErrorCode::Runtime);

    let mut host = RuntimeHost::new(&counter_bytes(), FakeNativeUi::default(), 10_000).unwrap();
    let handler = host.bridge().activations[0];
    host.activate(handler).unwrap();
    host.activate(handler).unwrap();
    assert!(host.tick().unwrap());
    assert!(host.tick().unwrap());
    assert!(host.bridge().nodes.values().any(|text| text == "Count: 2"));
}

#[test]
fn stop_removes_only_the_app_root_owned_nodes() {
    let mut host = RuntimeHost::new(&counter_bytes(), FakeNativeUi::default(), 10_000).unwrap();
    assert!(!host.bridge().nodes.is_empty());

    host.stop().unwrap();

    assert!(host.bridge().nodes.is_empty());
    assert_eq!(host.bridge().destroyed, 1);
}

#[test]
fn corrupt_mbc_has_a_stable_error_code_without_panicking() {
    let result =
        std::panic::catch_unwind(|| RuntimeHost::new(b"not MBC", FakeNativeUi::default(), 10_000));
    let error = match result.expect("decode must not panic") {
        Ok(_) => panic!("corrupt input unexpectedly decoded"),
        Err(error) => error,
    };
    assert_eq!(error.code(), MicroErrorCode::Mbc);
    assert_eq!(MicroErrorCode::Mbc as i32, 1);
}

#[test]
fn os_host_dispatches_through_the_shared_reducer() {
    let mut os = OsHost::new();
    assert_eq!(os.state(), MicroState::EarlyBoot);
    assert_eq!(
        os.dispatch(MicroEvent::from_core(&OsEvent::BootSampled {
            safe_mode: false
        }))
        .unwrap(),
        vec![MicroAction::new(MicroActionKind::InitializeStorage)]
    );
    assert_eq!(
        os.dispatch(MicroEvent::from_core(&OsEvent::StorageInitialized(Ok(()))))
            .unwrap(),
        vec![MicroAction::new(MicroActionKind::ValidateProfile)]
    );
    assert_eq!(os.state(), MicroState::StorageReady);
}

#[test]
fn every_core_event_variant_round_trips_without_losing_payload() {
    let events = vec![
        OsEvent::BootSampled { safe_mode: false },
        OsEvent::BootSampled { safe_mode: true },
        OsEvent::StorageInitialized(Ok(())),
        OsEvent::StorageInitialized(Err(FailureReason::StorageCorrupt)),
        OsEvent::ProfileValidated(Err(FailureReason::InvalidBoardProfile)),
        OsEvent::DisplayInitialized(Err(FailureReason::HardwareUnavailable)),
        OsEvent::SystemUiInitialized(Err(FailureReason::Internal)),
        OsEvent::NetworkConfigLoaded { configured: true },
        OsEvent::SetupSkipped,
        OsEvent::OpenSettings,
        OsEvent::SetBacklight(Backlight::Medium),
        OsEvent::BackPressed,
        OsEvent::HomePressed,
        OsEvent::OpenApp(AppId::Counter),
        OsEvent::AppStarted {
            session: AppSessionId(41),
        },
        OsEvent::AppFailed {
            session: AppSessionId(42),
            reason: FailureReason::AppCrashed,
        },
        OsEvent::RestartApp,
        OsEvent::AppStopped {
            session: AppSessionId(43),
        },
        OsEvent::WifiScanRequested,
        OsEvent::WifiScanCompleted {
            operation: WifiOperationId(51),
        },
        OsEvent::WifiScanFailed {
            operation: WifiOperationId(52),
            reason: WifiFailure::Timeout,
        },
        OsEvent::WifiConnectRequested,
        OsEvent::WifiConnected {
            operation: WifiOperationId(53),
        },
        OsEvent::WifiPersisted {
            operation: WifiOperationId(54),
        },
        OsEvent::WifiFailed {
            operation: WifiOperationId(55),
            reason: WifiFailure::Authentication,
        },
        OsEvent::ReconnectDue {
            reconnect: WifiOperationId(56),
        },
        OsEvent::ReconnectNowRequested,
        OsEvent::ClearNetworkRequested,
        OsEvent::ClearNetworkConfirmed {
            confirmation: ConfirmationId(61),
        },
        OsEvent::ClearNetworkCompleted {
            confirmation: ConfirmationId(62),
            result: Err(FailureReason::Internal),
        },
        OsEvent::FactoryResetRequested,
        OsEvent::FactoryResetConfirmed {
            confirmation: ConfirmationId(63),
        },
        OsEvent::FactoryResetCompleted {
            confirmation: ConfirmationId(64),
            result: Ok(()),
        },
        OsEvent::RebootRequested,
    ];
    for event in events {
        let wire = MicroEvent::from_core(&event);
        assert_eq!(wire.try_into_core().unwrap(), event);
        assert_unused_fields_are_rejected(wire);
    }
}

fn assert_unused_fields_are_rejected(wire: MicroEvent) {
    let mutations: [fn(&mut MicroEvent); 10] = [
        |value| value.result = MicroResult::Ok,
        |value| value.failure = MicroFailureReason::Internal,
        |value| value.wifi_failure = MicroWifiFailure::Internal,
        |value| value.app = MicroAppId::Counter,
        |value| value.flag = 2,
        |value| value.after_secs = 1,
        |value| value.reserved = 1,
        |value| value.session_id = 99,
        |value| value.operation_id = 99,
        |value| value.confirmation_id = 99,
    ];
    let allowed = match wire.kind {
        MicroEventKind::BootSampled | MicroEventKind::NetworkConfigLoaded => [
            false, false, false, false, true, false, false, false, false, false,
        ],
        MicroEventKind::StorageInitialized
        | MicroEventKind::ProfileValidated
        | MicroEventKind::DisplayInitialized
        | MicroEventKind::SystemUiInitialized => [
            true, true, false, false, false, false, false, false, false, false,
        ],
        MicroEventKind::OpenApp => [
            false, false, false, true, false, false, false, false, false, false,
        ],
        MicroEventKind::SetBacklight => [
            false, false, false, false, true, false, false, false, false, false,
        ],
        MicroEventKind::AppStarted | MicroEventKind::AppStopped => [
            false, false, false, false, false, false, false, true, false, false,
        ],
        MicroEventKind::AppFailed => [
            false, true, false, false, false, false, false, true, false, false,
        ],
        MicroEventKind::WifiScanCompleted
        | MicroEventKind::WifiConnected
        | MicroEventKind::WifiPersisted
        | MicroEventKind::ReconnectDue => [
            false, false, false, false, false, false, false, false, true, false,
        ],
        MicroEventKind::WifiScanFailed | MicroEventKind::WifiFailed => [
            false, false, true, false, false, false, false, false, true, false,
        ],
        MicroEventKind::ClearNetworkConfirmed | MicroEventKind::FactoryResetConfirmed => [
            false, false, false, false, false, false, false, false, false, true,
        ],
        MicroEventKind::ClearNetworkCompleted | MicroEventKind::FactoryResetCompleted => [
            true, true, false, false, false, false, false, false, false, true,
        ],
        MicroEventKind::SetupSkipped
        | MicroEventKind::OpenSettings
        | MicroEventKind::BackPressed
        | MicroEventKind::HomePressed
        | MicroEventKind::RestartApp
        | MicroEventKind::WifiScanRequested
        | MicroEventKind::WifiConnectRequested
        | MicroEventKind::ReconnectNowRequested
        | MicroEventKind::ClearNetworkRequested
        | MicroEventKind::FactoryResetRequested
        | MicroEventKind::RebootRequested => [false; 10],
    };
    for (index, mutation) in mutations.into_iter().enumerate() {
        if allowed[index] {
            continue;
        }
        let mut candidate = wire;
        mutation(&mut candidate);
        if candidate != wire {
            assert_eq!(
                candidate.try_into_core(),
                Err(MicroErrorCode::InvalidArgument)
            );
        }
    }
}

#[test]
fn every_core_action_variant_and_composite_round_trips_without_losing_payload() {
    let leaves = vec![
        Action::None,
        Action::Rejected,
        Action::EnterSafeMode(FailureReason::SafeModeRequested),
        Action::InitializeStorage,
        Action::ValidateProfile,
        Action::InitializeDisplay,
        Action::InitializeSystemUi,
        Action::LoadNetworkConfig,
        Action::ShowFirstRunSetup,
        Action::ShowLauncher,
        Action::ShowSettings,
        Action::ApplyBacklight(Backlight::Low),
        Action::StartWifiScan {
            operation: WifiOperationId(1),
        },
        Action::ConnectWifi {
            operation: WifiOperationId(2),
        },
        Action::ConnectSavedWifi {
            operation: WifiOperationId(3),
        },
        Action::PersistWifi {
            operation: WifiOperationId(4),
        },
        Action::ClearPendingWifi {
            operation: WifiOperationId(5),
        },
        Action::ScheduleWifiReconnect {
            reconnect: WifiOperationId(6),
            after_secs: 30,
        },
        Action::StartApp {
            app: AppId::Counter,
            session: AppSessionId(7),
        },
        Action::StopApp {
            app: AppId::Counter,
            session: AppSessionId(8),
        },
        Action::ShowAppError {
            app: AppId::Counter,
            session: AppSessionId(9),
            reason: FailureReason::AppCrashed,
        },
        Action::ConfirmClearNetwork {
            confirmation: ConfirmationId(10),
        },
        Action::ClearNetwork {
            confirmation: ConfirmationId(11),
        },
        Action::ConfirmFactoryReset {
            confirmation: ConfirmationId(12),
        },
        Action::FactoryReset {
            confirmation: ConfirmationId(13),
        },
        Action::Reboot,
    ];
    for action in &leaves {
        assert_eq!(
            decode_action_batch(&encode_action_batch(action)).unwrap(),
            *action
        );
    }
    let composite = Action::Actions(vec![
        leaves[11].clone(),
        Action::Actions(vec![leaves[16].clone(), leaves[19].clone()]),
        leaves[24].clone(),
    ]);
    assert_eq!(
        decode_action_batch(&encode_action_batch(&composite)).unwrap(),
        composite
    );
}

#[test]
fn insufficient_action_capacity_does_not_advance_the_reducer_or_partially_write() {
    let mut os = OsHost::new();
    let event = MicroEvent::from_core(&OsEvent::BootSampled { safe_mode: false });
    let mut output = [];
    let error = os.dispatch_into(event, &mut output).unwrap_err();
    assert_eq!(error.code, MicroErrorCode::BufferTooSmall);
    assert_eq!(error.required, 1);
    assert_eq!(os.state(), MicroState::EarlyBoot);
    assert_eq!(
        os.dispatch(event).unwrap(),
        vec![MicroAction::new(MicroActionKind::InitializeStorage)]
    );
}

#[test]
fn c_abi_types_use_c_representation_and_fixed_layouts() {
    assert_eq!(std::mem::size_of::<MicroErrorCode>(), 4);
    assert_eq!(std::mem::size_of::<MicroEventKind>(), 4);
    assert_eq!(std::mem::size_of::<MicroActionKind>(), 4);
    assert_eq!(std::mem::size_of::<MicroState>(), 4);
    assert_eq!(std::mem::size_of::<MicroFailureReason>(), 4);
    assert_eq!(std::mem::size_of::<MicroWifiFailure>(), 4);
    assert_eq!(std::mem::size_of::<MicroResult>(), 4);
    assert_eq!(std::mem::size_of::<MicroAppId>(), 4);
    assert_eq!(std::mem::size_of::<MicroBacklight>(), 4);
    assert_eq!(std::mem::size_of::<MicroEvent>(), 56);
    assert_eq!(std::mem::size_of::<MicroAction>(), 56);
}

#[test]
fn all_c_abi_discriminants_are_stable_and_exhaustive() {
    assert_eq!(
        [
            MicroErrorCode::Ok as i32,
            MicroErrorCode::Mbc as i32,
            MicroErrorCode::Runtime as i32,
            MicroErrorCode::Ui as i32,
            MicroErrorCode::InvalidArgument as i32,
            MicroErrorCode::Panic as i32,
            MicroErrorCode::Stopped as i32,
            MicroErrorCode::BufferTooSmall as i32,
        ],
        [0, 1, 2, 3, 4, 5, 6, 7]
    );
    assert_eq!(
        [
            MicroResult::Unused as u32,
            MicroResult::Ok as u32,
            MicroResult::Err as u32,
        ],
        [0, 1, 2]
    );
    assert_eq!(
        [
            MicroFailureReason::Unused as u32,
            MicroFailureReason::SafeModeRequested as u32,
            MicroFailureReason::StorageCorrupt as u32,
            MicroFailureReason::InvalidBoardProfile as u32,
            MicroFailureReason::HardwareUnavailable as u32,
            MicroFailureReason::AppCrashed as u32,
            MicroFailureReason::Internal as u32,
        ],
        [0, 1, 2, 3, 4, 5, 6]
    );
    assert_eq!(
        [
            MicroWifiFailure::Unused as u32,
            MicroWifiFailure::Authentication as u32,
            MicroWifiFailure::NetworkMissing as u32,
            MicroWifiFailure::Timeout as u32,
            MicroWifiFailure::Internal as u32,
        ],
        [0, 1, 2, 3, 4]
    );
    assert_eq!(
        [MicroAppId::Unused as u32, MicroAppId::Counter as u32],
        [0, 1]
    );
    assert_eq!(
        [
            MicroBacklight::Unused as u32,
            MicroBacklight::Off as u32,
            MicroBacklight::Low as u32,
            MicroBacklight::Medium as u32,
            MicroBacklight::High as u32,
        ],
        [0, 1, 2, 3, 4]
    );
    assert_eq!(
        [
            MicroState::EarlyBoot as u32,
            MicroState::SafeMode as u32,
            MicroState::StorageReady as u32,
            MicroState::BoardProfileValidated as u32,
            MicroState::DisplayReady as u32,
            MicroState::SystemUiReady as u32,
            MicroState::FirstRunSetup as u32,
            MicroState::Launcher as u32,
            MicroState::AppStarting as u32,
            MicroState::AppRunning as u32,
            MicroState::AppStopping as u32,
            MicroState::AppError as u32,
            MicroState::Settings as u32,
        ],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12]
    );
    assert_eq!(
        [
            MicroActionKind::None as u32,
            MicroActionKind::Rejected as u32,
            MicroActionKind::Actions as u32,
            MicroActionKind::EnterSafeMode as u32,
            MicroActionKind::InitializeStorage as u32,
            MicroActionKind::ValidateProfile as u32,
            MicroActionKind::InitializeDisplay as u32,
            MicroActionKind::InitializeSystemUi as u32,
            MicroActionKind::LoadNetworkConfig as u32,
            MicroActionKind::ShowFirstRunSetup as u32,
            MicroActionKind::ShowLauncher as u32,
            MicroActionKind::ShowSettings as u32,
            MicroActionKind::StartWifiScan as u32,
            MicroActionKind::ConnectWifi as u32,
            MicroActionKind::ConnectSavedWifi as u32,
            MicroActionKind::PersistWifi as u32,
            MicroActionKind::ClearPendingWifi as u32,
            MicroActionKind::ScheduleWifiReconnect as u32,
            MicroActionKind::StartApp as u32,
            MicroActionKind::StopApp as u32,
            MicroActionKind::ShowAppError as u32,
            MicroActionKind::ConfirmClearNetwork as u32,
            MicroActionKind::ClearNetwork as u32,
            MicroActionKind::ConfirmFactoryReset as u32,
            MicroActionKind::FactoryReset as u32,
            MicroActionKind::Reboot as u32,
            MicroActionKind::ApplyBacklight as u32,
        ],
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26
        ]
    );
    assert_eq!(
        [
            MicroEventKind::BootSampled as u32,
            MicroEventKind::StorageInitialized as u32,
            MicroEventKind::ProfileValidated as u32,
            MicroEventKind::DisplayInitialized as u32,
            MicroEventKind::SystemUiInitialized as u32,
            MicroEventKind::NetworkConfigLoaded as u32,
            MicroEventKind::SetupSkipped as u32,
            MicroEventKind::OpenSettings as u32,
            MicroEventKind::BackPressed as u32,
            MicroEventKind::HomePressed as u32,
            MicroEventKind::OpenApp as u32,
            MicroEventKind::AppStarted as u32,
            MicroEventKind::AppFailed as u32,
            MicroEventKind::RestartApp as u32,
            MicroEventKind::AppStopped as u32,
            MicroEventKind::WifiScanRequested as u32,
            MicroEventKind::WifiScanCompleted as u32,
            MicroEventKind::WifiScanFailed as u32,
            MicroEventKind::WifiConnectRequested as u32,
            MicroEventKind::WifiConnected as u32,
            MicroEventKind::WifiPersisted as u32,
            MicroEventKind::WifiFailed as u32,
            MicroEventKind::ReconnectDue as u32,
            MicroEventKind::ReconnectNowRequested as u32,
            MicroEventKind::ClearNetworkRequested as u32,
            MicroEventKind::ClearNetworkConfirmed as u32,
            MicroEventKind::ClearNetworkCompleted as u32,
            MicroEventKind::FactoryResetRequested as u32,
            MicroEventKind::FactoryResetConfirmed as u32,
            MicroEventKind::FactoryResetCompleted as u32,
            MicroEventKind::RebootRequested as u32,
            MicroEventKind::SetBacklight as u32,
        ],
        [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23,
            24, 25, 26, 27, 28, 29, 30, 31
        ]
    );
}
