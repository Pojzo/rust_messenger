## Image Protocol

| Field         | Size      | Description                                |
|---------------|-----------|--------------------------------------------|
| **version**   | 1 byte    | Protocol version                           |
| **msg_type**  | 1 byte    | Type of message (e.g., header, data, ack)   |
| **offset_bytes** | 1 byte | Offset in bytes for the payload            |
| **payload_len** | 4 bytes | Length of the payload in bytes             |
| **width**     | 2 bytes   | Width of the image in pixels               |
| **height**    | 2 bytes   | Height of the image in pixels              |
| **payload**   | *n* bytes | The actual data being sent (image chunk)   

## Text Protocol

| Field          | Size       | Description                                |
|----------------|------------|--------------------------------------------|
| **version**    | 1 byte     | Protocol version                           |
| **msg_type**   | 1 byte     | Type of message (e.g., start, data, end)    |
| **offset_bytes** | 1 byte   | Offset in bytes for payload data (for chunking) |
| **checksum**   | 2 bytes    | Checksum for error detection (optional)     |
| **payload_len** | 4 bytes   | Length of the payload in bytes              |
| **payload**    | *n* bytes  | The actual text data (padded to 16-byte multiples) |