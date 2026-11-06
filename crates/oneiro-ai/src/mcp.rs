//! Model Context Protocol (JSON-RPC 2.0) request handling for the Oneiro tools. The stdio loop
//! lives in the `oneiro-mcp` binary; keeping the handler here makes it testable without processes.

use crate::b64;
use crate::guide::{self, Mode};
use crate::tools::Toolbox;
use serde_json::{json, Value};

pub const SERVER_INSTRUCTIONS: &str = "Oneiro is a language and studio for animated series. Call get_guide first: it explains the language, the exact vocabulary and this project's characters, sets and anchors. Then write the COMPLETE script, validate it with check_script, LOOK at it with contact_sheet / render_still, fix problems, and save with write_script. Create missing characters with write_character.";

fn error(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

/// Handles one JSON-RPC message. Returns the response, or `None` for notifications.
pub fn handle(tb: &Toolbox, msg: &Value) -> Option<Value> {
    let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
    let id = msg.get("id").cloned();
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    // Notifications (no id) never get a response.
    let Some(id) = id else { return None };
    Some(match method {
        "initialize" => {
            let version = params.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or("2024-11-05");
            ok(
                &id,
                json!({
                    "protocolVersion": version,
                    "capabilities": {"tools": {"listChanged": false}, "resources": {"listChanged": false, "subscribe": false}},
                    "serverInfo": {"name": "oneiro", "version": env!("CARGO_PKG_VERSION")},
                    "instructions": SERVER_INSTRUCTIONS,
                }),
            )
        }
        "ping" => ok(&id, json!({})),
        "tools/list" => {
            let tools: Vec<Value> = Toolbox::definitions().into_iter().map(|t| json!({"name": t.name, "description": t.description, "inputSchema": t.schema})).collect();
            ok(&id, json!({"tools": tools}))
        }
        "tools/call" => {
            let name = params.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let args = params.get("arguments").cloned().unwrap_or_else(|| json!({}));
            if !Toolbox::definitions().iter().any(|t| t.name == name) {
                return Some(error(&id, -32602, &format!("unknown tool '{}'", name)));
            }
            let out = tb.call(name, &args);
            let mut content = vec![json!({"type": "text", "text": out.text})];
            for png in &out.images {
                content.push(json!({"type": "image", "data": b64::encode(png), "mimeType": "image/png"}));
            }
            ok(&id, json!({"content": content, "isError": out.is_error}))
        }
        "resources/list" => ok(
            &id,
            json!({"resources": [
                {"uri": "oneiro://guide", "name": "Oneiro language guide and project library", "mimeType": "text/markdown"},
                {"uri": "oneiro://script", "name": "Current script", "mimeType": "text/plain"},
            ]}),
        ),
        "resources/read" => {
            let uri = params.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            let text = match uri {
                "oneiro://guide" => Some(tb.call("get_guide", &json!({})).text),
                "oneiro://script" => Some(tb.call("read_script", &json!({})).text),
                _ => None,
            };
            match text {
                Some(t) => ok(&id, json!({"contents": [{"uri": uri, "mimeType": "text/plain", "text": t}]})),
                None => error(&id, -32002, &format!("unknown resource '{}'", uri)),
            }
        }
        "prompts/list" => ok(&id, json!({"prompts": []})),
        _ => error(&id, -32601, &format!("method not found: {}", method)),
    })
}

#[allow(dead_code)]
fn _keep(_: Mode) {
    let _ = guide::LANGUAGE_GUIDE;
}
