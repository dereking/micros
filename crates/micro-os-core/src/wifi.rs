#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiFailure {
    Authentication,
    NetworkMissing,
    Timeout,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiState {
    Idle,
    Scanning,
    Connecting,
    PendingPersistence,
    Connected,
    Failed(WifiFailure),
}
