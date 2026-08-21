use micro_host_web::{SystemIntent, SystemShell};

#[test]
fn system_intent_parser_has_a_closed_public_surface() {
    assert_eq!(SystemIntent::parse("open-counter"), Some(SystemIntent::OpenCounter));
    assert_eq!(SystemIntent::parse("erase-everything"), None);
}

#[test]
fn configured_boot_reaches_connected_launcher_and_records_actions() {
    let shell = SystemShell::configured_boot();
    let snapshot = shell.snapshot();

    assert_eq!(snapshot.screen, "Launcher");
    assert_eq!(snapshot.wifi, "Connected");
    assert_eq!(snapshot.backlight, "High");
    assert!(snapshot
        .actions
        .iter()
        .any(|action| action.contains("ConnectSavedWifi")));
}

#[test]
fn counter_start_and_stop_use_the_reducer_session() {
    let mut shell = SystemShell::configured_boot();

    let starting = shell.dispatch(SystemIntent::OpenCounter);
    assert_eq!(starting.screen, "AppStarting(Counter, 1)");
    assert_eq!(starting.counter_session, Some(1));

    let running = shell.dispatch(SystemIntent::CounterStarted);
    assert_eq!(running.screen, "AppRunning(Counter, 1)");

    let stopping = shell.dispatch(SystemIntent::Back);
    assert_eq!(stopping.screen, "AppStopping(Counter, 1)");

    assert_eq!(
        shell.dispatch(SystemIntent::CounterStopped).screen,
        "Launcher"
    );
}

#[test]
fn system_shell_delegates_backlight_to_the_os_reducer() {
    let mut shell = SystemShell::configured_boot();

    let snapshot = shell.dispatch(SystemIntent::ToggleBacklight);
    assert_eq!(snapshot.backlight, "Off");
    assert_eq!(snapshot.last_action, "ApplyBacklight(Off)");
}
