use crate::{LiveWifiState, ProvisioningState, WifiFailure, WifiOperationId};

const RECONNECT_DELAYS: [u32; 5] = [1, 2, 5, 10, 30];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppId {
    Counter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppSessionId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppDestination {
    Launcher,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfirmationId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PendingConfirmation {
    ClearNetwork(ConfirmationId),
    FactoryReset(ConfirmationId),
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
    AppStarting {
        app: AppId,
        session: AppSessionId,
    },
    AppRunning {
        app: AppId,
        session: AppSessionId,
    },
    AppStopping {
        app: AppId,
        session: AppSessionId,
        destination: AppDestination,
    },
    AppError {
        app: AppId,
        session: AppSessionId,
        reason: FailureReason,
    },
    Settings,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    BootSampled {
        safe_mode: bool,
    },
    StorageInitialized(Result<(), FailureReason>),
    ProfileValidated(Result<(), FailureReason>),
    DisplayInitialized(Result<(), FailureReason>),
    SystemUiInitialized(Result<(), FailureReason>),
    NetworkConfigLoaded {
        configured: bool,
    },
    SetupSkipped,
    OpenSettings,
    SetBacklight(Backlight),
    BackPressed,
    HomePressed,
    OpenApp(AppId),
    AppStarted {
        session: AppSessionId,
    },
    AppFailed {
        session: AppSessionId,
        reason: FailureReason,
    },
    RestartApp,
    AppStopped {
        session: AppSessionId,
    },
    WifiScanRequested,
    WifiScanCompleted {
        operation: WifiOperationId,
    },
    WifiScanFailed {
        operation: WifiOperationId,
        reason: WifiFailure,
    },
    WifiConnectRequested,
    WifiConnected {
        operation: WifiOperationId,
    },
    WifiPersisted {
        operation: WifiOperationId,
    },
    WifiFailed {
        operation: WifiOperationId,
        reason: WifiFailure,
    },
    ReconnectDue {
        reconnect: WifiOperationId,
    },
    ReconnectNowRequested,
    ClearNetworkRequested,
    ClearNetworkConfirmed {
        confirmation: ConfirmationId,
    },
    ClearNetworkCompleted {
        confirmation: ConfirmationId,
        result: Result<(), FailureReason>,
    },
    FactoryResetRequested,
    FactoryResetConfirmed {
        confirmation: ConfirmationId,
    },
    FactoryResetCompleted {
        confirmation: ConfirmationId,
        result: Result<(), FailureReason>,
    },
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
    ApplyBacklight(Backlight),
    StartWifiScan {
        operation: WifiOperationId,
    },
    ConnectWifi {
        operation: WifiOperationId,
    },
    ConnectSavedWifi {
        operation: WifiOperationId,
    },
    PersistWifi {
        operation: WifiOperationId,
    },
    ClearPendingWifi {
        operation: WifiOperationId,
    },
    ScheduleWifiReconnect {
        reconnect: WifiOperationId,
        after_secs: u32,
    },
    StartApp {
        app: AppId,
        session: AppSessionId,
    },
    StopApp {
        app: AppId,
        session: AppSessionId,
    },
    ShowAppError {
        app: AppId,
        session: AppSessionId,
        reason: FailureReason,
    },
    ConfirmClearNetwork {
        confirmation: ConfirmationId,
    },
    ClearNetwork {
        confirmation: ConfirmationId,
    },
    ConfirmFactoryReset {
        confirmation: ConfirmationId,
    },
    FactoryReset {
        confirmation: ConfirmationId,
    },
    Reboot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ClearOperation {
    confirmation: ConfirmationId,
    safe_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PendingReconnect {
    reconnect: WifiOperationId,
    after_secs: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicroOs {
    state: State,
    live_wifi_state: LiveWifiState,
    provisioning_state: ProvisioningState,
    network_configured: bool,
    boot_started: bool,
    pending_confirmation: Option<PendingConfirmation>,
    clearing_network: Option<ClearOperation>,
    factory_resetting: Option<ConfirmationId>,
    pending_reconnect: Option<WifiOperationId>,
    pending_reconnect_delay: Option<u32>,
    suspended_reconnect: Option<PendingReconnect>,
    backlight: Backlight,
    reconnect_index: usize,
    next_confirmation_id: u64,
    next_wifi_operation_id: u64,
    next_app_session_id: u64,
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
            live_wifi_state: LiveWifiState::Disconnected,
            provisioning_state: ProvisioningState::Idle,
            network_configured: false,
            boot_started: false,
            pending_confirmation: None,
            clearing_network: None,
            factory_resetting: None,
            pending_reconnect: None,
            pending_reconnect_delay: None,
            suspended_reconnect: None,
            backlight: Backlight::High,
            reconnect_index: 0,
            next_confirmation_id: 1,
            next_wifi_operation_id: 1,
            next_app_session_id: 1,
        }
    }

    #[must_use]
    pub fn state(&self) -> &State {
        &self.state
    }

    #[must_use]
    pub fn live_wifi_state(&self) -> &LiveWifiState {
        &self.live_wifi_state
    }

    #[must_use]
    pub fn provisioning_state(&self) -> &ProvisioningState {
        &self.provisioning_state
    }

    #[must_use]
    pub fn pending_confirmation(&self) -> Option<&PendingConfirmation> {
        self.pending_confirmation.as_ref()
    }

    #[must_use]
    pub fn network_configured(&self) -> bool {
        self.network_configured
    }

    #[must_use]
    pub fn backlight(&self) -> &Backlight {
        &self.backlight
    }

    #[must_use]
    pub fn next_reconnect_delay(&self) -> u32 {
        RECONNECT_DELAYS[self.reconnect_index]
    }

    pub fn dispatch(&mut self, event: Event) -> Action {
        if self.destructive_operation_in_flight()
            && matches!(
                &event,
                Event::SetupSkipped
                    | Event::OpenSettings
                    | Event::BackPressed
                    | Event::HomePressed
                    | Event::OpenApp(_)
                    | Event::AppStarted { .. }
                    | Event::AppFailed { .. }
                    | Event::RestartApp
                    | Event::AppStopped { .. }
                    | Event::WifiScanRequested
                    | Event::WifiScanCompleted { .. }
                    | Event::WifiScanFailed { .. }
                    | Event::WifiConnectRequested
                    | Event::WifiConnected { .. }
                    | Event::WifiPersisted { .. }
                    | Event::WifiFailed { .. }
                    | Event::ReconnectDue { .. }
                    | Event::ReconnectNowRequested
            )
        {
            return Action::Rejected;
        }
        match event {
            Event::BootSampled { safe_mode } => self.boot_sampled(safe_mode),
            Event::StorageInitialized(result) => self.storage_initialized(result),
            Event::ProfileValidated(result) => self.profile_validated(result),
            Event::DisplayInitialized(result) => self.display_initialized(result),
            Event::SystemUiInitialized(result) => self.system_ui_initialized(result),
            Event::NetworkConfigLoaded { configured } => self.network_config_loaded(configured),
            Event::SetupSkipped => self.setup_skipped(),
            Event::OpenSettings => self.open_settings(),
            Event::SetBacklight(backlight) => self.set_backlight(backlight),
            Event::BackPressed | Event::HomePressed => self.go_home(),
            Event::OpenApp(app) => self.open_app(app),
            Event::AppStarted { session } => self.app_started(session),
            Event::AppFailed { session, reason } => self.app_failed(session, reason),
            Event::RestartApp => self.restart_app(),
            Event::AppStopped { session } => self.app_stopped(session),
            Event::WifiScanRequested => self.wifi_scan_requested(),
            Event::WifiScanCompleted { operation } => self.wifi_scan_completed(operation),
            Event::WifiScanFailed { operation, reason } => self.wifi_scan_failed(operation, reason),
            Event::WifiConnectRequested => self.wifi_connect_requested(),
            Event::WifiConnected { operation } => self.wifi_connected(operation),
            Event::WifiPersisted { operation } => self.wifi_persisted(operation),
            Event::WifiFailed { operation, reason } => self.wifi_failed(operation, reason),
            Event::ReconnectDue { reconnect } => self.reconnect_due(reconnect),
            Event::ReconnectNowRequested => self.connect_saved_now(),
            Event::ClearNetworkRequested => self.clear_network_requested(),
            Event::ClearNetworkConfirmed { confirmation } => {
                self.clear_network_confirmed(confirmation)
            }
            Event::ClearNetworkCompleted {
                confirmation,
                result,
            } => self.clear_network_completed(confirmation, result),
            Event::FactoryResetRequested => self.factory_reset_requested(),
            Event::FactoryResetConfirmed { confirmation } => {
                self.factory_reset_confirmed(confirmation)
            }
            Event::FactoryResetCompleted {
                confirmation,
                result,
            } => self.factory_reset_completed(confirmation, result),
            Event::RebootRequested => Action::Reboot,
        }
    }

    fn issue_confirmation_id(&mut self) -> Option<ConfirmationId> {
        let next = self.next_confirmation_id.checked_add(1)?;
        let id = ConfirmationId(self.next_confirmation_id);
        self.next_confirmation_id = next;
        Some(id)
    }

    fn issue_wifi_operation_id(&mut self) -> Option<WifiOperationId> {
        let next = self.next_wifi_operation_id.checked_add(1)?;
        let id = WifiOperationId(self.next_wifi_operation_id);
        self.next_wifi_operation_id = next;
        Some(id)
    }

    fn issue_app_session_id(&mut self) -> Option<AppSessionId> {
        let next = self.next_app_session_id.checked_add(1)?;
        let id = AppSessionId(self.next_app_session_id);
        self.next_app_session_id = next;
        Some(id)
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
        if configured {
            let Some(operation) = self.issue_wifi_operation_id() else {
                return Action::Rejected;
            };
            self.network_configured = true;
            self.live_wifi_state = LiveWifiState::Connecting(operation);
            self.state = State::Launcher;
            Action::Actions(vec![
                Action::ShowLauncher,
                Action::ConnectSavedWifi { operation },
            ])
        } else {
            self.network_configured = false;
            self.live_wifi_state = LiveWifiState::Disconnected;
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
        let _ = self.cancel_pending_confirmation();
        self.state = State::Settings;
        Action::ShowSettings
    }

    fn set_backlight(&mut self, backlight: Backlight) -> Action {
        if !matches!(&self.state, State::Launcher | State::Settings) {
            return Action::Rejected;
        }
        self.backlight = backlight.clone();
        Action::ApplyBacklight(backlight)
    }

    fn go_home(&mut self) -> Action {
        match self.state.clone() {
            State::Settings => {
                let resumed = self.cancel_pending_confirmation();
                self.state = State::Launcher;
                if let Some(resumed) = resumed {
                    Action::Actions(vec![Action::ShowLauncher, resumed])
                } else {
                    Action::ShowLauncher
                }
            }
            State::AppStarting { app, session } | State::AppRunning { app, session } => {
                self.state = State::AppStopping {
                    app: app.clone(),
                    session,
                    destination: AppDestination::Launcher,
                };
                Action::StopApp { app, session }
            }
            State::AppStopping { .. } => Action::Rejected,
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
        let Some(session) = self.issue_app_session_id() else {
            return Action::Rejected;
        };
        let _ = self.cancel_pending_confirmation();
        self.state = State::AppStarting {
            app: app.clone(),
            session,
        };
        Action::StartApp { app, session }
    }

    fn app_started(&mut self, callback_session: AppSessionId) -> Action {
        let State::AppStarting { app, session } = self.state.clone() else {
            return Action::Rejected;
        };
        if session != callback_session {
            return Action::Rejected;
        }
        self.state = State::AppRunning { app, session };
        Action::None
    }

    fn app_failed(&mut self, callback_session: AppSessionId, reason: FailureReason) -> Action {
        match self.state.clone() {
            State::AppStarting { app, session } | State::AppRunning { app, session }
                if session == callback_session =>
            {
                self.state = State::AppError {
                    app: app.clone(),
                    session,
                    reason: reason.clone(),
                };
                Action::ShowAppError {
                    app,
                    session,
                    reason,
                }
            }
            State::AppStopping { session, .. } if session == callback_session => Action::None,
            _ => Action::Rejected,
        }
    }

    fn restart_app(&mut self) -> Action {
        let State::AppError { app, .. } = self.state.clone() else {
            return Action::Rejected;
        };
        let Some(session) = self.issue_app_session_id() else {
            return Action::Rejected;
        };
        self.state = State::AppStarting {
            app: app.clone(),
            session,
        };
        Action::StartApp { app, session }
    }

    fn app_stopped(&mut self, callback_session: AppSessionId) -> Action {
        let State::AppStopping {
            session,
            destination,
            ..
        } = self.state.clone()
        else {
            return Action::Rejected;
        };
        if session != callback_session {
            return Action::Rejected;
        }
        match destination {
            AppDestination::Launcher => {
                self.state = State::Launcher;
                Action::ShowLauncher
            }
        }
    }

    fn wifi_context(&self) -> bool {
        matches!(self.state, State::FirstRunSetup | State::Settings)
    }

    fn destructive_operation_in_flight(&self) -> bool {
        self.clearing_network.is_some() || self.factory_resetting.is_some()
    }

    fn wifi_admits_provisioning(&self) -> bool {
        matches!(
            self.provisioning_state,
            ProvisioningState::Idle | ProvisioningState::Failed { .. }
        ) && !matches!(self.live_wifi_state, LiveWifiState::Connecting(_))
            && self.pending_confirmation.is_none()
            && !self.destructive_operation_in_flight()
    }

    fn wifi_operation_in_flight(&self) -> bool {
        matches!(self.live_wifi_state, LiveWifiState::Connecting(_))
            || matches!(
                self.provisioning_state,
                ProvisioningState::Scanning(_)
                    | ProvisioningState::ConnectingReplacement(_)
                    | ProvisioningState::Persisting(_)
            )
    }

    fn wifi_scan_requested(&mut self) -> Action {
        if !self.wifi_context() || !self.wifi_admits_provisioning() {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
        self.pending_reconnect_delay = None;
        self.provisioning_state = ProvisioningState::Scanning(operation);
        Action::StartWifiScan { operation }
    }

    fn wifi_scan_completed(&mut self, operation: WifiOperationId) -> Action {
        if self.provisioning_state != ProvisioningState::Scanning(operation) {
            return Action::Rejected;
        }
        self.provisioning_state = ProvisioningState::Idle;
        Action::None
    }

    fn wifi_scan_failed(&mut self, operation: WifiOperationId, reason: WifiFailure) -> Action {
        if self.provisioning_state != ProvisioningState::Scanning(operation) {
            return Action::Rejected;
        }
        self.provisioning_state = ProvisioningState::Failed { operation, reason };
        Action::None
    }

    fn wifi_connect_requested(&mut self) -> Action {
        if !self.wifi_context() || !self.wifi_admits_provisioning() {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
        self.pending_reconnect_delay = None;
        self.provisioning_state = ProvisioningState::ConnectingReplacement(operation);
        Action::ConnectWifi { operation }
    }

    fn wifi_connected(&mut self, operation: WifiOperationId) -> Action {
        if self.live_wifi_state == LiveWifiState::Connecting(operation) {
            self.live_wifi_state = LiveWifiState::Connected;
            self.reconnect_index = 0;
            self.pending_reconnect = None;
            self.pending_reconnect_delay = None;
            return Action::None;
        }
        if self.provisioning_state == ProvisioningState::ConnectingReplacement(operation) {
            self.provisioning_state = ProvisioningState::Persisting(operation);
            self.reconnect_index = 0;
            return Action::PersistWifi { operation };
        }
        Action::Rejected
    }

    fn wifi_persisted(&mut self, operation: WifiOperationId) -> Action {
        if self.provisioning_state != ProvisioningState::Persisting(operation) {
            return Action::Rejected;
        }
        self.network_configured = true;
        self.live_wifi_state = LiveWifiState::Connected;
        self.provisioning_state = ProvisioningState::Idle;
        self.pending_reconnect = None;
        self.pending_reconnect_delay = None;
        if self.state == State::FirstRunSetup {
            self.state = State::Launcher;
            Action::ShowLauncher
        } else {
            Action::None
        }
    }

    fn wifi_failed(&mut self, operation: WifiOperationId, reason: WifiFailure) -> Action {
        let saved_connect = self.live_wifi_state == LiveWifiState::Connecting(operation);
        let replacement = matches!(
            self.provisioning_state,
            ProvisioningState::ConnectingReplacement(id) | ProvisioningState::Persisting(id)
                if id == operation
        );
        if !saved_connect && !replacement {
            return Action::Rejected;
        }
        let needs_saved_recovery = self.network_configured
            && (saved_connect || self.live_wifi_state == LiveWifiState::Disconnected);
        let reconnect = if needs_saved_recovery {
            self.issue_wifi_operation_id()
        } else {
            None
        };
        if saved_connect {
            self.live_wifi_state = LiveWifiState::Disconnected;
        }
        if replacement {
            self.provisioning_state = ProvisioningState::Failed { operation, reason };
        }
        if let Some(reconnect) = reconnect {
            let delay = RECONNECT_DELAYS[self.reconnect_index];
            self.reconnect_index = (self.reconnect_index + 1).min(RECONNECT_DELAYS.len() - 1);
            self.pending_reconnect = Some(reconnect);
            self.pending_reconnect_delay = Some(delay);
            let schedule = Action::ScheduleWifiReconnect {
                reconnect,
                after_secs: delay,
            };
            if replacement {
                Action::Actions(vec![Action::ClearPendingWifi { operation }, schedule])
            } else {
                schedule
            }
        } else if replacement {
            self.pending_reconnect = None;
            self.pending_reconnect_delay = None;
            Action::ClearPendingWifi { operation }
        } else {
            Action::None
        }
    }

    fn reconnect_due(&mut self, reconnect: WifiOperationId) -> Action {
        if self.pending_reconnect != Some(reconnect)
            || !self.network_configured
            || self.pending_confirmation.is_some()
            || self.destructive_operation_in_flight()
            || self.live_wifi_state != LiveWifiState::Disconnected
            || !matches!(
                self.provisioning_state,
                ProvisioningState::Idle | ProvisioningState::Failed { .. }
            )
        {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
        self.pending_reconnect_delay = None;
        self.live_wifi_state = LiveWifiState::Connecting(operation);
        Action::ConnectSavedWifi { operation }
    }

    fn connect_saved_now(&mut self) -> Action {
        if !self.network_configured
            || self.pending_confirmation.is_some()
            || self.destructive_operation_in_flight()
            || matches!(self.live_wifi_state, LiveWifiState::Connecting(_))
            || !matches!(
                self.provisioning_state,
                ProvisioningState::Idle | ProvisioningState::Failed { .. }
            )
        {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
        self.pending_reconnect_delay = None;
        self.live_wifi_state = LiveWifiState::Connecting(operation);
        Action::ConnectSavedWifi { operation }
    }

    fn confirmation_context(&self) -> bool {
        matches!(self.state, State::Settings | State::SafeMode)
    }

    fn clear_network_requested(&mut self) -> Action {
        if !self.confirmation_context()
            || self.pending_confirmation.is_some()
            || self.clearing_network.is_some()
            || self.factory_resetting.is_some()
            || self.wifi_operation_in_flight()
        {
            return Action::Rejected;
        }
        let Some(confirmation) = self.issue_confirmation_id() else {
            return Action::Rejected;
        };
        self.pending_confirmation = Some(PendingConfirmation::ClearNetwork(confirmation));
        Action::ConfirmClearNetwork { confirmation }
    }

    fn clear_network_confirmed(&mut self, confirmation: ConfirmationId) -> Action {
        if self.pending_confirmation != Some(PendingConfirmation::ClearNetwork(confirmation)) {
            return Action::Rejected;
        }
        self.pending_confirmation = None;
        self.suspend_pending_reconnect();
        self.clearing_network = Some(ClearOperation {
            confirmation,
            safe_mode: self.state == State::SafeMode,
        });
        Action::ClearNetwork { confirmation }
    }

    fn clear_network_completed(
        &mut self,
        confirmation: ConfirmationId,
        result: Result<(), FailureReason>,
    ) -> Action {
        let Some(operation) = self.clearing_network.clone() else {
            return Action::Rejected;
        };
        if operation.confirmation != confirmation {
            return Action::Rejected;
        }
        self.clearing_network = None;
        match result {
            Err(_) => self.resume_suspended_reconnect().unwrap_or(Action::None),
            Ok(()) => {
                self.network_configured = false;
                self.live_wifi_state = LiveWifiState::Disconnected;
                self.provisioning_state = ProvisioningState::Idle;
                self.pending_reconnect = None;
                self.pending_reconnect_delay = None;
                self.suspended_reconnect = None;
                if operation.safe_mode {
                    self.state = State::SafeMode;
                    Action::None
                } else {
                    self.state = State::FirstRunSetup;
                    Action::ShowFirstRunSetup
                }
            }
        }
    }

    fn factory_reset_requested(&mut self) -> Action {
        if !self.confirmation_context()
            || self.pending_confirmation.is_some()
            || self.factory_resetting.is_some()
            || self.clearing_network.is_some()
            || self.wifi_operation_in_flight()
        {
            return Action::Rejected;
        }
        let Some(confirmation) = self.issue_confirmation_id() else {
            return Action::Rejected;
        };
        self.pending_confirmation = Some(PendingConfirmation::FactoryReset(confirmation));
        Action::ConfirmFactoryReset { confirmation }
    }

    fn factory_reset_confirmed(&mut self, confirmation: ConfirmationId) -> Action {
        if self.pending_confirmation != Some(PendingConfirmation::FactoryReset(confirmation)) {
            return Action::Rejected;
        }
        self.pending_confirmation = None;
        self.suspend_pending_reconnect();
        self.factory_resetting = Some(confirmation);
        Action::FactoryReset { confirmation }
    }

    fn factory_reset_completed(
        &mut self,
        confirmation: ConfirmationId,
        result: Result<(), FailureReason>,
    ) -> Action {
        if self.factory_resetting != Some(confirmation) {
            return Action::Rejected;
        }
        self.factory_resetting = None;
        if result.is_ok() {
            self.pending_reconnect = None;
            self.pending_reconnect_delay = None;
            self.suspended_reconnect = None;
            Action::Reboot
        } else {
            self.resume_suspended_reconnect().unwrap_or(Action::None)
        }
    }

    fn cancel_pending_confirmation(&mut self) -> Option<Action> {
        let was_pending = self.pending_confirmation.take().is_some();
        if !was_pending {
            return None;
        }
        self.pending_reconnect
            .zip(self.pending_reconnect_delay)
            .map(|(reconnect, after_secs)| Action::ScheduleWifiReconnect {
                reconnect,
                after_secs,
            })
    }

    fn suspend_pending_reconnect(&mut self) {
        self.suspended_reconnect = self
            .pending_reconnect
            .take()
            .zip(self.pending_reconnect_delay.take())
            .map(|(reconnect, after_secs)| PendingReconnect {
                reconnect,
                after_secs,
            });
    }

    fn resume_suspended_reconnect(&mut self) -> Option<Action> {
        let reconnect = self.suspended_reconnect.take()?;
        if !self.network_configured || self.live_wifi_state != LiveWifiState::Disconnected {
            return None;
        }
        self.pending_reconnect = Some(reconnect.reconnect);
        self.pending_reconnect_delay = Some(reconnect.after_secs);
        Some(Action::ScheduleWifiReconnect {
            reconnect: reconnect.reconnect,
            after_secs: reconnect.after_secs,
        })
    }

    fn discard_reconnects(&mut self) {
        self.pending_confirmation = None;
        self.pending_reconnect = None;
        self.pending_reconnect_delay = None;
        self.suspended_reconnect = None;
    }

    fn enter_safe_mode(&mut self, reason: FailureReason) -> Action {
        self.discard_reconnects();
        self.state = State::SafeMode;
        Action::EnterSafeMode(reason)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confirmation_ids_stop_before_max_without_mutating_state() {
        let mut os = MicroOs::new();
        os.state = State::Settings;
        os.next_confirmation_id = u64::MAX - 1;
        assert_eq!(
            os.dispatch(Event::ClearNetworkRequested),
            Action::ConfirmClearNetwork {
                confirmation: ConfirmationId(u64::MAX - 1)
            }
        );
        os.pending_confirmation = None;
        let before = os.clone();
        assert_eq!(os.dispatch(Event::ClearNetworkRequested), Action::Rejected);
        assert_eq!(os, before);
    }

    #[test]
    fn wifi_operation_ids_stop_before_max_without_mutating_state() {
        let mut os = MicroOs::new();
        os.state = State::FirstRunSetup;
        os.next_wifi_operation_id = u64::MAX - 1;
        assert_eq!(
            os.dispatch(Event::WifiScanRequested),
            Action::StartWifiScan {
                operation: WifiOperationId(u64::MAX - 1)
            }
        );
        os.provisioning_state = ProvisioningState::Idle;
        let before = os.clone();
        assert_eq!(os.dispatch(Event::WifiScanRequested), Action::Rejected);
        assert_eq!(os, before);
    }

    #[test]
    fn app_session_ids_stop_before_max_without_mutating_state() {
        let mut os = MicroOs::new();
        os.state = State::Launcher;
        os.next_app_session_id = u64::MAX - 1;
        assert_eq!(
            os.dispatch(Event::OpenApp(AppId::Counter)),
            Action::StartApp {
                app: AppId::Counter,
                session: AppSessionId(u64::MAX - 1)
            }
        );
        os.state = State::Launcher;
        let before = os.clone();
        assert_eq!(
            os.dispatch(Event::OpenApp(AppId::Counter)),
            Action::Rejected
        );
        assert_eq!(os, before);
    }

    #[test]
    fn saved_wifi_failure_is_recorded_when_reconnect_id_is_exhausted() {
        let operation = WifiOperationId(41);
        let mut os = MicroOs::new();
        os.state = State::Launcher;
        os.network_configured = true;
        os.live_wifi_state = LiveWifiState::Connecting(operation);
        os.next_wifi_operation_id = u64::MAX;
        assert_eq!(
            os.dispatch(Event::WifiFailed {
                operation,
                reason: WifiFailure::Timeout
            }),
            Action::None
        );
        assert_eq!(os.live_wifi_state, LiveWifiState::Disconnected);
        assert_eq!(os.pending_reconnect, None);
    }

    #[test]
    fn replacement_failure_is_recorded_when_reconnect_id_is_exhausted() {
        let operation = WifiOperationId(42);
        let reason = WifiFailure::Authentication;
        let mut os = MicroOs::new();
        os.state = State::Settings;
        os.network_configured = true;
        os.live_wifi_state = LiveWifiState::Disconnected;
        os.provisioning_state = ProvisioningState::ConnectingReplacement(operation);
        os.next_wifi_operation_id = u64::MAX;
        assert_eq!(
            os.dispatch(Event::WifiFailed {
                operation,
                reason: reason.clone()
            }),
            Action::ClearPendingWifi { operation }
        );
        assert_eq!(
            os.provisioning_state,
            ProvisioningState::Failed { operation, reason }
        );
        assert_eq!(os.pending_reconnect, None);
    }
}
