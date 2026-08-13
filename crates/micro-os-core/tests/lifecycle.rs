use micro_os_core::{
    Action, AppDestination, AppId, AppSessionId, ConfirmationId, Event, FailureReason,
    LiveWifiState, MicroOs, PendingConfirmation, ProvisioningState, State, WifiFailure,
    WifiOperationId,
};

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
    assert_eq!(
        os.dispatch(Event::DisplayInitialized(Ok(()))),
        Action::InitializeSystemUi
    );
    assert_eq!(
        os.dispatch(Event::SystemUiInitialized(Ok(()))),
        Action::LoadNetworkConfig
    );
    assert_eq!(os.state(), &State::SystemUiReady);
}

fn boot_first_run(os: &mut MicroOs) {
    boot_to_system_ui(os);
    assert_eq!(
        os.dispatch(Event::NetworkConfigLoaded { configured: false }),
        Action::ShowFirstRunSetup
    );
    assert_eq!(os.state(), &State::FirstRunSetup);
}

fn boot_configured(os: &mut MicroOs) -> WifiOperationId {
    boot_to_system_ui(os);
    let Action::ConnectSavedWifi { operation } =
        os.dispatch(Event::NetworkConfigLoaded { configured: true })
    else {
        panic!("configured boot must connect saved WiFi");
    };
    assert_eq!(os.state(), &State::Launcher);
    assert_eq!(os.live_wifi_state(), &LiveWifiState::Connecting(operation));
    operation
}

fn connect_saved(os: &mut MicroOs) {
    let operation = boot_configured(os);
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation }),
        Action::None
    );
    assert_eq!(os.live_wifi_state(), &LiveWifiState::Connected);
}

fn action_session(action: Action) -> AppSessionId {
    let Action::StartApp {
        app: AppId::Counter,
        session,
    } = action
    else {
        panic!("counter must start with a session");
    };
    session
}

#[test]
fn normal_boot_is_strict_and_safe_boot_rejects_apps() {
    let mut os = MicroOs::new();
    assert_eq!(
        os.dispatch(Event::OpenApp(AppId::Counter)),
        Action::Rejected
    );
    boot_first_run(&mut os);
    assert_eq!(os.dispatch(Event::SetupSkipped), Action::ShowLauncher);

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
}

