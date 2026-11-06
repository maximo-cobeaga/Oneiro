//! `oneiro-mcp [PROJECT]` - MCP server over stdio. Register it with an MCP client, e.g.
//! `claude mcp add oneiro -- oneiro-mcp path/to/project`.

use oneiro_ai::mcp::handle;
use oneiro_ai::tools::Toolbox;
use std::io::{BufRead, Write};

fn main() {
    let path = std::env::args().nth(1).unwrap_or_else(|| ".".into());
    let tb = match Toolbox::open(std::path::Path::new(&path)) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("oneiro-mcp: {}", e);
            std::process::exit(2);
        }
    };
    eprintln!("oneiro-mcp: serving project {}", tb.root.display());
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let response = match serde_json::from_str::<serde_json::Value>(&line) {
            Ok(msg) => handle(&tb, &msg),
            Err(e) => Some(serde_json::json!({"jsonrpc": "2.0", "id": null, "error": {"code": -32700, "message": format!("parse error: {}", e)}})),
        };
        if let Some(r) = response {
            let _ = writeln!(stdout, "{}", r);
            let _ = stdout.flush();
        }
    }
}
