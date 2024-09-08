use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};
use u4::U4;

use crate::enums::{
    connection_status::ConnectionStatus,
    message::{
        construct_connection_message, construct_text_message_generic, CombinedMessage, Message,
    },
};

pub async fn handle_connection(
    stream: tokio::net::TcpStream,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    rx: Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    let (read_stream, write_stream) = stream.into_split();
    println!("Connection established");
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

pub async fn stream_read(
    mut read_stream: OwnedReadHalf,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
    disconnect_notify: Arc<Notify>,
) {
    let mut buffer = [0; 512];
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
                        let content = String::from_utf8_lossy(&buffer[..n]).to_string();
                        println!("Read {} bytes: {}", n, content);

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

                match write_stream.write_all(content.as_bytes()).await {
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

fn to_4bit_string(value: u8) -> String {
    let four_bit_value = value & 0x0F;

    format!("{:04b}", four_bit_value)
}

fn to_20bit_string(value: u32) -> String {
    let twenty_bit_value = value & 0x000FFFFF;

    format!("{:020b}", twenty_bit_value)
}

fn to_8bit_string(value: u8) -> String{
    format!("{:08b}", value)
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
#[derive(Clone, PartialEq, Eq)]
pub struct Protocol {
    pub version: u8,
    pub message_type: u8,
    pub offset: u8,
    pub payload_len: u32,
    pub checksum: u8,
    pub payload: String,
}

impl Protocol {
    pub fn serialize(&self) -> String {
        let mut serialized_payload = String::new();

        let version_bytes = to_4bit_string(self.version);
        let message_type_bytes = to_4bit_string(self.message_type);
        let offset_bytes = to_4bit_string(self.offset);
        let checksum = to_8bit_string(self.checksum);

        let payload_len = to_20bit_string(self.payload_len);
        let payload = self.payload.clone();

        serialized_payload.push_str(&version_bytes);
        serialized_payload.push_str(&message_type_bytes);
        serialized_payload.push_str(&offset_bytes);
        serialized_payload.push_str(&payload_len);
        serialized_payload.push_str(&checksum);
        serialized_payload.push_str(&payload);
        
        return serialized_payload;
    }
}

pub fn construct_payload(version: u8, message_type: Message, payload: String) -> String {
    let message_type = match message_type {
        Message::TextMessage(_) => 0 as u8,
    };

    let checksum = 10;

    let padded_payload = pad_string(&payload, 16);

    let original_len = payload.len();
    let payload_len = padded_payload.len();
    let offset = (payload_len - original_len) as u8;

    let protocol = Protocol {
        version,
        message_type,
        offset,
        payload_len: payload_len as u32,
        checksum: 10,
        payload: padded_payload
    }

    payload
}
