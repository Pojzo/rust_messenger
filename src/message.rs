#[derive(Clone)]
pub(crate) struct Message {
    pub content: String,
    pub sent: bool,
}

impl Message {
    pub fn new(content: String, sent: bool) -> Self {
        Message { content, sent }
    }
}
