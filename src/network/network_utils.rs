use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};

use crate::enums::{
    connection_status::ConnectionStatus,
    message::{
        construct_connection_message, construct_text_message_generic, CombinedMessage, Message,
        MessageType,
    },
};

pub async fn handle_connection(
    stream: tokio::net::TcpStream,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    rx: Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    let (read_stream, write_stream) = stream.into_split();

    {
        let connect_message = construct_connection_message(ConnectionStatus::CONNECTED);
        let tx_guard = tx.lock().await;
        tx_guard.send(connect_message).await.unwrap();
    }

    let read_handle = tokio::spawn(stream_read(
        read_stream,
        tx.clone(),
        disconnect_notify.clone(),
    ));

    let write_handle = tokio::spawn(stream_write(
        write_stream,
        rx.clone(),
        tx.clone(),
        disconnect_notify.clone(),
    ));

    let (read_result, write_result) = tokio::join!(read_handle, write_handle);

    if let Err(e) = read_result {
        eprintln!("Error in read task: {:?}", e);
    }
    if let Err(e) = write_result {
        eprintln!("Error in write task: {:?}", e);
    }
    println!("Got to the end of the connection handler");
}

pub async fn handle_disconnect_from_source(
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    disconnect_notify.notify_one();
    let disconnect_message = construct_connection_message(ConnectionStatus::DISCONNECTED);
    let tx_guard = tx.lock().await;
    tx_guard.send(disconnect_message).await.unwrap();
}

pub async fn send_profile_picture(write_stream: &mut OwnedWriteHalf) {
    // let image = include_bytes!("../../data/server.jpg");
    // let image_str = String::from_utf8_lossy(image).to_string();

    // let payload = construct_payload(1, MessageType::IMAGE, image_str);
    // match write_stream.write_all(payload.as_bytes()).await {
    //     Ok(_) => {
    //         println!("Sent image");
    //     }
    //     Err(e) => {
    //         eprintln!("Failed to write to connection: {}", e);
    //     }
    // }
}

