use micro_os_core::{Action, Event, FailureReason, MicroOs, State};

#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroErrorCode {
    Ok = 0,
    Mbc = 1,
    Runtime = 2,
    Ui = 3,
    InvalidArgument = 4,
    Panic = 5,
    Stopped = 6,
}

#[repr(u32)]
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

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroEventKind {
    BootNormal = 0,
    BootSafeMode = 1,
    StorageReady = 2,
    StorageFailed = 3,
    ProfileValid = 4,
    ProfileInvalid = 5,
    DisplayReady = 6,
    DisplayFailed = 7,
    SystemUiReady = 8,
    SystemUiFailed = 9,
    NetworkConfigured = 10,
    NetworkUnconfigured = 11,
    SetupSkipped = 12,
    OpenSettings = 13,
    BackPressed = 14,
    HomePressed = 15,
    RebootRequested = 16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MicroEvent {
    pub kind: u32,
}

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicroAction {
    None = 0,
    Rejected = 1,
    EnterSafeMode = 2,
    InitializeStorage = 3,
    ValidateProfile = 4,
    InitializeDisplay = 5,
    InitializeSystemUi = 6,
    LoadNetworkConfig = 7,
    ShowFirstRunSetup = 8,
    ShowLauncher = 9,
    ShowSettings = 10,
    ConnectSavedWifi = 11,
    Reboot = 12,
    Composite = 13,
    Other = 14,
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

pub(crate) fn dispatch(os: &mut MicroOs, event: MicroEvent) -> MicroAction {
    let event = match event.kind {
        value if value == MicroEventKind::BootNormal as u32 => {
            Event::BootSampled { safe_mode: false }
        }
        value if value == MicroEventKind::BootSafeMode as u32 => {
            Event::BootSampled { safe_mode: true }
        }
        value if value == MicroEventKind::StorageReady as u32 => Event::StorageInitialized(Ok(())),
        value if value == MicroEventKind::StorageFailed as u32 => {
            Event::StorageInitialized(Err(FailureReason::StorageCorrupt))
        }
        value if value == MicroEventKind::ProfileValid as u32 => Event::ProfileValidated(Ok(())),
        value if value == MicroEventKind::ProfileInvalid as u32 => {
            Event::ProfileValidated(Err(FailureReason::InvalidBoardProfile))
        }
        value if value == MicroEventKind::DisplayReady as u32 => Event::DisplayInitialized(Ok(())),
        value if value == MicroEventKind::DisplayFailed as u32 => {
            Event::DisplayInitialized(Err(FailureReason::HardwareUnavailable))
        }
        value if value == MicroEventKind::SystemUiReady as u32 => {
            Event::SystemUiInitialized(Ok(()))
        }
        value if value == MicroEventKind::SystemUiFailed as u32 => {
            Event::SystemUiInitialized(Err(FailureReason::HardwareUnavailable))
        }
        value if value == MicroEventKind::NetworkConfigured as u32 => {
            Event::NetworkConfigLoaded { configured: true }
        }
        value if value == MicroEventKind::NetworkUnconfigured as u32 => {
            Event::NetworkConfigLoaded { configured: false }
        }
        value if value == MicroEventKind::SetupSkipped as u32 => Event::SetupSkipped,
        value if value == MicroEventKind::OpenSettings as u32 => Event::OpenSettings,
        value if value == MicroEventKind::BackPressed as u32 => Event::BackPressed,
        value if value == MicroEventKind::HomePressed as u32 => Event::HomePressed,
        value if value == MicroEventKind::RebootRequested as u32 => Event::RebootRequested,
        _ => return MicroAction::Rejected,
    };
    map_action(os.dispatch(event))
}

fn map_action(action: Action) -> MicroAction {
    match action {
        Action::None => MicroAction::None,
        Action::Rejected => MicroAction::Rejected,
        Action::EnterSafeMode(_) => MicroAction::EnterSafeMode,
        Action::InitializeStorage => MicroAction::InitializeStorage,
        Action::ValidateProfile => MicroAction::ValidateProfile,
        Action::InitializeDisplay => MicroAction::InitializeDisplay,
        Action::InitializeSystemUi => MicroAction::InitializeSystemUi,
        Action::LoadNetworkConfig => MicroAction::LoadNetworkConfig,
        Action::ShowFirstRunSetup => MicroAction::ShowFirstRunSetup,
        Action::ShowLauncher => MicroAction::ShowLauncher,
        Action::ShowSettings => MicroAction::ShowSettings,
        Action::ConnectSavedWifi { .. } => MicroAction::ConnectSavedWifi,
        Action::Reboot => MicroAction::Reboot,
        Action::Actions(_) => MicroAction::Composite,
        _ => MicroAction::Other,
    }
}
