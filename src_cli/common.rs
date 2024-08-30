use tokio::io::{self, AsyncBufReadExt, BufReader};

pub(crate) async fn get_user_input() -> io::Result<String> {
    let mut buffer = String::new();
    let stdin = io::stdin();
    let mut reader = io::BufReader::new(stdin);

    reader.read_line(&mut buffer).await?;

    // Remove any newline characters
    buffer = buffer.trim_end().to_string();

    Ok(buffer)
}
