use micro_os_core::{
    Action, AppId, AppSessionId, Backlight, Event, MicroOs, State, WifiOperationId,
};
use serde::Serialize;

const ACTION_LOG_CAPACITY: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemIntent {
    OpenCounter,
    CounterStarted,
    CounterStopped,
    OpenSettings,
    Back,
    WifiScan,
    WifiConnect,
    WifiConnected,
    WifiPersisted,
    SafeMode,
    ToggleBacklight,
}

impl SystemIntent {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "open-counter" => Some(Self::OpenCounter),
            "counter-started" => Some(Self::CounterStarted),
            "counter-stopped" => Some(Self::CounterStopped),
            "open-settings" => Some(Self::OpenSettings),
            "back" => Some(Self::Back),
            "wifi-scan" => Some(Self::WifiScan),
            "wifi-connect" => Some(Self::WifiConnect),
            "wifi-connected" => Some(Self::WifiConnected),
            "wifi-persisted" => Some(Self::WifiPersisted),
            "safe-mode" => Some(Self::SafeMode),
            "backlight-toggle" => Some(Self::ToggleBacklight),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SystemSnapshot {
    pub screen: String,
    pub wifi: String,
    pub backlight: String,
    pub last_action: String,
    pub actions: Vec<String>,
    pub counter_session: Option<u64>,
}

pub struct SystemShell {
    os: MicroOs,
    active_app: Option<AppSessionId>,
    active_wifi: Option<WifiOperationId>,
    actions: Vec<String>,
    last_action: String,
}

impl SystemShell {
    #[must_use]
    pub fn configured_boot() -> Self {
        let mut shell = Self::new();
        shell.issue(Event::BootSampled { safe_mode: false });
        shell.issue(Event::StorageInitialized(Ok(())));
        shell.issue(Event::ProfileValidated(Ok(())));
        shell.issue(Event::DisplayInitialized(Ok(())));
        shell.issue(Event::SystemUiInitialized(Ok(())));
        let action = shell.issue(Event::NetworkConfigLoaded { configured: true });
        if let Some(operation) = find_saved_wifi_operation(&action) {
            shell.issue(Event::WifiConnected { operation });
        }
        shell
    }

    #[must_use]
    pub fn snapshot(&self) -> SystemSnapshot {
        SystemSnapshot {
            screen: describe_state(self.os.state()),
            wifi: format!("{:?}", self.os.live_wifi_state()),
            backlight: format!("{:?}", self.os.backlight()),
            last_action: self.last_action.clone(),
            actions: self.actions.clone(),
            counter_session: self.active_app.map(|session| session.0),
        }
    }

    pub fn dispatch(&mut self, intent: SystemIntent) -> SystemSnapshot {
        match intent {
            SystemIntent::OpenCounter => {
                let action = self.issue(Event::OpenApp(AppId::Counter));
                self.active_app = find_app_session(&action);
            }
            SystemIntent::CounterStarted => {
                if let Some(session) = self.active_app {
                    self.issue(Event::AppStarted { session });
                } else {
                    self.reject_missing_callback();
                }
            }
            SystemIntent::CounterStopped => {
                if let Some(session) = self.active_app {
                    self.issue(Event::AppStopped { session });
                    if self.os.state() == &State::Launcher {
                        self.active_app = None;
                    }
                } else {
                    self.reject_missing_callback();
                }
            }
            SystemIntent::OpenSettings => {
                self.issue(Event::OpenSettings);
            }
            SystemIntent::Back => {
                self.issue(Event::BackPressed);
            }
            SystemIntent::WifiScan => {
                let action = self.issue(Event::WifiScanRequested);
                if let Some(operation) = find_wifi_operation(&action) {
                    self.active_wifi = Some(operation);
                    self.issue(Event::WifiScanCompleted { operation });
                }
            }
            SystemIntent::WifiConnect => {
                let action = self.issue(Event::WifiConnectRequested);
                self.active_wifi = find_wifi_operation(&action);
            }
            SystemIntent::WifiConnected => {
                if let Some(operation) = self.active_wifi {
                    self.issue(Event::WifiConnected { operation });
                } else {
                    self.reject_missing_callback();
                }
            }
            SystemIntent::WifiPersisted => {
                if let Some(operation) = self.active_wifi {
                    self.issue(Event::WifiPersisted { operation });
                    self.active_wifi = None;
                } else {
                    self.reject_missing_callback();
                }
            }
            SystemIntent::SafeMode => {
                self.os = MicroOs::new();
                self.active_app = None;
                self.active_wifi = None;
                self.issue(Event::BootSampled { safe_mode: true });
            }
            SystemIntent::ToggleBacklight => {
                let backlight = if self.os.backlight() == &Backlight::Off {
                    Backlight::High
                } else {
                    Backlight::Off
                };
                self.issue(Event::SetBacklight(backlight));
            }
        }
        self.snapshot()
    }

    fn new() -> Self {
        Self {
            os: MicroOs::new(),
            active_app: None,
            active_wifi: None,
            actions: Vec::new(),
            last_action: String::new(),
        }
    }

    fn issue(&mut self, event: Event) -> Action {
        let action = self.os.dispatch(event);
        self.remember_action(&action);
        action
    }

    fn reject_missing_callback(&mut self) {
        self.remember_action(&Action::Rejected);
    }

    fn remember_action(&mut self, action: &Action) {
        let description = format!("{action:?}");
        self.last_action = description.clone();
        self.actions.push(description);
        if self.actions.len() > ACTION_LOG_CAPACITY {
            self.actions.remove(0);
        }
    }
}

fn find_saved_wifi_operation(action: &Action) -> Option<WifiOperationId> {
    match action {
        Action::ConnectSavedWifi { operation } => Some(*operation),
        Action::Actions(actions) => actions.iter().find_map(find_saved_wifi_operation),
        _ => None,
    }
}

fn find_wifi_operation(action: &Action) -> Option<WifiOperationId> {
    match action {
        Action::StartWifiScan { operation }
        | Action::ConnectWifi { operation }
        | Action::ConnectSavedWifi { operation } => Some(*operation),
        Action::Actions(actions) => actions.iter().find_map(find_wifi_operation),
        _ => None,
    }
}

fn find_app_session(action: &Action) -> Option<AppSessionId> {
    match action {
        Action::StartApp { session, .. } => Some(*session),
        Action::Actions(actions) => actions.iter().find_map(find_app_session),
        _ => None,
    }
}

fn describe_state(state: &State) -> String {
    match state {
        State::AppStarting { app, session } => format!("AppStarting({app:?}, {})", session.0),
        State::AppRunning { app, session } => format!("AppRunning({app:?}, {})", session.0),
        State::AppStopping { app, session, .. } => format!("AppStopping({app:?}, {})", session.0),
        State::AppError {
            app,
            session,
            reason,
        } => {
            format!("AppError({app:?}, {}, {reason:?})", session.0)
        }
        state => format!("{state:?}"),
    }
}
