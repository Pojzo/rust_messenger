#[cfg(test)]
mod tests {
    use image::io::Reader as ImageReader;

    use crate::{enums::message::MessageType, network::network_utils::Protocol};

    #[test]
    fn test_serialize_deserialize() {
        let original_message = "Hello world".to_string();
        let protocol = Protocol::new(2, MessageType::TEXT, original_message.clone());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);

        assert_eq!(original_message, deserialized.get_content());
    }
    #[test]
    fn test_serialize_long_message() {
        let original_message = "Hello world".repeat(1000);
        let protocol = Protocol::new(2, MessageType::TEXT, original_message.clone());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);

        assert_eq!(original_message, deserialized.get_content());
    }
    #[test]
    fn test_serialize_really_long_message() {
        let original_message = "Hello world".repeat(10000000);
        let protocol = Protocol::new(2, MessageType::TEXT, original_message.clone());

        println!("Len of original message: {}", original_message.len());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);
        println!(
            "Len of deserialized message: {}",
            deserialized.get_content().len()
        );

        assert_eq!(original_message, deserialized.get_content());
    }

    #[test]
    fn test_serialize_deserialize_empty() {
        let original_message = "".to_string();
        let protocol = Protocol::new(2, MessageType::TEXT, original_message.clone());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);
        assert_eq!(original_message, deserialized.get_content());
    }
    #[test]
    fn test_serialize_deserialize_image() {
        let image = include_bytes!("../../data/server.png");
        let image_string = String::from_utf8_lossy(image).to_string();

        let protocol = Protocol::new(2, MessageType::IMAGE, image_string.clone());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);
        assert_eq!(image_string, deserialized.get_content());
    }
}
