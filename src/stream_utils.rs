use egui::plot::Text;
use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};

use crate::{
    connection_status::ConnectionStatus,
    message::{CombinedMessage, ConnectionMessage, LogMessage, Message, TextMessage},
};

pub async fn stream_read(
    mut read_stream: OwnedReadHalf,
    tx: Arc<AsyncMutex<mpsc::Sender<CombinedMessage>>>,
) {
    let mut buffer = [0; 512];

    match read_stream.peer_addr() {
        Ok(_) => {
            let tx_guard = tx.lock().await;
            tx_guard
                .send(CombinedMessage::LogMessage(LogMessage::ConnectionMessage(
                    ConnectionMessage {
                        connection_status: ConnectionStatus::CONNECTED,
                    },
                )))
                .await
                .unwrap();
        }
        Err(e) => {
            eprintln!("Failed to get peer address {}", e);
        }
    }
    loop {
        match read_stream.read(&mut buffer).await {
            Ok(0) => {
                println!("Connection closed by the client.");
                break;
            }
            Ok(n) => {
                println!("Read {} bytes", n);
                let tx_guard = tx.lock().await;
                let content = String::from_utf8_lossy(&buffer[..n]).to_string();
                let message = CombinedMessage::Message(Message::TextMessage(TextMessage {
                    content,
                    sent: false,
                }));

                match tx_guard.send(message).await {
                    Ok(_) => {}
                    Err(e) => {
                        println!("Couldnt receive message: {}", e);
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read from connection: {}", e);
                break;
            }
        }
    }
}

pub async fn stream_write(
    mut write_stream: OwnedWriteHalf,
    rx: Arc<AsyncMutex<mpsc::Receiver<CombinedMessage>>>,
) {
    loop {
        let mut message_guard = rx.lock().await;

        let generic_message = match message_guard.recv().await {
            Some(message) => message,
            None => todo!(),
        };

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
