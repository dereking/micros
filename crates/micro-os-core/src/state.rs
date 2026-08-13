use crate::{WifiFailure, WifiState};

const RECONNECT_DELAYS: [u32; 5] = [1, 2, 5, 10, 30];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppId {
    Counter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FailureReason {
    SafeModeRequested,
    StorageCorrupt,
    InvalidBoardProfile,
    HardwareUnavailable,
    AppCrashed,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Language {
    English,
    SimplifiedChinese,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backlight {
    Off,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ScreenTimeout {
    Never,
    Seconds(u32),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum State {
    EarlyBoot,
    SafeMode,
    StorageReady,
    BoardProfileValidated,
    DisplayReady,
    SystemUiReady,
    FirstRunSetup,
    Launcher,
    AppStarting(AppId),
    AppRunning(AppId),
    AppError { app: AppId, reason: FailureReason },
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    BootSampled { safe_mode: bool },
    StorageInitialized(Result<(), FailureReason>),
    ProfileValidated(Result<(), FailureReason>),
    DisplayInitialized(Result<(), FailureReason>),
    SystemUiInitialized(Result<(), FailureReason>),
    NetworkConfigLoaded { configured: bool },
    SetupSkipped,
    OpenSettings,
    BackPressed,
    HomePressed,
    OpenApp(AppId),
    AppStarted,
    AppFailed(FailureReason),
    RestartApp,
    AppStopped,
    WifiScanRequested,
    WifiScanCompleted,
    WifiConnectRequested,
    WifiConnected,
    WifiPersisted,
    WifiFailed(WifiFailure),
    ClearNetworkRequested,
    ClearNetworkConfirmed,
    FactoryResetRequested,
    FactoryResetConfirmed,
    RebootRequested,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    None,
    Rejected,
    Actions(Vec<Action>),
    EnterSafeMode(FailureReason),
    InitializeStorage,
    ValidateProfile,
    InitializeDisplay,
    InitializeSystemUi,
    LoadNetworkConfig,
    ShowFirstRunSetup,
    ShowLauncher,
    ShowSettings,
    StartWifiScan,
    ConnectWifi,
    PersistWifi,
    ClearPendingWifi,
    ScheduleWifiReconnect { after_secs: u32 },
    StartApp(AppId),
    StopApp(AppId),
    ShowAppError { app: AppId, reason: FailureReason },
    ConfirmClearNetwork,
    ClearNetwork,
    ConfirmFactoryReset,
    FactoryReset,
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroOs {
    state: State,
    wifi_state: WifiState,
    network_configured: bool,
    boot_started: bool,
    app_stop_pending: bool,
    clear_network_pending: bool,
    factory_reset_pending: bool,
    reconnect_index: usize,
    last_reconnect_delay: u32,
}

impl Default for MicroOs {
    fn default() -> Self {
        Self::new()
    }
}

impl MicroOs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: State::EarlyBoot,
            wifi_state: WifiState::Idle,
            network_configured: false,
            boot_started: false,
            app_stop_pending: false,
            clear_network_pending: false,
            factory_reset_pending: false,
            reconnect_index: 0,
            last_reconnect_delay: RECONNECT_DELAYS[0],
        }
    }

    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    #[must_use]
    pub fn wifi_state(&self) -> &WifiState {
        &self.wifi_state
    }

    #[must_use]
    pub fn network_configured(&self) -> bool {
        self.network_configured
    }

    #[must_use]
    pub fn next_reconnect_delay(&self) -> u32 {
        self.last_reconnect_delay
    }

    pub fn dispatch(&mut self, event: Event) -> Action {
        match event {
            Event::BootSampled { safe_mode } => self.boot_sampled(safe_mode),
            Event::StorageInitialized(result) => self.storage_initialized(result),
            Event::ProfileValidated(result) => self.profile_validated(result),
            Event::DisplayInitialized(result) => self.display_initialized(result),
            Event::SystemUiInitialized(result) => self.system_ui_initialized(result),
            Event::NetworkConfigLoaded { configured } => self.network_config_loaded(configured),
            Event::SetupSkipped => self.setup_skipped(),
            Event::OpenSettings => self.open_settings(),
            Event::BackPressed | Event::HomePressed => self.go_home(),
            Event::OpenApp(app) => self.open_app(app),
            Event::AppStarted => self.app_started(),
            Event::AppFailed(reason) => self.app_failed(reason),
            Event::RestartApp => self.restart_app(),
            Event::AppStopped => self.app_stopped(),
            Event::WifiScanRequested => self.wifi_scan_requested(),
            Event::WifiScanCompleted => self.wifi_scan_completed(),
            Event::WifiConnectRequested => self.wifi_connect_requested(),
            Event::WifiConnected => self.wifi_connected(),
            Event::WifiPersisted => self.wifi_persisted(),
            Event::WifiFailed(reason) => self.wifi_failed(reason),
            Event::ClearNetworkRequested => self.clear_network_requested(),
            Event::ClearNetworkConfirmed => self.clear_network_confirmed(),
            Event::FactoryResetRequested => self.factory_reset_requested(),
            Event::FactoryResetConfirmed => self.factory_reset_confirmed(),
            Event::RebootRequested => Action::Reboot,
        }
    }

    fn boot_sampled(&mut self, safe_mode: bool) -> Action {
        if self.state != State::EarlyBoot || self.boot_started {
            return Action::Rejected;
        }
        self.boot_started = true;
        if safe_mode {
            self.enter_safe_mode(FailureReason::SafeModeRequested)
        } else {
            Action::InitializeStorage
        }
    }

    fn storage_initialized(&mut self, result: Result<(), FailureReason>) -> Action {
        if self.state != State::EarlyBoot || !self.boot_started {
            return Action::Rejected;
        }
        match result {
            Ok(()) => {
                self.state = State::StorageReady;
                Action::ValidateProfile
            }
            Err(reason) => self.enter_safe_mode(reason),
        }
    }

    fn profile_validated(&mut self, result: Result<(), FailureReason>) -> Action {
        if self.state != State::StorageReady {
            return Action::Rejected;
        }
        match result {
            Ok(()) => {
                self.state = State::BoardProfileValidated;
                Action::InitializeDisplay
            }
            Err(reason) => self.enter_safe_mode(reason),
        }
    }

    fn display_initialized(&mut self, result: Result<(), FailureReason>) -> Action {
        if self.state != State::BoardProfileValidated {
            return Action::Rejected;
        }
        match result {
            Ok(()) => {
                self.state = State::DisplayReady;
                Action::InitializeSystemUi
            }
            Err(reason) => self.enter_safe_mode(reason),
        }
    }

    fn system_ui_initialized(&mut self, result: Result<(), FailureReason>) -> Action {
        if self.state != State::DisplayReady {
            return Action::Rejected;
        }
        match result {
            Ok(()) => {
                self.state = State::SystemUiReady;
                Action::LoadNetworkConfig
            }
            Err(reason) => self.enter_safe_mode(reason),
        }
    }

    fn network_config_loaded(&mut self, configured: bool) -> Action {
        if self.state != State::SystemUiReady {
            return Action::Rejected;
        }
        self.network_configured = configured;
        if configured {
            self.wifi_state = WifiState::Connected;
            self.state = State::Launcher;
            Action::ShowLauncher
        } else {
            self.wifi_state = WifiState::Idle;
            self.state = State::FirstRunSetup;
            Action::ShowFirstRunSetup
        }
    }

    fn setup_skipped(&mut self) -> Action {
        if self.state != State::FirstRunSetup {
            return Action::Rejected;
        }
        self.state = State::Launcher;
        Action::ShowLauncher
    }

    fn open_settings(&mut self) -> Action {
        if self.state != State::Launcher {
            return Action::Rejected;
        }
        self.state = State::Settings;
        Action::ShowSettings
    }

    fn go_home(&mut self) -> Action {
        match self.state.clone() {
            State::Settings => {
                self.state = State::Launcher;
                Action::ShowLauncher
            }
            State::AppStarting(app) | State::AppRunning(app) => {
                if self.app_stop_pending {
                    Action::Rejected
                } else {
                    self.app_stop_pending = true;
                    Action::StopApp(app)
                }
            }
            State::AppError { .. } => {
                self.state = State::Launcher;
                Action::ShowLauncher
            }
            State::Launcher => Action::None,
            _ => Action::Rejected,
        }
    }

    fn open_app(&mut self, app: AppId) -> Action {
        if self.state != State::Launcher {
            return Action::Rejected;
        }
        self.app_stop_pending = false;
        self.state = State::AppStarting(app.clone());
        Action::StartApp(app)
    }

    fn app_started(&mut self) -> Action {
        if self.app_stop_pending {
            return Action::Rejected;
        }
        let State::AppStarting(app) = self.state.clone() else {
            return Action::Rejected;
        };
        self.state = State::AppRunning(app);
        Action::None
    }

    fn app_failed(&mut self, reason: FailureReason) -> Action {
        let app = match self.state.clone() {
            State::AppStarting(app) | State::AppRunning(app) => app,
            _ => return Action::Rejected,
        };
        self.app_stop_pending = false;
        self.state = State::AppError {
            app: app.clone(),
            reason: reason.clone(),
        };
        Action::ShowAppError { app, reason }
    }

    fn restart_app(&mut self) -> Action {
        let State::AppError { app, .. } = self.state.clone() else {
            return Action::Rejected;
        };
        self.app_stop_pending = false;
        self.state = State::AppStarting(app.clone());
        Action::StartApp(app)
    }

    fn app_stopped(&mut self) -> Action {
        if !self.app_stop_pending
            || !matches!(self.state, State::AppStarting(_) | State::AppRunning(_))
        {
            return Action::Rejected;
        }
        self.app_stop_pending = false;
        self.state = State::Launcher;
        Action::ShowLauncher
    }

    fn wifi_context(&self) -> bool {
        matches!(self.state, State::FirstRunSetup | State::Settings)
    }

    fn wifi_scan_requested(&mut self) -> Action {
        if !self.wifi_context()
            || !matches!(
                self.wifi_state,
                WifiState::Idle | WifiState::Connected | WifiState::Failed(_)
            )
        {
            return Action::Rejected;
        }
        self.wifi_state = WifiState::Scanning;
        Action::StartWifiScan
    }

    fn wifi_scan_completed(&mut self) -> Action {
        if self.wifi_state != WifiState::Scanning {
            return Action::Rejected;
        }
        self.wifi_state = WifiState::Idle;
        Action::None
    }

    fn wifi_connect_requested(&mut self) -> Action {
        if !self.wifi_context()
            || !matches!(self.wifi_state, WifiState::Idle | WifiState::Failed(_))
        {
            return Action::Rejected;
        }
        self.wifi_state = WifiState::Connecting;
        Action::ConnectWifi
    }

    fn wifi_connected(&mut self) -> Action {
        if self.wifi_state != WifiState::Connecting {
            return Action::Rejected;
        }
        self.reconnect_index = 0;
        self.last_reconnect_delay = RECONNECT_DELAYS[0];
        self.wifi_state = WifiState::PendingPersistence;
        Action::PersistWifi
    }

    fn wifi_persisted(&mut self) -> Action {
        if self.wifi_state != WifiState::PendingPersistence {
            return Action::Rejected;
        }
        self.network_configured = true;
        self.wifi_state = WifiState::Connected;
        if self.state == State::FirstRunSetup {
            self.state = State::Launcher;
            Action::ShowLauncher
        } else {
            Action::None
        }
    }

    fn wifi_failed(&mut self, reason: WifiFailure) -> Action {
        if !matches!(
            self.wifi_state,
            WifiState::Connecting | WifiState::PendingPersistence
        ) {
            return Action::Rejected;
        }
        self.wifi_state = WifiState::Failed(reason);
        let delay = RECONNECT_DELAYS[self.reconnect_index];
        self.last_reconnect_delay = delay;
        self.reconnect_index = (self.reconnect_index + 1).min(RECONNECT_DELAYS.len() - 1);
        Action::Actions(vec![
            Action::ClearPendingWifi,
            Action::ScheduleWifiReconnect { after_secs: delay },
        ])
    }

    fn clear_network_requested(&mut self) -> Action {
        if self.state != State::Settings {
            return Action::Rejected;
        }
        self.clear_network_pending = true;
        Action::ConfirmClearNetwork
    }

    fn clear_network_confirmed(&mut self) -> Action {
        if !self.clear_network_pending {
            return Action::Rejected;
        }
        self.clear_network_pending = false;
        self.network_configured = false;
        self.wifi_state = WifiState::Idle;
        Action::ClearNetwork
    }

    fn factory_reset_requested(&mut self) -> Action {
        if !matches!(self.state, State::Settings | State::SafeMode) {
            return Action::Rejected;
        }
        self.factory_reset_pending = true;
        Action::ConfirmFactoryReset
    }

    fn factory_reset_confirmed(&mut self) -> Action {
        if !self.factory_reset_pending {
            return Action::Rejected;
        }
        self.factory_reset_pending = false;
        Action::FactoryReset
    }

    fn enter_safe_mode(&mut self, reason: FailureReason) -> Action {
        self.state = State::SafeMode;
        Action::EnterSafeMode(reason)
    }
}
