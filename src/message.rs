use crate::connection_status::ConnectionStatus;

#[derive(Clone)]
pub(crate) enum CombinedMessage {
    Message(Message),
    LogMessage(LogMessage),
}

#[derive(Clone)]
pub enum Message {
    TextMessage(TextMessage),
}

#[derive(Clone)]
pub struct TextMessage {
    pub content: String,
    pub sent: bool,
}

#[derive(Clone)]
pub enum LogMessage {
    ConnectionMessage(ConnectionMessage),
}

#[derive(Clone)]
pub struct ConnectionMessage {
    pub connection_status: ConnectionStatus,
}
