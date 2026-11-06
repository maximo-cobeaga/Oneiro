//! Talks to the real `oneiro-mcp` binary over stdio, exactly like an MCP client would.

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

fn example() -> String {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap().to_string_lossy().to_string()
}

struct Client {
    child: Child,
    stdin: Option<ChildStdin>,
    out: BufReader<ChildStdout>,
}

impl Client {
    fn start() -> Client {
        let mut child = Command::new(env!("CARGO_BIN_EXE_oneiro-mcp"))
            .arg(example())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("start oneiro-mcp");
        let stdin = child.stdin.take();
        let out = BufReader::new(child.stdout.take().unwrap());
        Client { child, stdin, out }
    }

    fn send_raw(&mut self, text: &str) {
        let s = self.stdin.as_mut().unwrap();
        writeln!(s, "{}", text).unwrap();
        s.flush().unwrap();
    }

    fn read(&mut self) -> Value {
        let mut line = String::new();
        self.out.read_line(&mut line).unwrap();
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad JSON {:?}: {}", line, e))
    }

    fn ask(&mut self, msg: Value) -> Value {
        self.send_raw(&msg.to_string());
        self.read()
    }
}

#[test]
fn the_server_speaks_json_rpc_over_stdio() {
    let mut c = Client::start();
    let init = c.ask(json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}));
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["serverInfo"]["name"], "oneiro");

    // A notification gets no answer: the next response must belong to the next request.
    c.send_raw(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"}).to_string());
    let list = c.ask(json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}));
    assert_eq!(list["id"], 2);
    assert!(list["result"]["tools"].as_array().unwrap().len() >= 10);

    let call = c.ask(json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "check_script", "arguments": {}}}));
    assert_eq!(call["id"], 3);
    assert!(call["result"]["content"][0]["text"].as_str().unwrap().contains("\"valid\": true"));

    let img = c.ask(json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "contact_sheet", "arguments": {"max_shots": 3}}}));
    assert_eq!(img["result"]["content"][1]["mimeType"], "image/png");

    // Garbage does not kill the server.
    c.send_raw("this is not json");
    let err = c.read();
    assert_eq!(err["error"]["code"], -32700);
    let pong = c.ask(json!({"jsonrpc": "2.0", "id": 5, "method": "ping"}));
    assert_eq!(pong["id"], 5);

    c.stdin = None; // closing stdin ends the server
    assert!(c.child.wait().unwrap().success());
}
