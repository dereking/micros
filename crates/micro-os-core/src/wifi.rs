#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiOperationId(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WifiFailure {
    Authentication,
    NetworkMissing,
    Timeout,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LiveWifiState {
    Disconnected,
    Connecting(WifiOperationId),
    Connected,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProvisioningState {
    Idle,
    Scanning(WifiOperationId),
    ConnectingReplacement(WifiOperationId),
    Persisting(WifiOperationId),
    Failed {
        operation: WifiOperationId,
        reason: WifiFailure,
    },
}