pub async fn stream_read(
    mut read_stream: OwnedReadHalf,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    // let mut buffer = [0; 512];
    let mut buffer = Vec::with_capacity(50_000_000);
    buffer.resize(50_000_000, 0);
    {
        let connect_message = construct_connection_message(ConnectionStatus::CONNECTED);
        let tx_guard = tx.lock().await;
        tx_guard.send(connect_message).await.unwrap();
    }
    loop {
        tokio::select! {
            result = read_stream.read(&mut buffer) => {
                match result {
                    Ok(0) => {
                        println!("Connection closed by the client.");
                        handle_disconnect(&tx, &disconnect_notify).await;
                        break;
                    }
                    Ok(n) => {
                        let tx_guard = tx.lock().await;
                        let buffer = &buffer[..n];
                        let payload = deconstruct_payload(String::from_utf8_lossy(buffer).to_string());
                        let content = payload.text_protocol.unwrap().content;

                        // let content = String::from_utf8_lossy(&buffer[..n]).to_string();
                        println!("Read {} bytes", n);

                        if content.trim() == "<DISCONNECT>" {
                            println!("Received disconnect message");
                            handle_disconnect(&tx, &disconnect_notify).await;
                            break;
                        }

                        if payload.message_type as u8 == MessageType::IMAGE as u8 {
                            println!("Received image");
                            continue;
                        }

                        let message = construct_text_message_generic(content.clone(), false);

                        match tx_guard.send(message).await {
                            Ok(_) => {}
                            Err(e) => {
                                println!("Couldn't send message: {}", e);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to read from connection: {}", e);
                        break;
                    }
                }
            }
            _ = disconnect_notify.notified() => {
                println!("Sending disconnect message to UI ");
                handle_disconnect(&tx, &disconnect_notify).await;
                break;
            }
        }
    }
}

async fn handle_disconnect(
    tx: &Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    disconnect_notify: &Arc<Notify>,
) {
    let disconnect_message = construct_connection_message(ConnectionStatus::DISCONNECTED);
    let tx_guard = tx.lock().await;
    tx_guard.send(disconnect_message).await.unwrap();
    disconnect_notify.notify_one();
}

pub async fn stream_write(
    mut write_stream: OwnedWriteHalf,
    rx: Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    send_profile_picture(&mut write_stream).await; // Pass mutable reference
    loop {
        let mut message_guard = rx.lock().await;

        tokio::select! {
            _ = disconnect_notify.notified() => {
                // Send message to the client that the connection is being closed
                let disconnect_message_string = "<DISCONNECT>".to_string();
                match write_stream
                    .write_all(disconnect_message_string.as_bytes())
                    .await
                {
                    Ok(_) => {
                        println!("Sent disconnect message");
                    }
                    Err(e) => {
                        eprintln!("Failed to write to connection: {}", e);
                    }
                }
                // Send message to the UI that the connection is being closed
                let tx_guard = tx.lock().await;
                let disconnect_message = construct_connection_message(ConnectionStatus::DISCONNECTED);
                tx_guard.send(disconnect_message).await.unwrap();
                break

            }
            Some(generic_message) = message_guard.recv() => {
                let content = match generic_message {
                    CombinedMessage::Message(Message::TextMessage(message)) => message.content,
                    _ => todo!(),
                };

                let payload = construct_payload(1, MessageType::TEXT, content.clone());
                match write_stream.write_all(payload.as_bytes()).await {
                    Ok(_) => {
                        println!("Sent message: {}", content);
                    }
                    Err(e) => {
                        eprintln!("Failed to write to connection: {}", e);
                        break;
                    }
                }
            }
        }
    }
}

fn pad_string(string: &str, chunk_size: usize) -> String {
    let mut new_string = string.to_string();

    let add_size = chunk_size - (string.len() % chunk_size);

    if add_size != chunk_size {
        for _ in 0..add_size {
            new_string.push('0');
        }
    }

    return new_string;
}

/*
---- version ---- | ---- message type ---- | ---- offset ---- | ---- payload_len ----
        4 bits   |          4 bits        |           8 bits         |     20 bits
---- checksum ------------------------------------------------------------------
      8 bits    24 0-bits

---- chunk1 ---- | ---- chunk2 ----
      16 bits        16 bits

 */

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct TextProtocol {
    pub content: String,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct ImageProtocol {
    pub content: String,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Protocol {
    pub version: u8,
    pub message_type: u8,
    pub payload: String,

    pub text_protocol: Option<TextProtocol>,
    pub image_protocol: Option<ImageProtocol>,
}

impl Protocol {
    pub fn new_text(version: u8, payload: String) -> Protocol {
        let message_type = MessageType::TEXT as u8;

        let text_protocol = Some(TextProtocol {
            content: payload.clone(),
        });
        let image_protocol = None;

        let payload_len = payload.len() as u32;

        Protocol {
            version,
            message_type,
            payload,
            text_protocol,
            image_protocol,
        }
    }

    pub fn new_image(version: u8, payload: String, width: u16, height: u16) -> Protocol {
        let message_type = MessageType::IMAGE as u8;

        let text_protocol = None;
        let image_protocol = Some(ImageProtocol {
            content: payload.clone(),
            width,
            height,
        });

        Protocol {
            version,
            message_type,
            payload,
            text_protocol,
            image_protocol,
        }
    }

    pub fn serialize(&self) -> String {
        match self.message_type {
            0 => self.serialize_text(),
            1 => self.serialize_image(),
            _ => panic!("Unknown message type"),
        }
    }

    pub fn serialize_text(&self) -> String {
        let mut serialized_payload = String::new();

        let version_bytes = to_4bit_string(self.version);
        let message_type_bytes = to_4bit_string(self.message_type);

        let padded_payload = pad_string(&self.payload, 16);

        let offset = padded_payload.len() - self.payload.len();
        let offset_bytes = to_4bit_string(offset as u8);

        let payload_len = to_32bit_string(padded_payload.len() as u32);

        serialized_payload.push_str(&version_bytes);
        serialized_payload.push_str(&message_type_bytes);
        serialized_payload.push_str(&offset_bytes);
        serialized_payload.push_str(&payload_len);
        serialized_payload.push_str(&padded_payload);

        serialized_payload
    }

    pub fn serialize_image(&self) -> String {
        let mut serialized_payload = String::new();

        let version_bytes = to_4bit_string(self.version);
        let message_type_bytes = to_4bit_string(self.message_type);

        let offset = self.payload.len() - self.image_protocol.as_ref().unwrap().content.len();
        let offset_bytes = to_4bit_string(offset as u8);

        let padded_payload = pad_string(&self.payload, 16);
        let payload_len_bytes = to_32bit_string(self.payload.len() as u32);

        let width = self.image_protocol.as_ref().unwrap().width;
        let height = self.image_protocol.as_ref().unwrap().height;

        let width_bytes = to_16bit_string(width);
        let height_bytes = to_16bit_string(height);

        serialized_payload.push_str(&version_bytes);
        serialized_payload.push_str(&message_type_bytes);
        serialized_payload.push_str(&offset_bytes);
        serialized_payload.push_str(&payload_len_bytes);
        serialized_payload.push_str(&width_bytes);
        serialized_payload.push_str(&height_bytes);
        serialized_payload.push_str(&padded_payload);

        serialized_payload
    }

    pub fn deserialize(serialized_payload: String) -> Protocol {
        let message_type_str = &serialized_payload[4..8];

        let message_type = u8::from_str_radix(message_type_str, 2).unwrap();
        println!("Message type: {}", message_type);

        match message_type {
            0 => Protocol::deserialize_text(serialized_payload),
            1 => Protocol::deserialize_image(serialized_payload),
            _ => panic!("Unknown message type: {}", message_type),
        }
    }

    pub fn deserialize_text(serialized_payload: String) -> Protocol {
        let version = u8::from_str_radix(&serialized_payload[0..4], 2).unwrap();
        let message_type = u8::from_str_radix(&serialized_payload[4..8], 2).unwrap();
        let offset = u8::from_str_radix(&serialized_payload[8..12], 2).unwrap();

        let payload_len = u32::from_str_radix(&serialized_payload[12..44], 2).unwrap();
        let payload_start = 44;
        let payload_end = payload_start + payload_len as usize;

        let full_payload = &serialized_payload[payload_start..payload_end];
        let payload = &full_payload[..(full_payload.len() - offset as usize)];

        let text_protocol = Some(TextProtocol {
            content: payload.to_string(),
        });
        let image_protocol = None;

        Protocol {
            version,
            message_type,
            payload: payload.to_string(),
            text_protocol,
            image_protocol,
        }
    }

    pub fn deserialize_image(serialized_payload: String) -> Protocol {
        let version = u8::from_str_radix(&serialized_payload[0..4], 2).unwrap();
        let message_type = u8::from_str_radix(&serialized_payload[4..8], 2).unwrap();
        let offset = u8::from_str_radix(&serialized_payload[8..12], 2).unwrap();

        let payload_len = u32::from_str_radix(&serialized_payload[12..44], 2).unwrap();
        let width = u16::from_str_radix(&serialized_payload[44..60], 2).unwrap();
        let height = u16::from_str_radix(&serialized_payload[60..76], 2).unwrap();

        let payload_start = 76;
        let payload_end = payload_start + payload_len as usize;

        let full_payload = &serialized_payload[payload_start..payload_end];
        let payload = &full_payload[..(full_payload.len() - offset as usize)];

        let text_protocol = None;
        let image_protocol = Some(ImageProtocol {
            content: payload.to_string(),
            width,
            height,
        });

        Protocol {
            version,
            message_type,
            payload: payload.to_string(),
            text_protocol,
            image_protocol,
        }
    }
}

// Helper functions to convert integers to binary strings
fn to_4bit_string(value: u8) -> String {
    format!("{:04b}", value)
}

fn to_8bit_string(value: u8) -> String {
    format!("{:08b}", value)
}

fn to_32bit_string(value: u32) -> String {
    format!("{:032b}", value)
}

fn to_16bit_string(value: u16) -> String {
    format!("{:016b}", value)
}

pub fn construct_payload(version: u8, message_type: MessageType, payload: String) -> String {
    match message_type {
        MessageType::TEXT => {
            let protocol = Protocol::new_text(version, payload);
            protocol.serialize()
        }
        MessageType::IMAGE => {
            let protocol = Protocol::new_image(version, payload, 300, 300);
            protocol.serialize()
        }
    }
}

pub fn deconstruct_payload(payload: String) -> Protocol {
    Protocol::deserialize(payload)
}