#[test]
fn every_initialization_failure_enters_safe_mode() {
    let reason = FailureReason::HardwareUnavailable;
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
fn confirmation_ids_are_context_bound_single_use_and_mutually_exclusive() {
    let mut os = MicroOs::new();
    connect_saved(&mut os);
    os.dispatch(Event::OpenSettings);
    let Action::ConfirmClearNetwork {
        confirmation: clear,
    } = os.dispatch(Event::ClearNetworkRequested)
    else {
        panic!("clear confirmation expected")
    };
    assert_eq!(
        os.pending_confirmation(),
        Some(&PendingConfirmation::ClearNetwork(clear))
    );
    assert_eq!(
        os.dispatch(Event::ClearNetworkConfirmed {
            confirmation: ConfirmationId(clear.0 + 1)
        }),
        Action::Rejected
    );
    assert_eq!(os.dispatch(Event::BackPressed), Action::ShowLauncher);
    assert_eq!(os.pending_confirmation(), None);
    assert_eq!(
        os.dispatch(Event::ClearNetworkConfirmed {
            confirmation: clear
        }),
        Action::Rejected
    );

    os.dispatch(Event::OpenSettings);
    let Action::ConfirmFactoryReset {
        confirmation: reset,
    } = os.dispatch(Event::FactoryResetRequested)
    else {
        panic!("reset confirmation expected")
    };
    assert_eq!(
        os.pending_confirmation(),
        Some(&PendingConfirmation::FactoryReset(reset))
    );
    assert_eq!(
        os.dispatch(Event::FactoryResetConfirmed {
            confirmation: reset
        }),
        Action::FactoryReset {
            confirmation: reset
        }
    );
    assert_eq!(
        os.dispatch(Event::FactoryResetConfirmed {
            confirmation: reset
        }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::FactoryResetCompleted {
            confirmation: reset,
            result: Ok(())
        }),
        Action::Reboot
    );
    assert_eq!(
        os.dispatch(Event::FactoryResetCompleted {
            confirmation: reset,
            result: Ok(())
        }),
        Action::Rejected
    );
}

#[test]
fn destructive_operations_cannot_overlap() {
    let mut os = MicroOs::new();
    connect_saved(&mut os);
    os.dispatch(Event::OpenSettings);
    let Action::ConfirmClearNetwork { confirmation } = os.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    os.dispatch(Event::ClearNetworkConfirmed { confirmation });
    assert_eq!(os.dispatch(Event::FactoryResetRequested), Action::Rejected);
    os.dispatch(Event::ClearNetworkCompleted {
        confirmation,
        result: Err(FailureReason::Internal),
    });
    let Action::ConfirmFactoryReset { confirmation } = os.dispatch(Event::FactoryResetRequested)
    else {
        panic!()
    };
    os.dispatch(Event::FactoryResetConfirmed { confirmation });
    assert_eq!(os.dispatch(Event::ClearNetworkRequested), Action::Rejected);
}

#[test]
fn clear_network_changes_saved_state_only_after_matching_success() {
    let mut os = MicroOs::new();
    connect_saved(&mut os);
    os.dispatch(Event::OpenSettings);
    let Action::ConfirmClearNetwork { confirmation } = os.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    assert_eq!(
        os.dispatch(Event::ClearNetworkConfirmed { confirmation }),
        Action::ClearNetwork { confirmation }
    );
    assert!(os.network_configured());
    assert_eq!(
        os.dispatch(Event::ClearNetworkCompleted {
            confirmation: ConfirmationId(99),
            result: Ok(())
        }),
        Action::Rejected
    );
    assert!(os.network_configured());
    assert_eq!(
        os.dispatch(Event::ClearNetworkCompleted {
            confirmation,
            result: Err(FailureReason::Internal)
        }),
        Action::None
    );
    assert!(os.network_configured());
    assert_eq!(os.state(), &State::Settings);

    let Action::ConfirmClearNetwork { confirmation } = os.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    os.dispatch(Event::ClearNetworkConfirmed { confirmation });
    assert_eq!(
        os.dispatch(Event::ClearNetworkCompleted {
            confirmation,
            result: Ok(())
        }),
        Action::ShowFirstRunSetup
    );
    assert!(!os.network_configured());
    assert_eq!(os.state(), &State::FirstRunSetup);
}

#[test]
fn safe_mode_can_clear_network_and_never_leaves_safe_mode() {
    for result in [Err(FailureReason::Internal), Ok(())] {
        let mut os = MicroOs::new();
        os.dispatch(Event::BootSampled { safe_mode: true });
        let Action::ConfirmClearNetwork { confirmation } =
            os.dispatch(Event::ClearNetworkRequested)
        else {
            panic!()
        };
        assert_eq!(
            os.dispatch(Event::ClearNetworkConfirmed { confirmation }),
            Action::ClearNetwork { confirmation }
        );
        assert_eq!(
            os.dispatch(Event::ClearNetworkCompleted {
                confirmation,
                result
            }),
            Action::None
        );
        assert_eq!(os.state(), &State::SafeMode);
    }
}

#[test]
fn configured_boot_connects_saved_wifi_without_blocking_launcher() {
    let mut os = MicroOs::new();
    let operation = boot_configured(&mut os);
    assert!(os.network_configured());
    assert_eq!(os.dispatch(Event::OpenSettings), Action::ShowSettings);
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation }),
        Action::None
    );
    assert_eq!(os.live_wifi_state(), &LiveWifiState::Connected);
}

#[test]
fn stale_wifi_attempt_and_persist_callbacks_cannot_promote_credentials() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::ConnectWifi { operation: first } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    let Action::ConnectWifi { operation: second } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    assert_ne!(first, second);
    assert_eq!(
        os.dispatch(Event::WifiFailed {
            operation: first,
            reason: WifiFailure::Timeout
        }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation: first }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation: second }),
        Action::PersistWifi { operation: second }
    );
    assert_eq!(
        os.dispatch(Event::WifiPersisted { operation: first }),
        Action::Rejected
    );
    assert!(!os.network_configured());
    assert_eq!(
        os.dispatch(Event::WifiPersisted { operation: second }),
        Action::ShowLauncher
    );
    assert!(os.network_configured());
}

#[test]
fn persistence_cannot_be_overwritten_by_new_provisioning() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::ConnectWifi { operation } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    os.dispatch(Event::WifiConnected { operation });
    assert_eq!(
        os.provisioning_state(),
        &ProvisioningState::Persisting(operation)
    );
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::WifiConnectRequested), Action::Rejected);
    assert_eq!(
        os.dispatch(Event::WifiPersisted { operation }),
        Action::ShowLauncher
    );
}

#[test]
fn stale_scan_completion_is_rejected() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::StartWifiScan { operation: first } = os.dispatch(Event::WifiScanRequested) else {
        panic!()
    };
    let Action::StartWifiScan { operation: second } = os.dispatch(Event::WifiScanRequested) else {
        panic!()
    };
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted { operation: first }),
        Action::Rejected
    );
    assert_eq!(
        os.provisioning_state(),
        &ProvisioningState::Scanning(second)
    );
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted { operation: second }),
        Action::None
    );
}

