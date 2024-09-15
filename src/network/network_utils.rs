use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};

use crate::{
    app::app::{color_image_to_bytes, Profile},
    enums::{
        connection_status::ConnectionStatus,
        message::{
            construct_connection_message, construct_image_message_generic,
            construct_text_message_generic, CombinedMessage, Message, MessageType,
        },
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
    let size_reduction = 0.2;
    let profile = Profile::new("data/pojzo.jpg", size_reduction);
    let image = profile.get_image().unwrap();
    let bytes = color_image_to_bytes(&image);
    let width = image.width() as u16;
    let height = image.height() as u16;
    let protocol = Protocol::new_image(2, bytes, width, height);
    let serialized = protocol.serialize();

    let payload_size = serialized.len() as u32;
    let payload_size_bytes = u32::to_be_bytes(payload_size);

    match write_stream.write_all(&payload_size_bytes).await {
        Ok(_) => {
            println!("Sent payload size: {}", payload_size_bytes.len());
        }
        Err(e) => {
            eprintln!("Failed to write to connection: {}", e);
        }
    }

    match write_stream.write_all(&serialized).await {
        Ok(_) => {
            println!("Sent profile picture");
        }
        Err(e) => {
            eprintln!("Failed to write to connection: {}", e);
        }
    }
}

pub async fn receive_profile_picture(
    tx: &Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    payload: Protocol,
) {
    let image_protocol = payload.image_protocol.unwrap();
    let content = image_protocol.content;
    let width = image_protocol.width;
    let height = image_protocol.height;

    println!(
        "Received profile picture, width: {}, height: {}",
        width, height
    );

    let image_message = construct_image_message_generic(content.to_vec(), width, height);

    let tx_guard = tx.lock().await;
    tx_guard.send(image_message).await.unwrap();
    println!(
        "Received profile picture, sent to UI, width: {}, height: {}",
        width, height
    );
}

pub async fn stream_read(
    mut read_stream: OwnedReadHalf,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    let mut buffer = Vec::with_capacity(50_000_000);
    buffer.resize(50_000_000, 0);
    let mut payload_len_buffer = [0u8; 4];
    {
        let connect_message = construct_connection_message(ConnectionStatus::CONNECTED);
        let tx_guard = tx.lock().await;
        tx_guard.send(connect_message).await.unwrap();
    }
    loop {
        tokio::select! {
            result = read_stream.read_exact(&mut payload_len_buffer) => {
                match result {
                    Ok(0) => {
                        println!("Connection closed by the client.");
                        handle_disconnect(&tx, &disconnect_notify).await;
                        break;
                    }
                    Ok(n) => {
                        println!("Read {} bytes", n);
                        let payload_len = u32::from_be_bytes(payload_len_buffer);

                        let mut payload_buffer = vec![0u8; payload_len as usize];

                        // Read the exact number of bytes specified by payload_len
                        read_stream.read_exact(&mut payload_buffer).await.unwrap();

                        let payload = deconstruct_payload(&payload_buffer);
                        if payload.image_protocol.is_some() {
                            receive_profile_picture(&tx, payload).await;
                            continue;
                        }

                        let tx_guard = tx.lock().await;
                        let content = payload.text_protocol.unwrap().content;

                        if content.trim() == "<DISCONNECT>" {
                            println!("Received disconnect message");
                            handle_disconnect(&tx, &disconnect_notify).await;
                            break;
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
    // send_profile_picture(&mut write_stream).await; // Pass mutable reference
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
                let content = match generic_message {CombinedMessage::Message(Message::TextMessage(message))=>message.content,
                    CombinedMessage::LogMessage(_) => {
                        continue;
                    }
                    CombinedMessage::Message(Message::ImageMessage(_)) => {
                        continue;
                    }
                };

                    if content == "send" {
                        send_profile_picture(&mut write_stream).await;
                        continue;
                    }

                let payload = construct_payload(1, MessageType::TEXT, content.as_bytes().to_vec());
                let payload_size = payload.len();
                let payload_size_bytes = u32::to_be_bytes(payload_size as u32);
                // first send the size of the payload

                match write_stream.write_all(&payload_size_bytes).await {
                    Ok(_) => {
                        println!("Sent payload size: {}", payload_size_bytes.len());
                    }
                    Err(e) => {
                        eprintln!("Failed to write to connection: {}", e);
                        break;
                    }
                }

                match write_stream.write_all(&payload).await {
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

fn pad_bytes(bytes: Vec<u8>, chunk_size: usize) -> Vec<u8> {
    let mut new_bytes = bytes.to_vec();

    let add_size = chunk_size - (bytes.len() % chunk_size);

    if add_size != chunk_size {
        for _ in 0..add_size {
            new_bytes.push(0);
        }
    }

    return new_bytes;
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
    pub content: Vec<u8>,
    pub width: u16,
    pub height: u16,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Protocol {
    pub version: u8,
    pub message_type: u8,
    pub payload: Vec<u8>,

    pub text_protocol: Option<TextProtocol>,
    pub image_protocol: Option<ImageProtocol>,
}

impl Protocol {
    pub fn new_text(version: u8, payload: Vec<u8>) -> Protocol {
        let message_type = MessageType::TEXT as u8;

        let text_protocol = Some(TextProtocol {
            content: String::from_utf8_lossy(&payload).to_string(),
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

    pub fn new_image(version: u8, payload: Vec<u8>, width: u16, height: u16) -> Protocol {
        let message_type = MessageType::IMAGE as u8;

        let text_protocol = None;
        let image_protocol = Some(ImageProtocol {
            content: payload.to_vec(),
            width,
            height,
        });

        Protocol {
            version,
            message_type,
            payload: payload.to_vec(),
            text_protocol,
            image_protocol,
        }
    }

    pub fn serialize(&self) -> Vec<u8> {
        match self.message_type {
            0 => self.serialize_text(),
            1 => self.serialize_image(),
            _ => panic!("Unknown message type"),
        }
    }

    pub fn serialize_text(&self) -> Vec<u8> {
        let mut serialized_payload = Vec::new();

        let version_bytes = self.version.to_be_bytes();
        let message_type_bytes = self.message_type.to_be_bytes();

        let padded_payload = pad_bytes(self.payload.clone(), 16);
        let padded_payload_len = padded_payload.len() as u32;

        let offset = (padded_payload.len() - self.payload.len()) as u8;
        let offset_bytes = offset.to_be_bytes();

        let payload_len_bytes = padded_payload_len.to_be_bytes();

        let eof = vec![0u8; 8]; // 8 bytes of zero

        serialized_payload.extend_from_slice(&version_bytes);
        serialized_payload.extend_from_slice(&message_type_bytes);
        serialized_payload.extend_from_slice(&offset_bytes);
        serialized_payload.extend_from_slice(&payload_len_bytes);
        serialized_payload.extend_from_slice(&padded_payload);
        serialized_payload.extend_from_slice(&eof);

        serialized_payload
    }

    pub fn serialize_image(&self) -> Vec<u8> {
        let mut serialized_payload = Vec::new();

        let version_bytes = u8::to_be_bytes(self.version);
        let message_type_bytes = u8::to_be_bytes(self.message_type);

        let offset = self.payload.len() - self.image_protocol.as_ref().unwrap().content.len();
        let offset_bytes = u8::to_be_bytes(offset as u8);

        let padded_payload = pad_bytes(self.payload.clone(), 16);

        // let payload_len_bytes = to_32bit_bytes(self.payload.len() as u32);
        let payload_len_bytes = u32::to_be_bytes(padded_payload.len() as u32);

        let width = self.image_protocol.as_ref().unwrap().width;
        let height = self.image_protocol.as_ref().unwrap().height;

        let width_bytes = u16::to_be_bytes(width);
        let height_bytes = u16::to_be_bytes(height);

        let eof = vec![0u8; 8]; // 8 bytes of zero

        serialized_payload.extend_from_slice(&version_bytes);
        serialized_payload.extend_from_slice(&message_type_bytes);
        serialized_payload.extend_from_slice(&offset_bytes);
        serialized_payload.extend_from_slice(&payload_len_bytes);
        serialized_payload.extend_from_slice(&width_bytes);
        serialized_payload.extend_from_slice(&height_bytes);
        serialized_payload.extend_from_slice(&padded_payload);
        serialized_payload.extend_from_slice(&eof);

        serialized_payload
    }

    pub fn deserialize(serialized_payload: &[u8]) -> Protocol {
        let message_type = &serialized_payload[1];

        match message_type {
            0 => Protocol::deserialize_text(serialized_payload),
            1 => Protocol::deserialize_image(serialized_payload),
            _ => panic!("Unknown message type: {}", message_type),
        }
    }

    pub fn deserialize_text(serialized_payload: &[u8]) -> Protocol {
        // Extract version, message_type, and offset from the first 12 bits
        println!("Deserializing");
        let version = serialized_payload[0];
        let message_type = serialized_payload[1];
        let offset = serialized_payload[2];

        println!("Version: {}", version);
        println!("Message type: {}", message_type);
        println!("Offset: {}", offset);

        // Extract payload length from the next 32 bits
        let payload_len = u32::from_be_bytes([
            serialized_payload[3],
            serialized_payload[4],
            serialized_payload[5],
            serialized_payload[6],
        ]);
        println!("Payload len: {}", payload_len);

        let payload_start = 7;
        let payload_end = payload_start + payload_len as usize;

        // Extract the full payload and remove the offset
        let full_payload = &serialized_payload[payload_start..payload_end];
        let payload = &full_payload[..(full_payload.len() - offset as usize)];

        // Convert the payload to a string for the text protocol
        let text_protocol = Some(TextProtocol {
            content: String::from_utf8_lossy(payload).to_string(),
        });
        let image_protocol = None;

        Protocol {
            version,
            message_type,
            payload: payload.to_vec(),
            text_protocol,
            image_protocol,
        }
    }

    pub fn deserialize_image(serialized_payload: &[u8]) -> Protocol {
        let version = serialized_payload[0];
        let message_type = serialized_payload[1];
        let offset = serialized_payload[2];

        let payload_len = u32::from_be_bytes([
            serialized_payload[3],
            serialized_payload[4],
            serialized_payload[5],
            serialized_payload[6],
        ]);

        let width = u16::from_be_bytes([serialized_payload[7], serialized_payload[8]]);
        let height = u16::from_be_bytes([serialized_payload[9], serialized_payload[10]]);

        let payload_start = 11;
        let payload_end = payload_start + payload_len as usize;

        let full_payload = &serialized_payload[payload_start..payload_end];
        let payload = &full_payload[..(full_payload.len() - offset as usize)];

        let image_protocol = Some(ImageProtocol {
            content: payload.to_vec(),
            width,
            height,
        });

        Protocol {
            version,
            message_type,
            payload: payload.to_vec(),
            text_protocol: None,
            image_protocol,
        }
    }
}

// Helper functions to convert integers to byte vectors
fn to_4bit_bytes(value: u8) -> Vec<u8> {
    vec![(value & 0x0F) << 4]
}

fn to_8bit_bytes(value: u8) -> Vec<u8> {
    vec![value]
}

fn to_16bit_bytes(value: u16) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

fn to_32bit_bytes(value: u32) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

pub fn construct_payload(version: u8, message_type: MessageType, payload: Vec<u8>) -> Vec<u8> {
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

pub fn deconstruct_payload(payload: &[u8]) -> Protocol {
    Protocol::deserialize(payload)
}
