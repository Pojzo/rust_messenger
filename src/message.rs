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

pub fn construct_text_message(content: String, sent: bool) -> CombinedMessage {
    CombinedMessage::Message(Message::TextMessage(TextMessage { content, sent }))
}

pub fn construct_connection_message(connection_status: ConnectionStatus) -> CombinedMessage {
    CombinedMessage::LogMessage(LogMessage::ConnectionMessage(ConnectionMessage {
        connection_status,
    }))
}
