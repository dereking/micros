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
    let Action::Actions(actions) = os.dispatch(Event::NetworkConfigLoaded { configured: true })
    else {
        panic!("configured boot must expose UI and connection effects");
    };
    assert_eq!(actions[0], Action::ShowLauncher);
    let Action::ConnectSavedWifi { operation } = actions[1] else {
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
fn configured_boot_explicitly_shows_launcher_and_connects_saved_wifi() {
    let mut os = MicroOs::new();
    boot_to_system_ui(&mut os);
    let action = os.dispatch(Event::NetworkConfigLoaded { configured: true });
    let Action::Actions(actions) = action else {
        panic!("configured boot must expose both UI and connection effects")
    };
    assert_eq!(actions.len(), 2);
    assert_eq!(actions[0], Action::ShowLauncher);
    assert!(matches!(actions[1], Action::ConnectSavedWifi { .. }));
    assert_eq!(os.state(), &State::Launcher);
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
fn pending_destructive_confirmation_rejects_cross_requests_both_ways() {
    let mut clear_first = MicroOs::new();
    connect_saved(&mut clear_first);
    clear_first.dispatch(Event::OpenSettings);
    let clear = clear_first.dispatch(Event::ClearNetworkRequested);
    assert!(matches!(clear, Action::ConfirmClearNetwork { .. }));
    assert_eq!(
        clear_first.dispatch(Event::FactoryResetRequested),
        Action::Rejected
    );
    assert!(matches!(
        clear_first.pending_confirmation(),
        Some(PendingConfirmation::ClearNetwork(_))
    ));

    let mut reset_first = MicroOs::new();
    connect_saved(&mut reset_first);
    reset_first.dispatch(Event::OpenSettings);
    let reset = reset_first.dispatch(Event::FactoryResetRequested);
    assert!(matches!(reset, Action::ConfirmFactoryReset { .. }));
    assert_eq!(
        reset_first.dispatch(Event::ClearNetworkRequested),
        Action::Rejected
    );
    assert!(matches!(
        reset_first.pending_confirmation(),
        Some(PendingConfirmation::FactoryReset(_))
    ));
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
fn concurrent_wifi_attempt_is_rejected_without_invalidating_active_operation() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::ConnectWifi { operation: first } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    assert_eq!(os.dispatch(Event::WifiConnectRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
    let stale = WifiOperationId(first.0 + 1);
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation: stale }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::WifiFailed {
            operation: stale,
            reason: WifiFailure::Timeout
        }),
        Action::Rejected
    );
    assert_eq!(
        os.provisioning_state(),
        &ProvisioningState::ConnectingReplacement(first)
    );
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation: first }),
        Action::PersistWifi { operation: first }
    );
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
fn destructive_requests_are_rejected_while_wifi_persistence_is_in_flight() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::ConnectWifi { operation } = os.dispatch(Event::WifiConnectRequested) else {
        panic!()
    };
    os.dispatch(Event::WifiConnected { operation });
    let before = os.clone();
    assert_eq!(os.dispatch(Event::ClearNetworkRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::FactoryResetRequested), Action::Rejected);
    assert_eq!(os, before);
}

#[test]
fn concurrent_scan_is_rejected_and_active_scan_completion_remains_valid() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::StartWifiScan { operation: first } = os.dispatch(Event::WifiScanRequested) else {
        panic!()
    };
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::WifiConnectRequested), Action::Rejected);
    assert_eq!(os.provisioning_state(), &ProvisioningState::Scanning(first));
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted {
            operation: WifiOperationId(first.0 + 1)
        }),
        Action::Rejected
    );
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted { operation: first }),
        Action::None
    );
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted { operation: first }),
        Action::Rejected
    );
}

#[test]
fn scan_failure_is_tokened_and_releases_the_radio() {
    let mut os = MicroOs::new();
    boot_first_run(&mut os);
    let Action::StartWifiScan { operation } = os.dispatch(Event::WifiScanRequested) else {
        panic!()
    };
    let stale = WifiOperationId(operation.0 + 1);
    assert_eq!(
        os.dispatch(Event::WifiScanFailed {
            operation: stale,
            reason: WifiFailure::Internal
        }),
        Action::Rejected
    );
    assert_eq!(
        os.provisioning_state(),
        &ProvisioningState::Scanning(operation)
    );
    assert_eq!(
        os.dispatch(Event::WifiScanFailed {
            operation,
            reason: WifiFailure::Internal
        }),
        Action::None
    );
    assert!(matches!(
        os.provisioning_state(),
        ProvisioningState::Failed {
            operation: failed,
            reason: WifiFailure::Internal
        } if *failed == operation
    ));
    assert!(matches!(
        os.dispatch(Event::WifiScanRequested),
        Action::StartWifiScan { .. }
    ));
}

