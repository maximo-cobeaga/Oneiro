//! Anthropic Messages API (Claude) with an API key.

use super::{api_error, http_agent, post_json};
use crate::b64;
use crate::types::*;
use serde_json::{json, Value};

pub const DEFAULT_MODEL: &str = "claude-sonnet-5-5";
pub const DEFAULT_BASE: &str = "https://api.anthropic.com";

pub struct AnthropicApi {
    pub api_key: String,
    pub model: String,
    pub base_url: String,
}

impl AnthropicApi {
    pub fn new(api_key: String, model: Option<String>, base_url: Option<String>) -> Self {
        AnthropicApi {
            api_key,
            model: model.filter(|m| !m.is_empty()).unwrap_or_else(|| DEFAULT_MODEL.into()),
            base_url: base_url.filter(|b| !b.is_empty()).unwrap_or_else(|| DEFAULT_BASE.into()).trim_end_matches('/').to_string(),
        }
    }

    pub fn build_body(&self, req: &ChatRequest) -> Value {
        let messages: Vec<Value> = req
            .messages
            .iter()
            .map(|m| {
                let role = if m.role == Role::User { "user" } else { "assistant" };
                if m.images.is_empty() {
                    json!({"role": role, "content": m.text})
                } else {
                    let mut content: Vec<Value> = m
                        .images
                        .iter()
                        .map(|png| json!({"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64::encode(png)}}))
                        .collect();
                    content.push(json!({"type": "text", "text": m.text}));
                    json!({"role": role, "content": content})
                }
            })
            .collect();
        json!({"model": self.model, "max_tokens": req.max_tokens, "system": req.system, "messages": messages})
    }

    pub fn parse_response(body: &str) -> Result<ChatResponse, String> {
        let v: Value = serde_json::from_str(body).map_err(|e| format!("invalid JSON from the API: {}", e))?;
        let blocks = v.get("content").and_then(|c| c.as_array()).ok_or("the response has no content")?;
        let text: String = blocks.iter().filter_map(|b| b.get("text").and_then(|t| t.as_str())).collect::<Vec<_>>().join("");
        let usage = v.get("usage").map(|u| (u.get("input_tokens").and_then(|x| x.as_u64()).unwrap_or(0), u.get("output_tokens").and_then(|x| x.as_u64()).unwrap_or(0)));
        Ok(ChatResponse { text, usage })
    }
}

impl Provider for AnthropicApi {
    fn name(&self) -> String {
        format!("Anthropic API ({})", self.model)
    }

    fn supports_images(&self) -> bool {
        true
    }

    fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        if self.api_key.is_empty() {
            return Err("no API key: set the ANTHROPIC_API_KEY environment variable (or paste a key for this session)".into());
        }
        let agent = http_agent(600);
        let url = format!("{}/v1/messages", self.base_url);
        let headers = [("x-api-key", self.api_key.clone()), ("anthropic-version", "2023-06-01".to_string())];
        let (status, body) = post_json(&agent, &url, &headers, &self.build_body(req))?;
        if status != 200 {
            return Err(api_error(status, &body));
        }
        Self::parse_response(&body)
    }
}
