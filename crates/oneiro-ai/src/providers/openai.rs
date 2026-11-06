//! OpenAI-compatible chat completions: OpenAI itself, Ollama (`http://localhost:11434/v1`),
//! LM Studio, llama.cpp server, vLLM, OpenRouter... anything that speaks `/chat/completions`.

use super::{api_error, http_agent, post_json};
use crate::b64;
use crate::types::*;
use serde_json::{json, Value};

pub struct OpenAiCompatible {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    /// Send images as `image_url` data URIs (only for vision-capable models).
    pub vision: bool,
}

impl OpenAiCompatible {
    pub fn new(base_url: &str, api_key: Option<String>, model: &str, vision: bool) -> Self {
        OpenAiCompatible { base_url: base_url.trim_end_matches('/').to_string(), api_key: api_key.filter(|k| !k.is_empty()), model: model.to_string(), vision }
    }

    pub fn build_body(&self, req: &ChatRequest) -> Value {
        let mut messages = vec![json!({"role": "system", "content": req.system})];
        for m in &req.messages {
            let role = if m.role == Role::User { "user" } else { "assistant" };
            if m.images.is_empty() || !self.vision {
                messages.push(json!({"role": role, "content": m.text}));
            } else {
                let mut content: Vec<Value> = vec![json!({"type": "text", "text": m.text})];
                for png in &m.images {
                    content.push(json!({"type": "image_url", "image_url": {"url": format!("data:image/png;base64,{}", b64::encode(png))}}));
                }
                messages.push(json!({"role": role, "content": content}));
            }
        }
        json!({"model": self.model, "messages": messages, "max_tokens": req.max_tokens, "stream": false})
    }

    pub fn parse_response(body: &str) -> Result<ChatResponse, String> {
        let v: Value = serde_json::from_str(body).map_err(|e| format!("invalid JSON from the server: {}", e))?;
        let text = v
            .pointer("/choices/0/message/content")
            .and_then(|t| t.as_str())
            .ok_or("the response has no choices[0].message.content")?
            .to_string();
        let usage = v.get("usage").map(|u| (u.get("prompt_tokens").and_then(|x| x.as_u64()).unwrap_or(0), u.get("completion_tokens").and_then(|x| x.as_u64()).unwrap_or(0)));
        Ok(ChatResponse { text, usage })
    }
}

impl Provider for OpenAiCompatible {
    fn name(&self) -> String {
        format!("{} ({})", self.base_url, self.model)
    }

    fn supports_images(&self) -> bool {
        self.vision
    }

    fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        if self.model.is_empty() {
            return Err("no model name configured for the OpenAI-compatible server".into());
        }
        let agent = http_agent(900);
        let url = format!("{}/chat/completions", self.base_url);
        let mut headers: Vec<(&str, String)> = vec![];
        if let Some(k) = &self.api_key {
            headers.push(("authorization", format!("Bearer {}", k)));
        }
        let (status, body) = post_json(&agent, &url, &headers, &self.build_body(req))?;
        if status != 200 {
            return Err(api_error(status, &body));
        }
        Self::parse_response(&body)
    }
}
