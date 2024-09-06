use std::sync::Arc;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};

use crate::{
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
    let twenty_bit_value = value & 0x0FFFFF;

    format!("{:020b}", twenty_bit_value)
}

fn pad_payload(content: String, chunk_size: usize) -> String {
    let msg_len = content.len();
    let padded_message_len = msg_len + chunk_size - msg_len % chunk_size;
    let offset = padded_message_len - msg_len;

    let offset_bits = to_4bit_string(offset as u8);
    let payload_message_len = to_20bit_string(padded_message_len as u32);

    let mut padded_content = content.clone();
    for _ in 0..offset {
        padded_content.push('0');
    }

    format!("{}{}{}", payload_message_len, offset_bits, padded_content)
}

// version: u4
pub fn construct_payload(version: u8, msg_type: Message, content: String) -> String {
    let version_bits = to_4bit_string(version);
    let msg_type_bits = match msg_type {
        Message::TextMessage(_) => "0000".to_string(),
    };
    let msg_len = content.len();
    let chunk_size = 16;
    // offset is the value after padding the message length to 16 bits
    let padded_message_len = msg_len + 16 - msg_len % chunk_size;
    let offset = padded_message_len - msg_len;

    let offset_bits = to_4bit_string(offset as u8);
    let padded_message = pad_payload(content, chunk_size);
    let payload_message_len_with_offset = to_20bit_string(padded_message_len as u32);

    let final_message = format!(
        "{}{}{}{}{}",
        version_bits, msg_type_bits, payload_message_len_with_offset, offset_bits, padded_message
    );

    final_message
}
