#[cfg(test)]
mod tests {
    use crate::{enums::message::MessageType, network::network_utils::Protocol};

    use super::*;

    #[test]
    fn test_serialize_deserialize() {
        let protocol = Protocol::new(2, MessageType::TEXT, String::from("Hello world"));

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);
    }
    #[test]
    fn test_serialize_long_message() {
        let protocol = Protocol::new(2, MessageType::TEXT, "Hello world".repeat(1000));

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);
    }

    #[test]
    fn test_serialize_deserialize_empty() {
        let protocol = Protocol::new(2, MessageType::TEXT, String::new());

        let serialized = protocol.serialize();
        let deserialized = Protocol::deserialize(serialized);

        assert_eq!(protocol, deserialized);
    }
}
