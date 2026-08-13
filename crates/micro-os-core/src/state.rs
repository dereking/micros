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
    pub fn next_reconnect_delay(&self) -> u32 {
        RECONNECT_DELAYS[self.reconnect_index]
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
            Event::AppStarted { session } => self.app_started(session),
            Event::AppFailed { session, reason } => self.app_failed(session, reason),
            Event::RestartApp => self.restart_app(),
            Event::AppStopped { session } => self.app_stopped(session),
            Event::WifiScanRequested => self.wifi_scan_requested(),
            Event::WifiScanCompleted { operation } => self.wifi_scan_completed(operation),
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
        self.cancel_pending_confirmation();
        self.state = State::Settings;
        Action::ShowSettings
    }

    fn go_home(&mut self) -> Action {
        match self.state.clone() {
            State::Settings => {
                self.cancel_pending_confirmation();
                self.state = State::Launcher;
                Action::ShowLauncher
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
        self.cancel_pending_confirmation();
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

    fn wifi_scan_requested(&mut self) -> Action {
        if !self.wifi_context()
            || matches!(self.provisioning_state, ProvisioningState::Persisting(_))
        {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
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

    fn wifi_connect_requested(&mut self) -> Action {
        if !self.wifi_context()
            || matches!(self.provisioning_state, ProvisioningState::Persisting(_))
        {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
        self.provisioning_state = ProvisioningState::ConnectingReplacement(operation);
        Action::ConnectWifi { operation }
    }

    fn wifi_connected(&mut self, operation: WifiOperationId) -> Action {
        if self.live_wifi_state == LiveWifiState::Connecting(operation) {
            self.live_wifi_state = LiveWifiState::Connected;
            self.reconnect_index = 0;
            self.pending_reconnect = None;
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
        let Some(reconnect) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        let delay = RECONNECT_DELAYS[self.reconnect_index];
        self.reconnect_index = (self.reconnect_index + 1).min(RECONNECT_DELAYS.len() - 1);
        self.pending_reconnect = Some(reconnect);
        if saved_connect {
            self.live_wifi_state = LiveWifiState::Disconnected;
        }
        if replacement {
            self.provisioning_state = ProvisioningState::Failed { operation, reason };
            Action::Actions(vec![
                Action::ClearPendingWifi { operation },
                Action::ScheduleWifiReconnect {
                    reconnect,
                    after_secs: delay,
                },
            ])
        } else {
            Action::ScheduleWifiReconnect {
                reconnect,
                after_secs: delay,
            }
        }
    }

    fn reconnect_due(&mut self, reconnect: WifiOperationId) -> Action {
        if self.pending_reconnect != Some(reconnect) || !self.network_configured {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
        self.live_wifi_state = LiveWifiState::Connecting(operation);
        Action::ConnectSavedWifi { operation }
    }

    fn connect_saved_now(&mut self) -> Action {
        if !self.network_configured {
            return Action::Rejected;
        }
        let Some(operation) = self.issue_wifi_operation_id() else {
            return Action::Rejected;
        };
        self.pending_reconnect = None;
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
            Err(_) => Action::None,
            Ok(()) => {
                self.network_configured = false;
                self.live_wifi_state = LiveWifiState::Disconnected;
                self.provisioning_state = ProvisioningState::Idle;
                self.pending_reconnect = None;
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
            Action::Reboot
        } else {
            Action::None
        }
    }

    fn cancel_pending_confirmation(&mut self) {
        self.pending_confirmation = None;
    }

    fn enter_safe_mode(&mut self, reason: FailureReason) -> Action {
        self.cancel_pending_confirmation();
        self.state = State::SafeMode;
        Action::EnterSafeMode(reason)
    }
}
