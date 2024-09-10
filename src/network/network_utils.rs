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
                        let content = payload.get_content();

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
pub struct Protocol {
    pub version: u8,
    pub message_type: u8,
    pub offset: u8,
    pub payload_len: u32,
    pub checksum: u8,
    pub payload: String,
}

impl Protocol {
    pub fn new(version: u8, message_type: MessageType, payload: String) -> Protocol {
        let message_type = message_type as u8;

        let checksum = 10;

        let padded_payload = pad_string(&payload, 16);

        let original_len = payload.len();
        let payload_len = padded_payload.len();
        let offset = (payload_len - original_len) as u8;

        Protocol {
            version,
            message_type,
            offset,
            payload_len: payload_len as u32,
            checksum: 10,
            payload: padded_payload,
        }
    }

    pub fn serialize(&self) -> String {
        let mut serialized_payload = String::new();

        let version_bytes = to_4bit_string(self.version);
        let message_type_bytes = to_4bit_string(self.message_type);
        let offset_bytes = to_4bit_string(self.offset);
        let checksum = to_8bit_string(self.checksum);

        let payload_len = to_32bit_string(self.payload_len);
        let after_checksum_zeros = "0".repeat(16); // Adjusted to 16 bits to accommodate 32-bit payload_len
        let payload = self.payload.clone();

        serialized_payload.push_str(&version_bytes);
        serialized_payload.push_str(&message_type_bytes);
        serialized_payload.push_str(&offset_bytes);
        serialized_payload.push_str(&payload_len);
        serialized_payload.push_str(&checksum);
        serialized_payload.push_str(&after_checksum_zeros);
        serialized_payload.push_str(&payload);

        return serialized_payload;
    }

    pub fn deserialize(serialized_payload: String) -> Protocol {
        let version_str = &serialized_payload[0..4];
        let version = u8::from_str_radix(version_str, 2).unwrap();

        let message_type_str = &serialized_payload[4..8];
        let message_type = u8::from_str_radix(message_type_str, 2).unwrap();

        let offset_str = &serialized_payload[8..12];
        let offset = u8::from_str_radix(offset_str, 2).unwrap();

        let payload_len_str = &serialized_payload[12..44]; // Adjusted to 32 bits
        let payload_len = u32::from_str_radix(payload_len_str, 2).unwrap();

        let checksum_str = &serialized_payload[44..52]; // Adjusted to start after 32-bit payload_len
        let checksum = u8::from_str_radix(checksum_str, 2).unwrap();

        let _ = &serialized_payload[52..68]; // Adjusted to start after checksum

        let payload = serialized_payload[68..].to_string(); // Adjusted to start after the new bit offsets

        Protocol {
            version,
            message_type,
            offset,
            checksum,
            payload_len,
            payload,
        }
    }

    pub fn get_content(&self) -> String {
        // print all fields from payload
        let len_without_offset = self.payload_len - self.offset as u32;
        self.payload.clone()[0..len_without_offset as usize].to_string()
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

pub fn construct_payload(version: u8, message_type: MessageType, payload: String) -> String {
    let protocol = Protocol::new(version, message_type, payload);
    protocol.serialize()
}

pub fn deconstruct_payload(payload: String) -> Protocol {
    Protocol::deserialize(payload)
}
