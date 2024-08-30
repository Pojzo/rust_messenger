use std::fmt;

#[derive(PartialEq, Eq)]
pub(crate) enum ConnectionStatus {
    DISCONNECTED,
    CONNECTING,
    CONNECTED,
}

impl Default for ConnectionStatus {
    fn default() -> Self {
        ConnectionStatus::DISCONNECTED
    }
}

impl fmt::Display for ConnectionStatus {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ConnectionStatus::DISCONNECTED => write!(f, "Disconnected"),
            ConnectionStatus::CONNECTED => write!(f, "Connected"),
            ConnectionStatus::CONNECTING => write!(f, "Connecting"),
        }
    }
}
