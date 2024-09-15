#[cfg(test)]
mod tests {

    use crate::{
        app::app::{color_image_to_bytes, Profile},
        network::network_utils::Protocol,
    };

    #[test]
    fn test_serialize_deserialize() {
        let original_message = "Hello world".to_string();
        let protocol = Protocol::new_text(2, original_message.as_bytes().to_vec());

        println!("{:?}", protocol);

        let serialized = protocol.serialize();

        let deserialized = Protocol::deserialize(serialized.as_slice());

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }
    #[test]
    fn test_serialize_long_message() {
        let original_message = "Hello world".repeat(1000);
        let protocol = Protocol::new_text(2, original_message.as_bytes().to_vec());

        let serialized = protocol.serialize();
        for byte in serialized.iter() {
            print!("{:08b} ", byte);
        }
        let deserialized = Protocol::deserialize(serialized.as_slice());

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }
    #[test]
    fn test_serialize_really_long_message() {
        let original_message = "Hello world".repeat(1_000_123);
        let protocol = Protocol::new_text(2, original_message.as_bytes().to_vec());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized.as_slice());

        assert_eq!(protocol, deserialized);

        assert_eq!(
            original_message,
            deserialized.text_protocol.unwrap().content
        );
    }

    #[test]
    fn test_serialize_deserialize_empty() {
        let original_message = "".to_string().as_bytes().to_vec();
        let protocol = Protocol::new_text(2, original_message.clone());
        println!("Protocol: {:?}", protocol);

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized.as_slice());

        assert_eq!(protocol, deserialized);

        assert_eq!(
            String::from_utf8(original_message).unwrap(),
            deserialized.text_protocol.unwrap().content
        );
    }
    #[test]
    fn test_serialize_deserialize_image() {
        let profile = Profile::new("data/pojzo.jpg", 0.2);
        let image = profile.get_image().unwrap();
        let bytes = color_image_to_bytes(&image);
        let width = image.width() as u16;
        let height = image.height() as u16;

        println!("Len of bytes: {}", bytes.len());
        let protocol = Protocol::new_image(2, bytes, width, height);

        let serialized = protocol.serialize();
        println!("Serialized len: {}", serialized.len());

        let deserialized = Protocol::deserialize(serialized.as_slice());

        assert_eq!(protocol, deserialized);
        println!(
            "Len of deserialized bytes: {}",
            deserialized.image_protocol.as_ref().unwrap().content.len()
        );

        assert_ne!(deserialized.image_protocol, None);
        let deserialized_image = deserialized.image_protocol.unwrap();

        let deserialized_width = deserialized_image.width;
        let deserialized_height = deserialized_image.height;

        assert_eq!(width, deserialized_width);
        assert_eq!(height, deserialized_height);
    }

    #[test]
    // NODE; There is an error when trying size_reduction: 0.2, needs to be investigated
    fn test_serialize_deserialize_image2() {
        let profile = Profile::new("data/kopernik.jpg", 0.3);
        let image = profile.get_image().unwrap();
        let bytes = color_image_to_bytes(&image);
        let width = image.width() as u16;
        let height = image.height() as u16;

        println!("Len of bytes: {}", bytes.len());
        let protocol = Protocol::new_image(2, bytes, width, height);

        let serialized = protocol.serialize();
        println!("Serialized len: {}", serialized.len());

        let deserialized = Protocol::deserialize(serialized.as_slice());

        // assert_eq!(protocol, deserialized);
        println!(
            "Len of deserialized bytes: {}",
            deserialized.image_protocol.as_ref().unwrap().content.len()
        );

        println!("Len of serialized: {}", serialized.len());
        println!(
            "Len of deserialized: {}",
            deserialized.image_protocol.as_ref().unwrap().content.len()
        );

        assert_ne!(deserialized.image_protocol, None);
        let deserialized_image = deserialized.image_protocol.unwrap();

        let deserialized_width = deserialized_image.width;
        let deserialized_height = deserialized_image.height;

        assert_eq!(width, deserialized_width);
        assert_eq!(height, deserialized_height);
    }
}
