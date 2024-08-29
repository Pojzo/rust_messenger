use std::io::{stdin, stdout, Read, Write};
use std::net::TcpStream;

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

fn main() -> std::io::Result<()> {
    let mut stream = TcpStream::connect("127.0.0.1:8888")?;
    println!("Connected to the server!");

    let mut input_buffer = String::new();
    loop {
        handle_user_input(&mut input_buffer);
        let input_bytes = input_buffer.as_bytes();

        if let Err(e) = stream.write_all(input_bytes) {
            eprintln!("Could not send message: {}", e);
            break;
        }

        stream.flush()?;
        println!("Sent message: {}", input_buffer);
        input_buffer.clear();
    }

    // let mut buffer = [0; 512];
    // let n = stream.read(&mut buffer)?;
    // println!(
    //     "Received from server: {}",
    //     String::from_utf8_lossy(&buffer[..n])
    // );

    Ok(())
}
