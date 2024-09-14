#[cfg(test)]
mod tests {

    use crate::{
        app::app::{color_image_to_bytes, Profile},
        network::network_utils::Protocol,
    };

    #[test]
    fn test_serialize_deserialize() {
        let original_message = "Hello world".to_string();
        let protocol = Protocol::new_text(2, original_message.clone());

        println!("{:?}", protocol);

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }
    #[test]
    fn test_serialize_long_message() {
        let original_message = "Hello world".repeat(1000);
        let protocol = Protocol::new_text(2, original_message.clone());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }
    #[test]
    fn test_serialize_really_long_message() {
        let original_message = "Hello world".repeat(10000000);
        let protocol = Protocol::new_text(2, original_message.clone());

        println!("Len of original message: {}", original_message.len());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }

    #[test]
    fn test_serialize_deserialize_empty() {
        let original_message = "".to_string();
        let protocol = Protocol::new_text(2, original_message.clone());
        println!("Protocol: {:?}", protocol);

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }
    #[test]
    fn test_serialize_deserialize_image() {
        let profile = Profile::new("data/pojzo.jpg");
        let image = profile.get_image().unwrap();
        let bytes = color_image_to_bytes(&image);
        let width = image.width() as u16;
        let height = image.height() as u16;

        let string_bytes = String::from_utf8_lossy(&bytes);

        let protocol = Protocol::new_image(2, string_bytes.to_string(), width, height);

        let serialized = protocol.serialize();
        println!("Serialized len: {}", serialized.len());

        let deserialized = Protocol::deserialize(serialized);
        assert_eq!(protocol, deserialized);

        assert_ne!(deserialized.image_protocol, None);
        let deserialized_image = deserialized.image_protocol.unwrap();

        let deserialized_width = deserialized_image.width;
        let deserialized_height = deserialized_image.height;

        assert_eq!(width, deserialized_width);
        assert_eq!(height, deserialized_height);
    }
}
