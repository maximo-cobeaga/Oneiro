//! A scripted provider for tests and offline demos.

use crate::types::*;
use std::sync::Mutex;

pub struct MockProvider {
    replies: Mutex<Vec<String>>,
    pub requests: Mutex<Vec<ChatRequest>>,
    pub images: bool,
}

impl MockProvider {
    /// Replies are returned in order; the last one repeats when the list runs out.
    pub fn new(replies: Vec<String>) -> Self {
        MockProvider { replies: Mutex::new(replies), requests: Mutex::new(vec![]), images: false }
    }

    pub fn with_images(mut self) -> Self {
        self.images = true;
        self
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }
}

impl Provider for MockProvider {
    fn name(&self) -> String {
        "mock".into()
    }

    fn supports_images(&self) -> bool {
        self.images
    }

    fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        self.requests.lock().unwrap().push(req.clone());
        let mut r = self.replies.lock().unwrap();
        let text = if r.len() > 1 { r.remove(0) } else { r.first().cloned().unwrap_or_default() };
        Ok(ChatResponse { text, usage: Some((10, 10)) })
    }
}
