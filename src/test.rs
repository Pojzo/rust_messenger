use enums::message::{Message, TextMessage};
use network::network_utils::construct_payload;

mod enums;
mod network;

fn main() {
    let version: u8 = 1;
    let msg_type = Message::TextMessage(TextMessage {
        content: "content of message".to_string(),
        time_sent: "324".to_string(),
        sent: true,
    });
    let content = "This is some content";

    let payload = construct_payload(version, msg_type, content.to_string());
    let payload_bytes = payload.as_bytes();
    for i in 0..payload_bytes.len() {
        if i % 4 == 0 {
            if i % 32 == 0 && i != 0 {
                println!(); // Start a new row every 32 bytes
            }
            if i + 4 <= payload_bytes.len() {
                print!(
                    "{:02X}{:02X}{:02X}{:02X} ",
                    payload_bytes[i],
                    payload_bytes[i + 1],
                    payload_bytes[i + 2],
                    payload_bytes[i + 3]
                );
            } else {
                // Handle the case where the remaining bytes are less than 4
                for j in i..payload_bytes.len() {
                    print!("{:02X}", payload_bytes[j]);
                }
                print!(" ");
            }
        }
    }
    println!(); // Ensure the last line is printed
    println!("Payload {}", payload);
}
