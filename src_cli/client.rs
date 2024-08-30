use std::io::{stdin, stdout, Read, Write};

use common::get_user_input;
use tokio::io::AsyncReadExt;
use tokio::io::AsyncWriteExt;
use tokio::net::tcp::OwnedReadHalf;
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::TcpStream;
use tokio::task;

mod common;

fn handle_user_input(buffer: &mut String) {
    stdout().flush().unwrap();
    stdin()
        .read_line(buffer)
        .expect("Did not enter a correct string");

    if let Some('\n') = buffer.chars().next_back() {
        buffer.pop();
    }
    if let Some('\r') = buffer.chars().next_back() {
        buffer.pop();
    }
}

async fn handle_read(mut read_stream: OwnedReadHalf) {
    println!("I got to this function");
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
                println!("Connection closed by the server.");
                break;
            }
            Ok(n) => {
                println!(
                    "Received message: {}",
                    String::from_utf8_lossy(&buffer[..n])
                );
            }
            Err(e) => {
                eprintln!("Failed to read from connection: {}", e);
                break;
            }
        }
    }
}

async fn handle_write(mut write_stream: OwnedWriteHalf) {
    loop {
        match get_user_input().await {
            Ok(message) => match write_stream.write_all(message.as_bytes()).await {
                Ok(_) => {}
                Err(e) => {
                    eprintln!("{}", e);
                }
            },
            Err(e) => {
                eprintln!("Failed to get user input: {}", e);
                break;
            }
        }
    }
}

#[tokio::main]
async fn main() -> std::io::Result<()> {
    let stream = TcpStream::connect("0.0.0.0:8888").await?;
    println!("Connected to server");

    let (read_stream, write_stream) = stream.into_split();

    let read_future = task::spawn(async move {
        handle_read(read_stream).await;
    });

    let write_future = task::spawn(async move {
        handle_write(write_stream).await;
    });

    let _ = tokio::join!(read_future, write_future);

    Ok(())
}