#[test]
fn replacement_is_rejected_while_saved_wifi_connect_is_in_flight() {
    let mut os = MicroOs::new();
    let operation = boot_configured(&mut os);
    os.dispatch(Event::OpenSettings);
    let before = os.clone();
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::WifiConnectRequested), Action::Rejected);
    assert_eq!(os, before);
    assert_eq!(
        os.dispatch(Event::WifiConnected { operation }),
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
    assert_eq!(
        os.dispatch(Event::WifiFailed {
            operation,
            reason: WifiFailure::Authentication,
        }),
        Action::ClearPendingWifi { operation }
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
    let Action::ConnectWifi {
        operation: replacement,
    } = os.dispatch(Event::WifiConnectRequested)
    else {
        panic!()
    };
    assert_eq!(
        os.dispatch(Event::ReconnectDue { reconnect }),
        Action::Rejected
    );
    let Action::Actions(actions) = os.dispatch(Event::WifiFailed {
        operation: replacement,
        reason: WifiFailure::Authentication,
    }) else {
        panic!()
    };
    assert!(actions.contains(&Action::ClearPendingWifi {
        operation: replacement
    }));
    let Some(Action::ScheduleWifiReconnect { reconnect, .. }) = actions
        .iter()
        .find(|action| matches!(action, Action::ScheduleWifiReconnect { .. }))
    else {
        panic!("disconnected saved network must be recoverable")
    };
    assert!(matches!(
        os.dispatch(Event::ReconnectDue {
            reconnect: *reconnect
        }),
        Action::ConnectSavedWifi { .. }
    ));
}

#[test]
fn destructive_operations_freeze_trusted_context_until_completion() {
    let mut clear = MicroOs::new();
    connect_saved(&mut clear);
    clear.dispatch(Event::OpenSettings);
    let Action::ConfirmClearNetwork { confirmation } = clear.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    clear.dispatch(Event::ClearNetworkConfirmed { confirmation });
    assert_eq!(clear.dispatch(Event::BackPressed), Action::Rejected);
    assert_eq!(
        clear.dispatch(Event::OpenApp(AppId::Counter)),
        Action::Rejected
    );
    assert_eq!(clear.state(), &State::Settings);
    assert_eq!(
        clear.dispatch(Event::ClearNetworkCompleted {
            confirmation,
            result: Err(FailureReason::Internal)
        }),
        Action::None
    );
    assert_eq!(clear.dispatch(Event::BackPressed), Action::ShowLauncher);

    let mut reset = MicroOs::new();
    connect_saved(&mut reset);
    reset.dispatch(Event::OpenSettings);
    let Action::ConfirmFactoryReset { confirmation } = reset.dispatch(Event::FactoryResetRequested)
    else {
        panic!()
    };
    reset.dispatch(Event::FactoryResetConfirmed { confirmation });
    assert_eq!(reset.dispatch(Event::HomePressed), Action::Rejected);
    assert_eq!(
        reset.dispatch(Event::OpenApp(AppId::Counter)),
        Action::Rejected
    );
    assert_eq!(reset.state(), &State::Settings);
    assert_eq!(
        reset.dispatch(Event::FactoryResetCompleted {
            confirmation,
            result: Err(FailureReason::Internal)
        }),
        Action::None
    );
    assert_eq!(reset.dispatch(Event::HomePressed), Action::ShowLauncher);
}

#[test]
fn destructive_execution_blocks_wifi_and_cancels_pending_reconnect() {
    let mut clear = MicroOs::new();
    let failed_operation = boot_configured(&mut clear);
    let Action::ScheduleWifiReconnect { reconnect, .. } = clear.dispatch(Event::WifiFailed {
        operation: failed_operation,
        reason: WifiFailure::Timeout,
    }) else {
        panic!()
    };
    clear.dispatch(Event::OpenSettings);
    let Action::ConfirmClearNetwork { confirmation } = clear.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    assert_eq!(
        clear.dispatch(Event::ReconnectDue { reconnect }),
        Action::Rejected
    );
    clear.dispatch(Event::ClearNetworkConfirmed { confirmation });
    assert_eq!(clear.dispatch(Event::WifiScanRequested), Action::Rejected);
    assert_eq!(
        clear.dispatch(Event::WifiConnectRequested),
        Action::Rejected
    );
    assert_eq!(
        clear.dispatch(Event::ReconnectDue { reconnect }),
        Action::Rejected
    );
    assert_eq!(
        clear.dispatch(Event::ReconnectNowRequested),
        Action::Rejected
    );
    assert_eq!(
        clear.dispatch(Event::WifiConnected {
            operation: failed_operation
        }),
        Action::Rejected
    );
    clear.dispatch(Event::ClearNetworkCompleted {
        confirmation,
        result: Err(FailureReason::Internal),
    });

    let mut reset = MicroOs::new();
    connect_saved(&mut reset);
    reset.dispatch(Event::OpenSettings);
    let Action::ConfirmFactoryReset { confirmation } = reset.dispatch(Event::FactoryResetRequested)
    else {
        panic!()
    };
    reset.dispatch(Event::FactoryResetConfirmed { confirmation });
    assert_eq!(reset.dispatch(Event::WifiScanRequested), Action::Rejected);
    assert_eq!(
        reset.dispatch(Event::WifiConnectRequested),
        Action::Rejected
    );
    assert_eq!(
        reset.dispatch(Event::ReconnectNowRequested),
        Action::Rejected
    );
    reset.dispatch(Event::FactoryResetCompleted {
        confirmation,
        result: Err(FailureReason::Internal),
    });
}

#[test]
fn pending_destructive_confirmation_prevents_wifi_from_racing_confirm() {
    let mut os = MicroOs::new();
    connect_saved(&mut os);
    os.dispatch(Event::OpenSettings);
    let Action::ConfirmClearNetwork { confirmation } = os.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::WifiConnectRequested), Action::Rejected);
    assert_eq!(os.dispatch(Event::ReconnectNowRequested), Action::Rejected);
    assert_eq!(
        os.dispatch(Event::ClearNetworkConfirmed { confirmation }),
        Action::ClearNetwork { confirmation }
    );
}

