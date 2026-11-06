//! Test helper that behaves like an agent CLI in `custom command` mode: reads the prompt on stdin
//! and prints a reply on stdout. If FAKE_AGENT_REPLY_FILE is set, the file's text is the reply;
//! otherwise it echoes how many bytes it received. FAKE_AGENT_DUMP_STDIN (a path) stores the prompt.

use std::io::Read;

fn main() {
    let mut input = String::new();
    let _ = std::io::stdin().read_to_string(&mut input);
    if let Ok(p) = std::env::var("FAKE_AGENT_DUMP_STDIN") {
        let _ = std::fs::write(p, &input);
    }
    match std::env::var("FAKE_AGENT_REPLY_FILE") {
        Ok(f) => print!("{}", std::fs::read_to_string(f).unwrap_or_default()),
        Err(_) => print!("received {} bytes", input.len()),
    }
}