#[test]
fn replacement_scan_and_failure_preserve_active_link_and_credentials() {
    let mut os = MicroOs::new();
    connect_saved(&mut os);
    os.dispatch(Event::OpenSettings);
    let Action::StartWifiScan { operation: scan } = os.dispatch(Event::WifiScanRequested) else {
        panic!()
    };
    assert_eq!(os.live_wifi_state(), &LiveWifiState::Connected);
    os.dispatch(Event::WifiScanCompleted { operation: scan });
    let Action::ConnectWifi { operation } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    assert_eq!(os.live_wifi_state(), &LiveWifiState::Connected);
    let action = os.dispatch(Event::WifiFailed {
        operation,
        reason: WifiFailure::Authentication,
    });
    let Action::Actions(actions) = action else {
        panic!()
    };
    assert!(actions.contains(&Action::ClearPendingWifi { operation }));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ScheduleWifiReconnect { after_secs: 1, .. }))
    );
    assert!(os.network_configured());
    assert_eq!(os.live_wifi_state(), &LiveWifiState::Connected);
    assert_eq!(os.state(), &State::Settings);
}

#[test]
fn reconnect_due_is_screen_independent_tokened_and_resets_backoff() {
    let mut os = MicroOs::new();
    let mut operation = boot_configured(&mut os);
    let delays = [1, 2, 5, 10, 30, 30];
    for delay in delays {
        let Action::ScheduleWifiReconnect {
            reconnect,
            after_secs,
        } = os.dispatch(Event::WifiFailed {
            operation,
            reason: WifiFailure::Timeout,
        })
        else {
            panic!("saved failure must schedule reconnect")
        };
        assert_eq!(after_secs, delay);
        assert_eq!(os.state(), &State::Launcher);
        let Action::ConnectSavedWifi { operation: next } =
            os.dispatch(Event::ReconnectDue { reconnect })
        else {
            panic!()
        };
        assert_eq!(
            os.dispatch(Event::ReconnectDue { reconnect }),
            Action::Rejected
        );
        operation = next;
    }
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation }),
        Action::None
    );
    let Action::ConnectSavedWifi { operation: retry } = os.dispatch(Event::ReconnectNowRequested)
    else {
        panic!()
    };
    let Action::ScheduleWifiReconnect { after_secs, .. } = os.dispatch(Event::WifiFailed {
        operation: retry,
        reason: WifiFailure::Timeout,
    }) else {
        panic!()
    };
    assert_eq!(after_secs, 1);
}

#[test]
fn replacement_provisioning_invalidates_pending_saved_reconnect() {
    let mut os = MicroOs::new();
    let operation = boot_configured(&mut os);
    let Action::ScheduleWifiReconnect { reconnect, .. } = os.dispatch(Event::WifiFailed {
        operation,
        reason: WifiFailure::Timeout,
    }) else {
        panic!()
    };
    os.dispatch(Event::OpenSettings);
    let Action::ConnectWifi { .. } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    assert_eq!(
        os.dispatch(Event::ReconnectDue { reconnect }),
        Action::Rejected
    );
}

#[test]
fn app_stop_ignores_late_failure_and_requires_matching_session() {
    let mut os = MicroOs::new();
    boot_configured(&mut os);
    let session = action_session(os.dispatch(Event::OpenApp(AppId::Counter)));
    os.dispatch(Event::AppStarted { session });
    assert_eq!(
        os.dispatch(Event::HomePressed),
        Action::StopApp {
            app: AppId::Counter,
            session
        }
    );
    assert_eq!(
        os.state(),
        &State::AppStopping {
            app: AppId::Counter,
            session,
            destination: AppDestination::Launcher
        }
    );
    assert_eq!(
        os.dispatch(Event::AppFailed {
            session,
            reason: FailureReason::AppCrashed
        }),
        Action::None
    );
    assert_eq!(
        os.state(),
        &State::AppStopping {
            app: AppId::Counter,
            session,
            destination: AppDestination::Launcher
        }
    );
    assert_eq!(
        os.dispatch(Event::AppStopped {
            session: AppSessionId(session.0 + 1)
        }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::AppStopped { session }),
        Action::ShowLauncher
    );
    assert_eq!(os.state(), &State::Launcher);
}

#[test]
fn restarted_app_rejects_old_session_callbacks() {
    let mut os = MicroOs::new();
    boot_configured(&mut os);
    let old = action_session(os.dispatch(Event::OpenApp(AppId::Counter)));
    os.dispatch(Event::AppFailed {
        session: old,
        reason: FailureReason::AppCrashed,
    });
    let new = action_session(os.dispatch(Event::RestartApp));
    assert_ne!(old, new);
    assert_eq!(
        os.dispatch(Event::AppStarted { session: old }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::AppFailed {
            session: old,
            reason: FailureReason::Internal
        }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::AppStarted { session: new }),
        Action::None
    );
    assert_eq!(
        os.state(),
        &State::AppRunning {
            app: AppId::Counter,
            session: new
        }
    );
}
