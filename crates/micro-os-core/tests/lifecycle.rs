use micro_os_core::{Action, AppId, Event, FailureReason, MicroOs, State, WifiFailure, WifiState};

fn boot_to_system_ui(os: &mut MicroOs) {
    assert_eq!(
        os.dispatch(Event::BootSampled { safe_mode: false }),
        Action::InitializeStorage
    );
    assert_eq!(
        os.dispatch(Event::StorageInitialized(Ok(()))),
        Action::ValidateProfile
    );
    assert_eq!(os.state(), &State::StorageReady);
    assert_eq!(
        os.dispatch(Event::ProfileValidated(Ok(()))),
        Action::InitializeDisplay
    );
    assert_eq!(os.state(), &State::BoardProfileValidated);
    assert_eq!(
        os.dispatch(Event::DisplayInitialized(Ok(()))),
        Action::InitializeSystemUi
    );
    assert_eq!(os.state(), &State::DisplayReady);
    assert_eq!(
        os.dispatch(Event::SystemUiInitialized(Ok(()))),
        Action::LoadNetworkConfig
    );
    assert_eq!(os.state(), &State::SystemUiReady);
}

fn boot_to_launcher(os: &mut MicroOs) {
    boot_to_system_ui(os);
    assert_eq!(
        os.dispatch(Event::NetworkConfigLoaded { configured: true }),
        Action::ShowLauncher
    );
    assert_eq!(os.state(), &State::Launcher);
}

#[test]
fn normal_boot_selects_first_run_or_launcher_after_all_completions() {
    let mut first_run = MicroOs::new();
    boot_to_system_ui(&mut first_run);
    assert_eq!(
        first_run.dispatch(Event::NetworkConfigLoaded { configured: false }),
        Action::ShowFirstRunSetup
    );
    assert_eq!(first_run.state(), &State::FirstRunSetup);
    assert_eq!(
        first_run.dispatch(Event::SetupSkipped),
        Action::ShowLauncher
    );
    assert_eq!(first_run.state(), &State::Launcher);

    let mut configured = MicroOs::new();
    boot_to_launcher(&mut configured);
}

#[test]
fn safe_boot_and_each_initialization_failure_enter_safe_mode() {
    let reason = FailureReason::HardwareUnavailable;
    let mut safe = MicroOs::new();
    assert_eq!(
        safe.dispatch(Event::BootSampled { safe_mode: true }),
        Action::EnterSafeMode(FailureReason::SafeModeRequested)
    );
    assert_eq!(safe.state(), &State::SafeMode);
    assert_eq!(
        safe.dispatch(Event::OpenApp(AppId::Counter)),
        Action::Rejected
    );

    let mut storage = MicroOs::new();
    storage.dispatch(Event::BootSampled { safe_mode: false });
    assert_eq!(
        storage.dispatch(Event::StorageInitialized(Err(reason.clone()))),
        Action::EnterSafeMode(reason.clone())
    );

    let mut profile = MicroOs::new();
    profile.dispatch(Event::BootSampled { safe_mode: false });
    profile.dispatch(Event::StorageInitialized(Ok(())));
    assert_eq!(
        profile.dispatch(Event::ProfileValidated(Err(reason.clone()))),
        Action::EnterSafeMode(reason.clone())
    );

    let mut display = MicroOs::new();
    display.dispatch(Event::BootSampled { safe_mode: false });
    display.dispatch(Event::StorageInitialized(Ok(())));
    display.dispatch(Event::ProfileValidated(Ok(())));
    assert_eq!(
        display.dispatch(Event::DisplayInitialized(Err(reason.clone()))),
        Action::EnterSafeMode(reason.clone())
    );

    let mut ui = MicroOs::new();
    ui.dispatch(Event::BootSampled { safe_mode: false });
    ui.dispatch(Event::StorageInitialized(Ok(())));
    ui.dispatch(Event::ProfileValidated(Ok(())));
    ui.dispatch(Event::DisplayInitialized(Ok(())));
    assert_eq!(
        ui.dispatch(Event::SystemUiInitialized(Err(reason.clone()))),
        Action::EnterSafeMode(reason)
    );
}

