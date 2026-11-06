//! Provider-independent chat types.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
}

#[derive(Clone, Debug)]
pub struct Message {
    pub role: Role,
    pub text: String,
    /// PNG images attached to the message (only used by providers that support vision).
    pub images: Vec<Vec<u8>>,
}

impl Message {
    pub fn user(text: impl Into<String>) -> Message {
        Message { role: Role::User, text: text.into(), images: vec![] }
    }
    pub fn assistant(text: impl Into<String>) -> Message {
        Message { role: Role::Assistant, text: text.into(), images: vec![] }
    }
    pub fn with_images(mut self, images: Vec<Vec<u8>>) -> Message {
        self.images = images;
        self
    }
}

#[derive(Clone, Debug)]
pub struct ChatRequest {
    pub system: String,
    pub messages: Vec<Message>,
    pub max_tokens: u32,
}

#[derive(Clone, Debug, Default)]
pub struct ChatResponse {
    pub text: String,
    /// (input tokens, output tokens) when the provider reports them.
    pub usage: Option<(u64, u64)>,
}

/// Anything that can answer a chat request: a hosted API, a local model server or an agent CLI
/// running under the user's own login.
pub trait Provider: Send + Sync {
    fn name(&self) -> String;
    fn supports_images(&self) -> bool {
        false
    }
    fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, String>;
}
