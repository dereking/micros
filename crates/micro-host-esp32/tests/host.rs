use std::collections::BTreeMap;

use micro_compiler::compile_source;
use micro_host_esp32::{
    MicroAction, MicroErrorCode, MicroEvent, MicroEventKind, MicroState, OsHost, RuntimeHost,
};
use micro_ir::{FunctionId, NodeId, encode};
use micro_lvgl::NativeUi;

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
        os.dispatch(MicroEvent {
            kind: MicroEventKind::BootNormal as u32,
        }),
        MicroAction::InitializeStorage
    );
    assert_eq!(
        os.dispatch(MicroEvent {
            kind: MicroEventKind::StorageReady as u32,
        }),
        MicroAction::ValidateProfile
    );
    assert_eq!(os.state(), MicroState::StorageReady);
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
        ],
        [0, 1, 2, 3, 4, 5, 6]
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
            MicroAction::None as u32,
            MicroAction::Rejected as u32,
            MicroAction::EnterSafeMode as u32,
            MicroAction::InitializeStorage as u32,
            MicroAction::ValidateProfile as u32,
            MicroAction::InitializeDisplay as u32,
            MicroAction::InitializeSystemUi as u32,
            MicroAction::LoadNetworkConfig as u32,
            MicroAction::ShowFirstRunSetup as u32,
            MicroAction::ShowLauncher as u32,
            MicroAction::ShowSettings as u32,
            MicroAction::ConnectSavedWifi as u32,
            MicroAction::Reboot as u32,
            MicroAction::Composite as u32,
            MicroAction::Other as u32,
        ],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14]
    );
    assert_eq!(
        [
            MicroEventKind::BootNormal as u32,
            MicroEventKind::BootSafeMode as u32,
            MicroEventKind::StorageReady as u32,
            MicroEventKind::StorageFailed as u32,
            MicroEventKind::ProfileValid as u32,
            MicroEventKind::ProfileInvalid as u32,
            MicroEventKind::DisplayReady as u32,
            MicroEventKind::DisplayFailed as u32,
            MicroEventKind::SystemUiReady as u32,
            MicroEventKind::SystemUiFailed as u32,
            MicroEventKind::NetworkConfigured as u32,
            MicroEventKind::NetworkUnconfigured as u32,
            MicroEventKind::SetupSkipped as u32,
            MicroEventKind::OpenSettings as u32,
            MicroEventKind::BackPressed as u32,
            MicroEventKind::HomePressed as u32,
            MicroEventKind::RebootRequested as u32,
        ],
        [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]
    );
}