#[test]
fn settings_navigation_and_invalid_events_are_deterministic() {
    let mut os = MicroOs::new();
    let before = os.state().clone();
    assert_eq!(
        os.dispatch(Event::OpenApp(AppId::Counter)),
        Action::Rejected
    );
    assert_eq!(os.state(), &before);
    boot_to_launcher(&mut os);
    assert_eq!(os.dispatch(Event::OpenSettings), Action::ShowSettings);
    assert_eq!(os.state(), &State::Settings);
    assert_eq!(os.dispatch(Event::BackPressed), Action::ShowLauncher);
    assert_eq!(os.state(), &State::Launcher);
    os.dispatch(Event::OpenSettings);
    assert_eq!(os.dispatch(Event::HomePressed), Action::ShowLauncher);
}

#[test]
fn app_start_stop_waits_for_teardown_and_can_repeat_freshly() {
    let mut os = MicroOs::new();
    boot_to_launcher(&mut os);
    for event in [Event::HomePressed, Event::BackPressed] {
        assert_eq!(
            os.dispatch(Event::OpenApp(AppId::Counter)),
            Action::StartApp(AppId::Counter)
        );
        assert_eq!(os.state(), &State::AppStarting(AppId::Counter));
        assert_eq!(os.dispatch(Event::AppStarted), Action::None);
        assert_eq!(os.state(), &State::AppRunning(AppId::Counter));
        assert_eq!(os.dispatch(event), Action::StopApp(AppId::Counter));
        assert_eq!(os.state(), &State::AppRunning(AppId::Counter));
        assert_eq!(
            os.dispatch(Event::OpenApp(AppId::Counter)),
            Action::Rejected
        );
        assert_eq!(os.dispatch(Event::AppStopped), Action::ShowLauncher);
        assert_eq!(os.state(), &State::Launcher);
    }
}

#[test]
fn counter_failure_uses_trusted_error_ui_and_recovers() {
    let mut os = MicroOs::new();
    boot_to_launcher(&mut os);
    os.dispatch(Event::OpenApp(AppId::Counter));
    let reason = FailureReason::AppCrashed;
    assert_eq!(
        os.dispatch(Event::AppFailed(reason.clone())),
        Action::ShowAppError {
            app: AppId::Counter,
            reason: reason.clone()
        }
    );
    assert_eq!(
        os.state(),
        &State::AppError {
            app: AppId::Counter,
            reason: reason.clone()
        }
    );
    assert_eq!(
        os.dispatch(Event::RestartApp),
        Action::StartApp(AppId::Counter)
    );
    assert_eq!(os.state(), &State::AppStarting(AppId::Counter));
    os.dispatch(Event::AppStarted);
    assert_eq!(os.state(), &State::AppRunning(AppId::Counter));
    assert_eq!(
        os.dispatch(Event::AppFailed(reason.clone())),
        Action::ShowAppError {
            app: AppId::Counter,
            reason: reason.clone()
        }
    );
    assert_eq!(os.dispatch(Event::HomePressed), Action::ShowLauncher);
    assert_eq!(os.state(), &State::Launcher);
}

#[test]
fn wifi_is_saved_before_first_run_completes() {
    let mut os = MicroOs::new();
    boot_to_system_ui(&mut os);
    os.dispatch(Event::NetworkConfigLoaded { configured: false });
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::StartWifiScan);
    assert_eq!(os.wifi_state(), &WifiState::Scanning);
    assert_eq!(os.dispatch(Event::WifiScanCompleted), Action::None);
    assert_eq!(
        os.dispatch(Event::WifiConnectRequested),
        Action::ConnectWifi
    );
    assert_eq!(os.wifi_state(), &WifiState::Connecting);
    assert_eq!(os.dispatch(Event::WifiConnected), Action::PersistWifi);
    assert_eq!(os.wifi_state(), &WifiState::PendingPersistence);
    assert_eq!(os.state(), &State::FirstRunSetup);
    assert!(!os.network_configured());
    assert_eq!(os.dispatch(Event::WifiPersisted), Action::ShowLauncher);
    assert!(os.network_configured());
    assert_eq!(os.wifi_state(), &WifiState::Connected);
    assert_eq!(os.state(), &State::Launcher);
}

