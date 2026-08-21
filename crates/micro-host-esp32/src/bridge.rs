use micro_os_core::{
    Action, AppId, AppSessionId, Backlight, ConfirmationId, Event, FailureReason, State,
    WifiFailure, WifiOperationId,
};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroErrorCode {
    Ok = 0,
    Mbc = 1,
    Runtime = 2,
    Ui = 3,
    InvalidArgument = 4,
    Panic = 5,
    Stopped = 6,
    BufferTooSmall = 7,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroState {
    EarlyBoot = 0,
    SafeMode = 1,
    StorageReady = 2,
    BoardProfileValidated = 3,
    DisplayReady = 4,
    SystemUiReady = 5,
    FirstRunSetup = 6,
    Launcher = 7,
    AppStarting = 8,
    AppRunning = 9,
    AppStopping = 10,
    AppError = 11,
    Settings = 12,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroResult {
    Unused = 0,
    Ok = 1,
    Err = 2,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroFailureReason {
    Unused = 0,
    SafeModeRequested = 1,
    StorageCorrupt = 2,
    InvalidBoardProfile = 3,
    HardwareUnavailable = 4,
    AppCrashed = 5,
    Internal = 6,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroWifiFailure {
    Unused = 0,
    Authentication = 1,
    NetworkMissing = 2,
    Timeout = 3,
    Internal = 4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroAppId {
    Unused = 0,
    Counter = 1,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroBacklight {
    Unused = 0,
    Off = 1,
    Low = 2,
    Medium = 3,
    High = 4,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroEventKind {
    BootSampled = 0,
    StorageInitialized = 1,
    ProfileValidated = 2,
    DisplayInitialized = 3,
    SystemUiInitialized = 4,
    NetworkConfigLoaded = 5,
    SetupSkipped = 6,
    OpenSettings = 7,
    BackPressed = 8,
    HomePressed = 9,
    OpenApp = 10,
    AppStarted = 11,
    AppFailed = 12,
    RestartApp = 13,
    AppStopped = 14,
    WifiScanRequested = 15,
    WifiScanCompleted = 16,
    WifiScanFailed = 17,
    WifiConnectRequested = 18,
    WifiConnected = 19,
    WifiPersisted = 20,
    WifiFailed = 21,
    ReconnectDue = 22,
    ReconnectNowRequested = 23,
    ClearNetworkRequested = 24,
    ClearNetworkConfirmed = 25,
    ClearNetworkCompleted = 26,
    FactoryResetRequested = 27,
    FactoryResetConfirmed = 28,
    FactoryResetCompleted = 29,
    RebootRequested = 30,
    SetBacklight = 31,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicroEvent {
    pub kind: MicroEventKind,
    pub result: MicroResult,
    pub failure: MicroFailureReason,
    pub wifi_failure: MicroWifiFailure,
    pub app: MicroAppId,
    pub flag: u32,
    pub after_secs: u32,
    pub reserved: u32,
    pub session_id: u64,
    pub operation_id: u64,
    pub confirmation_id: u64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroActionKind {
    None = 0,
    Rejected = 1,
    Actions = 2,
    EnterSafeMode = 3,
    InitializeStorage = 4,
    ValidateProfile = 5,
    InitializeDisplay = 6,
    InitializeSystemUi = 7,
    LoadNetworkConfig = 8,
    ShowFirstRunSetup = 9,
    ShowLauncher = 10,
    ShowSettings = 11,
    StartWifiScan = 12,
    ConnectWifi = 13,
    ConnectSavedWifi = 14,
    PersistWifi = 15,
    ClearPendingWifi = 16,
    ScheduleWifiReconnect = 17,
    StartApp = 18,
    StopApp = 19,
    ShowAppError = 20,
    ConfirmClearNetwork = 21,
    ClearNetwork = 22,
    ConfirmFactoryReset = 23,
    FactoryReset = 24,
    Reboot = 25,
    ApplyBacklight = 26,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicroAction {
    pub kind: MicroActionKind,
    pub child_count: u32,
    pub failure: MicroFailureReason,
    pub app: MicroAppId,
    pub after_secs: u32,
    pub backlight: MicroBacklight,
    pub reserved_1: u32,
    pub reserved_2: u32,
    pub session_id: u64,
    pub operation_id: u64,
    pub confirmation_id: u64,
}

impl MicroAction {
    #[must_use]
    pub const fn new(kind: MicroActionKind) -> Self {
        Self {
            kind,
            child_count: 0,
            failure: MicroFailureReason::Unused,
            app: MicroAppId::Unused,
            after_secs: 0,
            backlight: MicroBacklight::Unused,
            reserved_1: 0,
            reserved_2: 0,
            session_id: 0,
            operation_id: 0,
            confirmation_id: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DispatchError {
    pub code: MicroErrorCode,
    pub required: usize,
}

fn failure_to_wire(reason: &FailureReason) -> MicroFailureReason {
    match reason {
        FailureReason::SafeModeRequested => MicroFailureReason::SafeModeRequested,
        FailureReason::StorageCorrupt => MicroFailureReason::StorageCorrupt,
        FailureReason::InvalidBoardProfile => MicroFailureReason::InvalidBoardProfile,
        FailureReason::HardwareUnavailable => MicroFailureReason::HardwareUnavailable,
        FailureReason::AppCrashed => MicroFailureReason::AppCrashed,
        FailureReason::Internal => MicroFailureReason::Internal,
    }
}

fn failure_from_wire(reason: MicroFailureReason) -> Result<FailureReason, MicroErrorCode> {
    match reason {
        MicroFailureReason::SafeModeRequested => Ok(FailureReason::SafeModeRequested),
        MicroFailureReason::StorageCorrupt => Ok(FailureReason::StorageCorrupt),
        MicroFailureReason::InvalidBoardProfile => Ok(FailureReason::InvalidBoardProfile),
        MicroFailureReason::HardwareUnavailable => Ok(FailureReason::HardwareUnavailable),
        MicroFailureReason::AppCrashed => Ok(FailureReason::AppCrashed),
        MicroFailureReason::Internal => Ok(FailureReason::Internal),
        MicroFailureReason::Unused => Err(MicroErrorCode::InvalidArgument),
    }
}

fn wifi_to_wire(reason: &WifiFailure) -> MicroWifiFailure {
    match reason {
        WifiFailure::Authentication => MicroWifiFailure::Authentication,
        WifiFailure::NetworkMissing => MicroWifiFailure::NetworkMissing,
        WifiFailure::Timeout => MicroWifiFailure::Timeout,
        WifiFailure::Internal => MicroWifiFailure::Internal,
    }
}

fn wifi_from_wire(reason: MicroWifiFailure) -> Result<WifiFailure, MicroErrorCode> {
    match reason {
        MicroWifiFailure::Authentication => Ok(WifiFailure::Authentication),
        MicroWifiFailure::NetworkMissing => Ok(WifiFailure::NetworkMissing),
        MicroWifiFailure::Timeout => Ok(WifiFailure::Timeout),
        MicroWifiFailure::Internal => Ok(WifiFailure::Internal),
        MicroWifiFailure::Unused => Err(MicroErrorCode::InvalidArgument),
    }
}

fn app_to_wire(app: &AppId) -> MicroAppId {
    match app {
        AppId::Counter => MicroAppId::Counter,
    }
}

fn app_from_wire(app: MicroAppId) -> Result<AppId, MicroErrorCode> {
    match app {
        MicroAppId::Counter => Ok(AppId::Counter),
        MicroAppId::Unused => Err(MicroErrorCode::InvalidArgument),
    }
}

fn backlight_to_wire(backlight: &Backlight) -> MicroBacklight {
    match backlight {
        Backlight::Off => MicroBacklight::Off,
        Backlight::Low => MicroBacklight::Low,
        Backlight::Medium => MicroBacklight::Medium,
        Backlight::High => MicroBacklight::High,
    }
}

fn backlight_from_wire(backlight: MicroBacklight) -> Result<Backlight, MicroErrorCode> {
    match backlight {
        MicroBacklight::Off => Ok(Backlight::Off),
        MicroBacklight::Low => Ok(Backlight::Low),
        MicroBacklight::Medium => Ok(Backlight::Medium),
        MicroBacklight::High => Ok(Backlight::High),
        MicroBacklight::Unused => Err(MicroErrorCode::InvalidArgument),
    }
}

impl MicroEvent {
    fn empty(kind: MicroEventKind) -> Self {
        Self {
            kind,
            result: MicroResult::Unused,
            failure: MicroFailureReason::Unused,
            wifi_failure: MicroWifiFailure::Unused,
            app: MicroAppId::Unused,
            flag: 0,
            after_secs: 0,
            reserved: 0,
            session_id: 0,
            operation_id: 0,
            confirmation_id: 0,
        }
    }

    #[must_use]
    pub fn from_core(event: &Event) -> Self {
        let mut wire = Self::empty(match event {
            Event::BootSampled { .. } => MicroEventKind::BootSampled,
            Event::StorageInitialized(_) => MicroEventKind::StorageInitialized,
            Event::ProfileValidated(_) => MicroEventKind::ProfileValidated,
            Event::DisplayInitialized(_) => MicroEventKind::DisplayInitialized,
            Event::SystemUiInitialized(_) => MicroEventKind::SystemUiInitialized,
            Event::NetworkConfigLoaded { .. } => MicroEventKind::NetworkConfigLoaded,
            Event::SetupSkipped => MicroEventKind::SetupSkipped,
            Event::OpenSettings => MicroEventKind::OpenSettings,
            Event::SetBacklight(_) => MicroEventKind::SetBacklight,
            Event::BackPressed => MicroEventKind::BackPressed,
            Event::HomePressed => MicroEventKind::HomePressed,
            Event::OpenApp(_) => MicroEventKind::OpenApp,
            Event::AppStarted { .. } => MicroEventKind::AppStarted,
            Event::AppFailed { .. } => MicroEventKind::AppFailed,
            Event::RestartApp => MicroEventKind::RestartApp,
            Event::AppStopped { .. } => MicroEventKind::AppStopped,
            Event::WifiScanRequested => MicroEventKind::WifiScanRequested,
            Event::WifiScanCompleted { .. } => MicroEventKind::WifiScanCompleted,
            Event::WifiScanFailed { .. } => MicroEventKind::WifiScanFailed,
            Event::WifiConnectRequested => MicroEventKind::WifiConnectRequested,
            Event::WifiConnected { .. } => MicroEventKind::WifiConnected,
            Event::WifiPersisted { .. } => MicroEventKind::WifiPersisted,
            Event::WifiFailed { .. } => MicroEventKind::WifiFailed,
            Event::ReconnectDue { .. } => MicroEventKind::ReconnectDue,
            Event::ReconnectNowRequested => MicroEventKind::ReconnectNowRequested,
            Event::ClearNetworkRequested => MicroEventKind::ClearNetworkRequested,
            Event::ClearNetworkConfirmed { .. } => MicroEventKind::ClearNetworkConfirmed,
            Event::ClearNetworkCompleted { .. } => MicroEventKind::ClearNetworkCompleted,
            Event::FactoryResetRequested => MicroEventKind::FactoryResetRequested,
            Event::FactoryResetConfirmed { .. } => MicroEventKind::FactoryResetConfirmed,
            Event::FactoryResetCompleted { .. } => MicroEventKind::FactoryResetCompleted,
            Event::RebootRequested => MicroEventKind::RebootRequested,
        });
        match event {
            Event::BootSampled { safe_mode } => wire.flag = u32::from(*safe_mode),
            Event::StorageInitialized(result)
            | Event::ProfileValidated(result)
            | Event::DisplayInitialized(result)
            | Event::SystemUiInitialized(result) => set_result(&mut wire, result),
            Event::NetworkConfigLoaded { configured } => wire.flag = u32::from(*configured),
            Event::SetBacklight(backlight) => wire.flag = backlight_to_wire(backlight) as u32,
            Event::OpenApp(app) => wire.app = app_to_wire(app),
            Event::AppStarted { session } | Event::AppStopped { session } => {
                wire.session_id = session.0
            }
            Event::AppFailed { session, reason } => {
                wire.session_id = session.0;
                wire.failure = failure_to_wire(reason);
            }
            Event::WifiScanCompleted { operation }
            | Event::WifiConnected { operation }
            | Event::WifiPersisted { operation } => wire.operation_id = operation.0,
            Event::WifiScanFailed { operation, reason }
            | Event::WifiFailed { operation, reason } => {
                wire.operation_id = operation.0;
                wire.wifi_failure = wifi_to_wire(reason);
            }
            Event::ReconnectDue { reconnect } => wire.operation_id = reconnect.0,
            Event::ClearNetworkConfirmed { confirmation }
            | Event::FactoryResetConfirmed { confirmation } => {
                wire.confirmation_id = confirmation.0
            }
            Event::ClearNetworkCompleted {
                confirmation,
                result,
            }
            | Event::FactoryResetCompleted {
                confirmation,
                result,
            } => {
                wire.confirmation_id = confirmation.0;
                set_result(&mut wire, result);
            }
            Event::SetupSkipped
            | Event::OpenSettings
            | Event::BackPressed
            | Event::HomePressed
            | Event::RestartApp
            | Event::WifiScanRequested
            | Event::WifiConnectRequested
            | Event::ReconnectNowRequested
            | Event::ClearNetworkRequested
            | Event::FactoryResetRequested
            | Event::RebootRequested => {}
        }
        wire
    }

    pub fn try_into_core(self) -> Result<Event, MicroErrorCode> {
        self.validate_canonical()?;
        let event = match self.kind {
            MicroEventKind::BootSampled => Event::BootSampled {
                safe_mode: bool_flag(self.flag)?,
            },
            MicroEventKind::StorageInitialized => {
                Event::StorageInitialized(result_from_wire(self.result, self.failure)?)
            }
            MicroEventKind::ProfileValidated => {
                Event::ProfileValidated(result_from_wire(self.result, self.failure)?)
            }
            MicroEventKind::DisplayInitialized => {
                Event::DisplayInitialized(result_from_wire(self.result, self.failure)?)
            }
            MicroEventKind::SystemUiInitialized => {
                Event::SystemUiInitialized(result_from_wire(self.result, self.failure)?)
            }
            MicroEventKind::NetworkConfigLoaded => Event::NetworkConfigLoaded {
                configured: bool_flag(self.flag)?,
            },
            MicroEventKind::SetupSkipped => Event::SetupSkipped,
            MicroEventKind::OpenSettings => Event::OpenSettings,
            MicroEventKind::SetBacklight => {
                Event::SetBacklight(backlight_from_wire(backlight_kind_from_raw(self.flag)?)?)
            }
            MicroEventKind::BackPressed => Event::BackPressed,
            MicroEventKind::HomePressed => Event::HomePressed,
            MicroEventKind::OpenApp => Event::OpenApp(app_from_wire(self.app)?),
            MicroEventKind::AppStarted => Event::AppStarted {
                session: AppSessionId(self.session_id),
            },
            MicroEventKind::AppFailed => Event::AppFailed {
                session: AppSessionId(self.session_id),
                reason: failure_from_wire(self.failure)?,
            },
            MicroEventKind::RestartApp => Event::RestartApp,
            MicroEventKind::AppStopped => Event::AppStopped {
                session: AppSessionId(self.session_id),
            },
            MicroEventKind::WifiScanRequested => Event::WifiScanRequested,
            MicroEventKind::WifiScanCompleted => Event::WifiScanCompleted {
                operation: WifiOperationId(self.operation_id),
            },
            MicroEventKind::WifiScanFailed => Event::WifiScanFailed {
                operation: WifiOperationId(self.operation_id),
                reason: wifi_from_wire(self.wifi_failure)?,
            },
            MicroEventKind::WifiConnectRequested => Event::WifiConnectRequested,
            MicroEventKind::WifiConnected => Event::WifiConnected {
                operation: WifiOperationId(self.operation_id),
            },
            MicroEventKind::WifiPersisted => Event::WifiPersisted {
                operation: WifiOperationId(self.operation_id),
            },
            MicroEventKind::WifiFailed => Event::WifiFailed {
                operation: WifiOperationId(self.operation_id),
                reason: wifi_from_wire(self.wifi_failure)?,
            },
            MicroEventKind::ReconnectDue => Event::ReconnectDue {
                reconnect: WifiOperationId(self.operation_id),
            },
            MicroEventKind::ReconnectNowRequested => Event::ReconnectNowRequested,
            MicroEventKind::ClearNetworkRequested => Event::ClearNetworkRequested,
            MicroEventKind::ClearNetworkConfirmed => Event::ClearNetworkConfirmed {
                confirmation: ConfirmationId(self.confirmation_id),
            },
            MicroEventKind::ClearNetworkCompleted => Event::ClearNetworkCompleted {
                confirmation: ConfirmationId(self.confirmation_id),
                result: result_from_wire(self.result, self.failure)?,
            },
            MicroEventKind::FactoryResetRequested => Event::FactoryResetRequested,
            MicroEventKind::FactoryResetConfirmed => Event::FactoryResetConfirmed {
                confirmation: ConfirmationId(self.confirmation_id),
            },
            MicroEventKind::FactoryResetCompleted => Event::FactoryResetCompleted {
                confirmation: ConfirmationId(self.confirmation_id),
                result: result_from_wire(self.result, self.failure)?,
            },
            MicroEventKind::RebootRequested => Event::RebootRequested,
        };
        Ok(event)
    }

    fn validate_canonical(&self) -> Result<(), MicroErrorCode> {
        let allowed = match self.kind {
            MicroEventKind::BootSampled | MicroEventKind::NetworkConfigLoaded => {
                [false, false, false, false, true, false, false, false]
            }
            MicroEventKind::StorageInitialized
            | MicroEventKind::ProfileValidated
            | MicroEventKind::DisplayInitialized
            | MicroEventKind::SystemUiInitialized => {
                [true, true, false, false, false, false, false, false]
            }
            MicroEventKind::OpenApp => [false, false, false, true, false, false, false, false],
            MicroEventKind::SetBacklight => [false, false, false, false, true, false, false, false],
            MicroEventKind::AppStarted | MicroEventKind::AppStopped => {
                [false, false, false, false, false, true, false, false]
            }
            MicroEventKind::AppFailed => [false, true, false, false, false, true, false, false],
            MicroEventKind::WifiScanCompleted
            | MicroEventKind::WifiConnected
            | MicroEventKind::WifiPersisted
            | MicroEventKind::ReconnectDue => {
                [false, false, false, false, false, false, true, false]
            }
            MicroEventKind::WifiScanFailed | MicroEventKind::WifiFailed => {
                [false, false, true, false, false, false, true, false]
            }
            MicroEventKind::ClearNetworkConfirmed | MicroEventKind::FactoryResetConfirmed => {
                [false, false, false, false, false, false, false, true]
            }
            MicroEventKind::ClearNetworkCompleted | MicroEventKind::FactoryResetCompleted => {
                [true, true, false, false, false, false, false, true]
            }
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
            | MicroEventKind::RebootRequested => [false; 8],
        };
        let invalid = (!allowed[0] && self.result != MicroResult::Unused)
            || (!allowed[1] && self.failure != MicroFailureReason::Unused)
            || (!allowed[2] && self.wifi_failure != MicroWifiFailure::Unused)
            || (!allowed[3] && self.app != MicroAppId::Unused)
            || (!allowed[4] && self.flag != 0)
            || (!allowed[5] && self.session_id != 0)
            || (!allowed[6] && self.operation_id != 0)
            || (!allowed[7] && self.confirmation_id != 0)
            || self.after_secs != 0
            || self.reserved != 0;
        if invalid {
            return Err(MicroErrorCode::InvalidArgument);
        }
        if allowed[0] {
            match self.result {
                MicroResult::Ok if self.failure == MicroFailureReason::Unused => {}
                MicroResult::Err if self.failure != MicroFailureReason::Unused => {}
                _ => return Err(MicroErrorCode::InvalidArgument),
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_raw(
        kind: u32,
        result: u32,
        failure: u32,
        wifi_failure: u32,
        app: u32,
        flag: u32,
        after_secs: u32,
        reserved: u32,
        session_id: u64,
        operation_id: u64,
        confirmation_id: u64,
    ) -> Result<Self, MicroErrorCode> {
        if reserved != 0 || after_secs != 0 {
            return Err(MicroErrorCode::InvalidArgument);
        }
        Ok(Self {
            kind: event_kind_from_raw(kind)?,
            result: result_kind_from_raw(result)?,
            failure: failure_kind_from_raw(failure)?,
            wifi_failure: wifi_failure_kind_from_raw(wifi_failure)?,
            app: app_kind_from_raw(app)?,
            flag,
            after_secs,
            reserved,
            session_id,
            operation_id,
            confirmation_id,
        })
    }
}

fn event_kind_from_raw(value: u32) -> Result<MicroEventKind, MicroErrorCode> {
    Ok(match value {
        0 => MicroEventKind::BootSampled,
        1 => MicroEventKind::StorageInitialized,
        2 => MicroEventKind::ProfileValidated,
        3 => MicroEventKind::DisplayInitialized,
        4 => MicroEventKind::SystemUiInitialized,
        5 => MicroEventKind::NetworkConfigLoaded,
        6 => MicroEventKind::SetupSkipped,
        7 => MicroEventKind::OpenSettings,
        8 => MicroEventKind::BackPressed,
        9 => MicroEventKind::HomePressed,
        10 => MicroEventKind::OpenApp,
        11 => MicroEventKind::AppStarted,
        12 => MicroEventKind::AppFailed,
        13 => MicroEventKind::RestartApp,
        14 => MicroEventKind::AppStopped,
        15 => MicroEventKind::WifiScanRequested,
        16 => MicroEventKind::WifiScanCompleted,
        17 => MicroEventKind::WifiScanFailed,
        18 => MicroEventKind::WifiConnectRequested,
        19 => MicroEventKind::WifiConnected,
        20 => MicroEventKind::WifiPersisted,
        21 => MicroEventKind::WifiFailed,
        22 => MicroEventKind::ReconnectDue,
        23 => MicroEventKind::ReconnectNowRequested,
        24 => MicroEventKind::ClearNetworkRequested,
        25 => MicroEventKind::ClearNetworkConfirmed,
        26 => MicroEventKind::ClearNetworkCompleted,
        27 => MicroEventKind::FactoryResetRequested,
        28 => MicroEventKind::FactoryResetConfirmed,
        29 => MicroEventKind::FactoryResetCompleted,
        30 => MicroEventKind::RebootRequested,
        31 => MicroEventKind::SetBacklight,
        _ => return Err(MicroErrorCode::InvalidArgument),
    })
}

fn result_kind_from_raw(value: u32) -> Result<MicroResult, MicroErrorCode> {
    match value {
        0 => Ok(MicroResult::Unused),
        1 => Ok(MicroResult::Ok),
        2 => Ok(MicroResult::Err),
        _ => Err(MicroErrorCode::InvalidArgument),
    }
}

fn failure_kind_from_raw(value: u32) -> Result<MicroFailureReason, MicroErrorCode> {
    match value {
        0 => Ok(MicroFailureReason::Unused),
        1 => Ok(MicroFailureReason::SafeModeRequested),
        2 => Ok(MicroFailureReason::StorageCorrupt),
        3 => Ok(MicroFailureReason::InvalidBoardProfile),
        4 => Ok(MicroFailureReason::HardwareUnavailable),
        5 => Ok(MicroFailureReason::AppCrashed),
        6 => Ok(MicroFailureReason::Internal),
        _ => Err(MicroErrorCode::InvalidArgument),
    }
}

fn wifi_failure_kind_from_raw(value: u32) -> Result<MicroWifiFailure, MicroErrorCode> {
    match value {
        0 => Ok(MicroWifiFailure::Unused),
        1 => Ok(MicroWifiFailure::Authentication),
        2 => Ok(MicroWifiFailure::NetworkMissing),
        3 => Ok(MicroWifiFailure::Timeout),
        4 => Ok(MicroWifiFailure::Internal),
        _ => Err(MicroErrorCode::InvalidArgument),
    }
}

fn app_kind_from_raw(value: u32) -> Result<MicroAppId, MicroErrorCode> {
    match value {
        0 => Ok(MicroAppId::Unused),
        1 => Ok(MicroAppId::Counter),
        _ => Err(MicroErrorCode::InvalidArgument),
    }
}

fn backlight_kind_from_raw(value: u32) -> Result<MicroBacklight, MicroErrorCode> {
    match value {
        0 => Ok(MicroBacklight::Unused),
        1 => Ok(MicroBacklight::Off),
        2 => Ok(MicroBacklight::Low),
        3 => Ok(MicroBacklight::Medium),
        4 => Ok(MicroBacklight::High),
        _ => Err(MicroErrorCode::InvalidArgument),
    }
}

fn bool_flag(value: u32) -> Result<bool, MicroErrorCode> {
    match value {
        0 => Ok(false),
        1 => Ok(true),
        _ => Err(MicroErrorCode::InvalidArgument),
    }
}

fn set_result(wire: &mut MicroEvent, result: &Result<(), FailureReason>) {
    match result {
        Ok(()) => wire.result = MicroResult::Ok,
        Err(reason) => {
            wire.result = MicroResult::Err;
            wire.failure = failure_to_wire(reason);
        }
    }
}

fn result_from_wire(
    result: MicroResult,
    failure: MicroFailureReason,
) -> Result<Result<(), FailureReason>, MicroErrorCode> {
    match result {
        MicroResult::Ok => Ok(Ok(())),
        MicroResult::Err => Ok(Err(failure_from_wire(failure)?)),
        MicroResult::Unused => Err(MicroErrorCode::InvalidArgument),
    }
}

pub(crate) fn map_state(state: &State) -> MicroState {
    match state {
        State::EarlyBoot => MicroState::EarlyBoot,
        State::SafeMode => MicroState::SafeMode,
        State::StorageReady => MicroState::StorageReady,
        State::BoardProfileValidated => MicroState::BoardProfileValidated,
        State::DisplayReady => MicroState::DisplayReady,
        State::SystemUiReady => MicroState::SystemUiReady,
        State::FirstRunSetup => MicroState::FirstRunSetup,
        State::Launcher => MicroState::Launcher,
        State::AppStarting { .. } => MicroState::AppStarting,
        State::AppRunning { .. } => MicroState::AppRunning,
        State::AppStopping { .. } => MicroState::AppStopping,
        State::AppError { .. } => MicroState::AppError,
        State::Settings => MicroState::Settings,
    }
}

#[must_use]
pub fn encode_action_batch(action: &Action) -> Vec<MicroAction> {
    let mut output = Vec::with_capacity(action_count(action));
    encode_action(action, &mut output);
    output
}

pub fn encode_action_into(
    action: &Action,
    output: &mut [MicroAction],
) -> Result<usize, DispatchError> {
    let required = action_count(action);
    if output.len() < required {
        return Err(DispatchError {
            code: MicroErrorCode::BufferTooSmall,
            required,
        });
    }
    let encoded = encode_action_batch(action);
    let destination = output.get_mut(..required).ok_or(DispatchError {
        code: MicroErrorCode::InvalidArgument,
        required,
    })?;
    if destination.len() != encoded.len() {
        return Err(DispatchError {
            code: MicroErrorCode::Runtime,
            required,
        });
    }
    destination.copy_from_slice(&encoded);
    Ok(required)
}

fn action_count(action: &Action) -> usize {
    match action {
        Action::Actions(actions) => 1 + actions.iter().map(action_count).sum::<usize>(),
        _ => 1,
    }
}

fn encode_action(action: &Action, output: &mut Vec<MicroAction>) {
    let mut wire = MicroAction::new(match action {
        Action::None => MicroActionKind::None,
        Action::Rejected => MicroActionKind::Rejected,
        Action::Actions(_) => MicroActionKind::Actions,
        Action::EnterSafeMode(_) => MicroActionKind::EnterSafeMode,
        Action::InitializeStorage => MicroActionKind::InitializeStorage,
        Action::ValidateProfile => MicroActionKind::ValidateProfile,
        Action::InitializeDisplay => MicroActionKind::InitializeDisplay,
        Action::InitializeSystemUi => MicroActionKind::InitializeSystemUi,
        Action::LoadNetworkConfig => MicroActionKind::LoadNetworkConfig,
        Action::ShowFirstRunSetup => MicroActionKind::ShowFirstRunSetup,
        Action::ShowLauncher => MicroActionKind::ShowLauncher,
        Action::ShowSettings => MicroActionKind::ShowSettings,
        Action::ApplyBacklight(_) => MicroActionKind::ApplyBacklight,
        Action::StartWifiScan { .. } => MicroActionKind::StartWifiScan,
        Action::ConnectWifi { .. } => MicroActionKind::ConnectWifi,
        Action::ConnectSavedWifi { .. } => MicroActionKind::ConnectSavedWifi,
        Action::PersistWifi { .. } => MicroActionKind::PersistWifi,
        Action::ClearPendingWifi { .. } => MicroActionKind::ClearPendingWifi,
        Action::ScheduleWifiReconnect { .. } => MicroActionKind::ScheduleWifiReconnect,
        Action::StartApp { .. } => MicroActionKind::StartApp,
        Action::StopApp { .. } => MicroActionKind::StopApp,
        Action::ShowAppError { .. } => MicroActionKind::ShowAppError,
        Action::ConfirmClearNetwork { .. } => MicroActionKind::ConfirmClearNetwork,
        Action::ClearNetwork { .. } => MicroActionKind::ClearNetwork,
        Action::ConfirmFactoryReset { .. } => MicroActionKind::ConfirmFactoryReset,
        Action::FactoryReset { .. } => MicroActionKind::FactoryReset,
        Action::Reboot => MicroActionKind::Reboot,
    });
    match action {
        Action::Actions(actions) => {
            wire.child_count = u32::try_from(actions.len()).unwrap_or(u32::MAX)
        }
        Action::EnterSafeMode(reason) => wire.failure = failure_to_wire(reason),
        Action::ApplyBacklight(backlight) => wire.backlight = backlight_to_wire(backlight),
        Action::StartWifiScan { operation }
        | Action::ConnectWifi { operation }
        | Action::ConnectSavedWifi { operation }
        | Action::PersistWifi { operation }
        | Action::ClearPendingWifi { operation } => wire.operation_id = operation.0,
        Action::ScheduleWifiReconnect {
            reconnect,
            after_secs,
        } => {
            wire.operation_id = reconnect.0;
            wire.after_secs = *after_secs;
        }
        Action::StartApp { app, session } | Action::StopApp { app, session } => {
            wire.app = app_to_wire(app);
            wire.session_id = session.0;
        }
        Action::ShowAppError {
            app,
            session,
            reason,
        } => {
            wire.app = app_to_wire(app);
            wire.session_id = session.0;
            wire.failure = failure_to_wire(reason);
        }
        Action::ConfirmClearNetwork { confirmation }
        | Action::ClearNetwork { confirmation }
        | Action::ConfirmFactoryReset { confirmation }
        | Action::FactoryReset { confirmation } => wire.confirmation_id = confirmation.0,
        Action::None
        | Action::Rejected
        | Action::InitializeStorage
        | Action::ValidateProfile
        | Action::InitializeDisplay
        | Action::InitializeSystemUi
        | Action::LoadNetworkConfig
        | Action::ShowFirstRunSetup
        | Action::ShowLauncher
        | Action::ShowSettings
        | Action::Reboot => {}
    }
    output.push(wire);
    if let Action::Actions(actions) = action {
        for child in actions {
            encode_action(child, output);
        }
    }
}

pub fn decode_action_batch(actions: &[MicroAction]) -> Result<Action, MicroErrorCode> {
    let mut cursor = 0;
    let action = decode_action(actions, &mut cursor)?;
    if cursor == actions.len() {
        Ok(action)
    } else {
        Err(MicroErrorCode::InvalidArgument)
    }
}

fn decode_action(actions: &[MicroAction], cursor: &mut usize) -> Result<Action, MicroErrorCode> {
    let wire = actions
        .get(*cursor)
        .ok_or(MicroErrorCode::InvalidArgument)?;
    *cursor += 1;
    let action = match wire.kind {
        MicroActionKind::None => Action::None,
        MicroActionKind::Rejected => Action::Rejected,
        MicroActionKind::Actions => {
            let mut children = Vec::new();
            children
                .try_reserve(wire.child_count as usize)
                .map_err(|_| MicroErrorCode::Runtime)?;
            for _ in 0..wire.child_count {
                children.push(decode_action(actions, cursor)?);
            }
            Action::Actions(children)
        }
        MicroActionKind::EnterSafeMode => Action::EnterSafeMode(failure_from_wire(wire.failure)?),
        MicroActionKind::InitializeStorage => Action::InitializeStorage,
        MicroActionKind::ValidateProfile => Action::ValidateProfile,
        MicroActionKind::InitializeDisplay => Action::InitializeDisplay,
        MicroActionKind::InitializeSystemUi => Action::InitializeSystemUi,
        MicroActionKind::LoadNetworkConfig => Action::LoadNetworkConfig,
        MicroActionKind::ShowFirstRunSetup => Action::ShowFirstRunSetup,
        MicroActionKind::ShowLauncher => Action::ShowLauncher,
        MicroActionKind::ShowSettings => Action::ShowSettings,
        MicroActionKind::ApplyBacklight => {
            Action::ApplyBacklight(backlight_from_wire(wire.backlight)?)
        }
        MicroActionKind::StartWifiScan => Action::StartWifiScan {
            operation: WifiOperationId(wire.operation_id),
        },
        MicroActionKind::ConnectWifi => Action::ConnectWifi {
            operation: WifiOperationId(wire.operation_id),
        },
        MicroActionKind::ConnectSavedWifi => Action::ConnectSavedWifi {
            operation: WifiOperationId(wire.operation_id),
        },
        MicroActionKind::PersistWifi => Action::PersistWifi {
            operation: WifiOperationId(wire.operation_id),
        },
        MicroActionKind::ClearPendingWifi => Action::ClearPendingWifi {
            operation: WifiOperationId(wire.operation_id),
        },
        MicroActionKind::ScheduleWifiReconnect => Action::ScheduleWifiReconnect {
            reconnect: WifiOperationId(wire.operation_id),
            after_secs: wire.after_secs,
        },
        MicroActionKind::StartApp => Action::StartApp {
            app: app_from_wire(wire.app)?,
            session: AppSessionId(wire.session_id),
        },
        MicroActionKind::StopApp => Action::StopApp {
            app: app_from_wire(wire.app)?,
            session: AppSessionId(wire.session_id),
        },
        MicroActionKind::ShowAppError => Action::ShowAppError {
            app: app_from_wire(wire.app)?,
            session: AppSessionId(wire.session_id),
            reason: failure_from_wire(wire.failure)?,
        },
        MicroActionKind::ConfirmClearNetwork => Action::ConfirmClearNetwork {
            confirmation: ConfirmationId(wire.confirmation_id),
        },
        MicroActionKind::ClearNetwork => Action::ClearNetwork {
            confirmation: ConfirmationId(wire.confirmation_id),
        },
        MicroActionKind::ConfirmFactoryReset => Action::ConfirmFactoryReset {
            confirmation: ConfirmationId(wire.confirmation_id),
        },
        MicroActionKind::FactoryReset => Action::FactoryReset {
            confirmation: ConfirmationId(wire.confirmation_id),
        },
        MicroActionKind::Reboot => Action::Reboot,
    };
    Ok(action)
}