#[test]
fn completed_wifi_token_remains_stale_across_destructive_confirmation() {
    let mut os = MicroOs::new();
    connect_saved(&mut os);
    os.dispatch(Event::OpenSettings);
    let Action::StartWifiScan { operation } = os.dispatch(Event::WifiScanRequested) else {
        panic!()
    };
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted { operation }),
        Action::None
    );
    let Action::ConfirmClearNetwork { confirmation } = os.dispatch(Event::ClearNetworkRequested)
    else {
        panic!()
    };
    assert_eq!(
        os.dispatch(Event::WifiScanCompleted { operation }),
        Action::Rejected
    );
    os.dispatch(Event::ClearNetworkConfirmed { confirmation });
    assert_eq!(
        os.dispatch(Event::WifiScanFailed {
            operation,
            reason: WifiFailure::Internal
        }),
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

#[test]
fn app_lifecycle_repeats_with_back_teardown_and_fresh_sessions() {
    let mut os = MicroOs::new();
    boot_configured(&mut os);
    let first = action_session(os.dispatch(Event::OpenApp(AppId::Counter)));
    os.dispatch(Event::AppStarted { session: first });
    assert_eq!(
        os.dispatch(Event::BackPressed),
        Action::StopApp {
            app: AppId::Counter,
            session: first
        }
    );
    assert_eq!(
        os.dispatch(Event::AppStopped { session: first }),
        Action::ShowLauncher
    );
    let second = action_session(os.dispatch(Event::OpenApp(AppId::Counter)));
    assert_ne!(first, second);
    os.dispatch(Event::AppStarted { session: second });
    assert_eq!(
        os.state(),
        &State::AppRunning {
            app: AppId::Counter,
            session: second
        }
    );
}

#[test]
fn trusted_app_error_and_settings_home_return_to_launcher() {
    let mut os = MicroOs::new();
    boot_configured(&mut os);
    let session = action_session(os.dispatch(Event::OpenApp(AppId::Counter)));
    os.dispatch(Event::AppFailed {
        session,
        reason: FailureReason::AppCrashed,
    });
    assert_eq!(os.dispatch(Event::HomePressed), Action::ShowLauncher);
    assert_eq!(os.state(), &State::Launcher);
    os.dispatch(Event::OpenSettings);
    assert_eq!(os.dispatch(Event::HomePressed), Action::ShowLauncher);
    assert_eq!(os.state(), &State::Launcher);
}

#[test]
fn all_wifi_failure_reasons_retain_operation_identity() {
    for reason in [
        WifiFailure::Timeout,
        WifiFailure::Authentication,
        WifiFailure::NetworkMissing,
        WifiFailure::Internal,
    ] {
        let mut os = MicroOs::new();
        boot_first_run(&mut os);
        let Action::ConnectWifi { operation } = os.dispatch(Event::WifiConnectRequested) else {
            panic!()
        };
        assert_eq!(
            os.dispatch(Event::WifiFailed {
                operation,
                reason: reason.clone(),
            }),
            Action::ClearPendingWifi { operation }
        );
        assert!(matches!(
            os.provisioning_state(),
            ProvisioningState::Failed {
                operation: failed_operation,
                reason: failed_reason
            } if *failed_operation == operation && *failed_reason == reason
        ));
    }
}