#[test]
fn wifi_failures_are_distinct_and_reconnect_is_capped_then_reset() {
    let failures = [
        WifiFailure::Authentication,
        WifiFailure::NetworkMissing,
        WifiFailure::Timeout,
        WifiFailure::Internal,
    ];
    let delays = [1, 2, 5, 10, 30, 30];
    let mut os = MicroOs::new();
    boot_to_system_ui(&mut os);
    os.dispatch(Event::NetworkConfigLoaded { configured: false });
    for (index, delay) in delays.into_iter().enumerate() {
        os.dispatch(Event::WifiConnectRequested);
        let failure = failures[index.min(failures.len() - 1)].clone();
        assert_eq!(
            os.dispatch(Event::WifiFailed(failure.clone())),
            Action::Actions(vec![
                Action::ClearPendingWifi,
                Action::ScheduleWifiReconnect { after_secs: delay }
            ])
        );
        assert_eq!(os.wifi_state(), &WifiState::Failed(failure));
        assert!(!os.network_configured());
        assert_eq!(os.next_reconnect_delay(), delay);
    }
    os.dispatch(Event::WifiConnectRequested);
    os.dispatch(Event::WifiConnected);
    assert_eq!(os.next_reconnect_delay(), 1);
}

#[test]
fn wifi_scan_is_limited_to_setup_and_settings() {
    let mut os = MicroOs::new();
    boot_to_launcher(&mut os);
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
    os.dispatch(Event::OpenSettings);
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::StartWifiScan);
}

#[test]
fn failed_replacement_wifi_keeps_the_active_saved_network() {
    let mut os = MicroOs::new();
    boot_to_launcher(&mut os);
    os.dispatch(Event::OpenSettings);
    os.dispatch(Event::WifiScanRequested);
    os.dispatch(Event::WifiScanCompleted);
    os.dispatch(Event::WifiConnectRequested);

    assert_eq!(
        os.dispatch(Event::WifiFailed(WifiFailure::Authentication)),
        Action::Actions(vec![
            Action::ClearPendingWifi,
            Action::ScheduleWifiReconnect { after_secs: 1 }
        ])
    );
    assert_eq!(os.state(), &State::Settings);
    assert!(os.network_configured());
}

#[test]
fn destructive_actions_require_matching_confirmation() {
    let mut os = MicroOs::new();
    boot_to_launcher(&mut os);
    os.dispatch(Event::OpenSettings);
    let before_invalid_confirmation = os.clone();
    assert_eq!(os.dispatch(Event::ClearNetworkConfirmed), Action::Rejected);
    assert_eq!(os.dispatch(Event::FactoryResetConfirmed), Action::Rejected);
    assert_eq!(os, before_invalid_confirmation);
    assert_eq!(
        os.dispatch(Event::ClearNetworkRequested),
        Action::ConfirmClearNetwork
    );
    assert_eq!(os.dispatch(Event::FactoryResetConfirmed), Action::Rejected);
    assert_eq!(
        os.dispatch(Event::ClearNetworkConfirmed),
        Action::ClearNetwork
    );
    assert_eq!(
        os.dispatch(Event::FactoryResetRequested),
        Action::ConfirmFactoryReset
    );
    assert_eq!(
        os.dispatch(Event::FactoryResetConfirmed),
        Action::FactoryReset
    );
    assert_eq!(os.dispatch(Event::RebootRequested), Action::Reboot);
}
