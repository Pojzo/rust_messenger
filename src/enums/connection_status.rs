use std::fmt;

#[derive(PartialEq, Eq, Clone)]
pub(crate) enum ConnectionStatus {
    DISCONNECTED,
    LISTENING,
    CONNECTING,
    CONNECTED,
    FAILED,
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
            ConnectionStatus::FAILED => write!(f, "Failed"),
            ConnectionStatus::LISTENING => write!(f, "Listening"),
        }
    }
}
