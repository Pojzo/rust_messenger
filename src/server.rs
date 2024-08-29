use std::{
    io::{Read, Write},
    net::{TcpListener, TcpStream},
};

fn handle_client(mut stream: TcpStream) {
    let mut buffer = [0; 512];
    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("Connection closed by the client.");
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

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("127.0.0.1:8888")?;

    for stream in listener.incoming() {
        handle_client(stream?);
    }

    Ok(())
}
