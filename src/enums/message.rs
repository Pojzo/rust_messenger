use crate::enums::connection_status::ConnectionStatus;

#[derive(Clone)]
pub(crate) enum CombinedMessage {
    Message(Message),
    LogMessage(LogMessage),
}

#[derive(Clone)]
pub enum Message {
    TextMessage(TextMessage),
}

pub enum MessageType {
    TEXT,
    IMAGE,
}

impl Message {
    pub fn to_int(&self) -> u8 {
        match self {
            Message::TextMessage(_) => 1,
        }
    }
}

#[derive(Clone)]
pub struct TextMessage {
    pub content: String,
    pub time_sent: String,
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

pub fn construct_text_message_generic(content: String, sent: bool) -> CombinedMessage {
    let time_now = chrono::Local::now().format("%H:%M:%S").to_string();
    CombinedMessage::Message(Message::TextMessage(TextMessage {
        content,
        sent,
        time_sent: time_now,
    }))
}

pub fn construct_text_message(content: String, sent: bool) -> Message {
    let combined_message = construct_text_message_generic(content, sent);
    let message = match combined_message {
        CombinedMessage::Message(Message::TextMessage(message)) => {
            Message::TextMessage(TextMessage {
                content: message.content,
                time_sent: message.time_sent,
                sent: message.sent,
            })
        }
        _ => panic!("Unexpected message type"),
    };
    message
}

pub fn construct_connection_message(connection_status: ConnectionStatus) -> CombinedMessage {
    CombinedMessage::LogMessage(LogMessage::ConnectionMessage(ConnectionMessage {
        connection_status,
    }))
}
