use std::sync::Arc;
use tokio::{
    io::{self, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    net::{
        tcp::{OwnedReadHalf, OwnedWriteHalf},
        TcpListener, TcpStream,
    },
    sync::Mutex,
    task,
};

use crate::common::get_user_input;

async fn handle_client_read(mut read_stream: OwnedReadHalf) -> String {
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
                String::from_utf8_lossy(&buffer[..n])
                // println!(
                //     "Received message: {}",
                //     String::from_utf8_lossy(&buffer[..n])
                // );
            }
            Err(e) => {
                eprintln!("Failed to read from connection: {}", e);
                break;
            }
        }
    }
}

async fn handle_client_write(mut write_stream: OwnedWriteHalf) {
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

async fn handle_client(stream: TcpStream) {
    let (read_stream, write_stream) = stream.into_split();

    let read_future = task::spawn(async move { handle_client_read(read_stream).await });

    let write_future = task::spawn(async move { handle_client_write(write_stream).await });

    let _ = tokio::join!(read_future, write_future);
}

// #[tokio::main]
// async fn main() -> std::io::Result<()> {
//     let listener = TcpListener::bind("0.0.0.0:8888").await?;
//     println!("Server listening on 0.0.0.0:8888");

//     while let Ok((stream, _)) = listener.accept().await {
//         tokio::spawn(handle_client(stream));
//     }
//     Ok(())
// }
