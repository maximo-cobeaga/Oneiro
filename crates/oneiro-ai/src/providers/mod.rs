pub mod anthropic;
pub mod cli;
pub mod mock;
pub mod openai;

use std::time::Duration;

/// Shared blocking HTTP agent (no automatic error on 4xx/5xx so we can show the server's message).
pub(crate) fn http_agent(timeout_secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout_secs)))
        .http_status_as_error(false)
        .build()
        .into()
}

/// Sends a JSON POST and returns (status, body).
pub(crate) fn post_json(agent: &ureq::Agent, url: &str, headers: &[(&str, String)], body: &serde_json::Value) -> Result<(u16, String), String> {
    let mut req = agent.post(url).header("content-type", "application/json");
    for (k, v) in headers {
        req = req.header(*k, v.as_str());
    }
    let mut resp = req.send(body.to_string()).map_err(|e| format!("could not reach {}: {}", url, e))?;
    let status = resp.status().as_u16();
    let text = resp.body_mut().read_to_string().map_err(|e| format!("could not read the response from {}: {}", url, e))?;
    Ok((status, text))
}

/// Pulls a human-readable error out of an API error body.
pub(crate) fn api_error(status: u16, body: &str) -> String {
    let detail = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| {
            v.pointer("/error/message")
                .or_else(|| v.pointer("/error"))
                .or_else(|| v.pointer("/message"))
                .and_then(|m| m.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| body.chars().take(300).collect());
    format!("HTTP {}: {}", status, detail)
}
