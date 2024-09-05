use egui::plot::Text;
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};

use crate::{
    connection_status::ConnectionStatus,
    message::{
        construct_connection_message, construct_text_message, CombinedMessage, ConnectionMessage,
        LogMessage, Message, TextMessage,
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
                        break;
                    }
                    Ok(n) => {
                        let tx_guard = tx.lock().await;
                        let content = String::from_utf8_lossy(&buffer[..n]).to_string();
                        println!("Read {} bytes: {}", n, content);

                        if content.trim() == "<DISCONNECT>" {
                            let disconnect_message = construct_connection_message(ConnectionStatus::DISCONNECTED);
                            tx_guard.send(disconnect_message).await.unwrap();
                            println!("Received disconnect message");

                            disconnect_notify.notify_one();
                            break;
                        }

                        let message = construct_text_message(content.clone(), false);

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
                    let disconnect_message =
                    construct_connection_message(ConnectionStatus::DISCONNECTED);
                    let tx_guard = tx.lock().await;
                    tx_guard.send(disconnect_message).await.unwrap();
                    println!("Sending disconnect message to UI ");
                    break;
            }
        }
    }
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
