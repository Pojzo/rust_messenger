use std::sync::{Arc, Mutex};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::tcp::{OwnedReadHalf, OwnedWriteHalf},
    sync::{mpsc, Mutex as AsyncMutex, Notify},
};

use crate::common::get_user_input;

pub async fn stream_read(
    mut read_stream: OwnedReadHalf,
    tx: Arc<AsyncMutex<mpsc::Sender<String>>>,
) {
    let mut buffer = [0; 512];

    match read_stream.peer_addr() {
        Ok(addr) => {
            println!("Connected to: {}", addr);
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
                let tx_guard = tx.lock().await;
                let message = String::from_utf8_lossy(&buffer[..n]);
                match tx_guard.send(message.to_string()).await {
                    Ok(_) => {
                        println!("Received message: {}", message);
                    }
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

pub async fn stream_write(mut write_stream: OwnedWriteHalf) {
    loop {
        match get_user_input().await {
            Ok(message) => match write_stream.write_all(message.as_bytes()).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}", e);
                    break;
                }
            },
            Err(e) => {
                eprintln!("Failed to get user input: {}", e);
                break;
            }
        }
    }
}
