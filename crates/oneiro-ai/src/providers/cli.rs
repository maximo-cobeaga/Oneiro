//! Agent CLI bridge: runs the user's own installed agent (Claude Code, Codex, or any command) as a
//! subprocess in non-interactive mode. Authentication lives entirely inside that CLI, under the
//! user's own login; Oneiro never reads, stores or forwards tokens. The CLI is started in an empty
//! scratch folder with tools disabled where the CLI allows it, so it only generates text.

use crate::types::*;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CliKind {
    Claude,
    Codex,
    Custom,
}

pub struct AgentCli {
    pub kind: CliKind,
    pub command: String,
    pub extra_args: Vec<String>,
    pub model: Option<String>,
    pub env: Vec<(String, String)>,
    pub timeout_secs: u64,
}

impl AgentCli {
    pub fn claude(model: Option<String>) -> Self {
        AgentCli { kind: CliKind::Claude, command: "claude".into(), extra_args: vec![], model: model.filter(|m| !m.is_empty()), env: vec![], timeout_secs: 600 }
    }
    pub fn codex(model: Option<String>) -> Self {
        AgentCli { kind: CliKind::Codex, command: "codex".into(), extra_args: vec![], model: model.filter(|m| !m.is_empty()), env: vec![], timeout_secs: 900 }
    }
    /// Any command that reads the prompt from stdin and prints the reply on stdout.
    pub fn custom(command: &str, args: Vec<String>) -> Self {
        AgentCli { kind: CliKind::Custom, command: command.into(), extra_args: args, model: None, env: vec![], timeout_secs: 600 }
    }

    /// The prompt as one text document (CLIs are single-shot, so the conversation is flattened).
    pub fn flatten_prompt(req: &ChatRequest) -> String {
        let mut s = String::new();
        s.push_str(&req.system);
        if req.messages.len() == 1 {
            s.push_str("\n\n# Request\n");
            s.push_str(&req.messages[0].text);
        } else {
            s.push_str("\n\n# Conversation so far\n");
            for m in &req.messages {
                s.push_str(if m.role == Role::User { "\n[USER]\n" } else { "\n[ASSISTANT]\n" });
                s.push_str(&m.text);
                s.push('\n');
            }
            s.push_str("\n# Your turn\nReply as the ASSISTANT to the last [USER] message. Output only your reply.\n");
        }
        s
    }

    fn workdir() -> PathBuf {
        let d = std::env::temp_dir().join("oneiro-ai-bridge");
        let _ = std::fs::create_dir_all(&d);
        d
    }
}

pub struct ProcOutput {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
}

/// Finds an executable like the shell would (PATH + PATHEXT on Windows).
pub fn resolve_program(program: &str) -> Option<PathBuf> {
    let p = std::path::Path::new(program);
    let exts: Vec<String> = if cfg!(windows) {
        let mut v: Vec<String> = std::env::var("PATHEXT").unwrap_or(".COM;.EXE;.BAT;.CMD".into()).split(';').filter(|s| !s.is_empty()).map(|s| s.to_lowercase()).collect();
        // A name that already has an extension (or a bare file) is tried last: npm leaves extensionless sh shims next to the .cmd.
        v.push(String::new());
        v
    } else {
        vec![String::new()]
    };
    let try_in = |dir: &std::path::Path| -> Option<PathBuf> {
        for e in &exts {
            let cand = dir.join(format!("{}{}", program, e));
            if cand.is_file() {
                return Some(cand);
            }
        }
        None
    };
    if p.components().count() > 1 {
        return try_in(std::path::Path::new(""));
    }
    std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).find_map(|d| try_in(&d)))
}

/// Runs a process with `stdin_text` on stdin, a timeout, and captured output.
pub fn run_process(program: &str, args: &[String], env: &[(String, String)], stdin_text: &str, workdir: &PathBuf, timeout: Duration) -> Result<ProcOutput, String> {
    let resolved = resolve_program(program).ok_or_else(|| format!("could not start '{}': command not found (is it installed and on PATH?)", program))?;
    let is_script = cfg!(windows) && matches!(resolved.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(), Some("cmd") | Some("bat"));
    let mut cmd = if is_script {
        // npm-installed CLIs are .cmd shims on Windows, which need cmd.exe to run.
        let mut c = Command::new("cmd");
        c.arg("/C").arg(&resolved);
        c
    } else {
        Command::new(&resolved)
    };
    cmd.args(args).envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str()))).current_dir(workdir).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| format!("could not start '{}': {}", program, e))?;
    let mut stdin = child.stdin.take().ok_or("no stdin")?;
    let input = stdin_text.to_string();
    let writer = std::thread::spawn(move || {
        let _ = stdin.write_all(input.as_bytes());
    });
    let mut so = child.stdout.take().ok_or("no stdout")?;
    let mut se = child.stderr.take().ok_or("no stderr")?;
    let out_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = so.read_to_end(&mut b);
        b
    });
    let err_t = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = se.read_to_end(&mut b);
        b
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() > timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("'{}' did not answer within {} seconds", program, timeout.as_secs()));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(e) => return Err(e.to_string()),
        }
    };
    let _ = writer.join();
    let stdout = String::from_utf8_lossy(&out_t.join().unwrap_or_default()).to_string();
    let stderr = String::from_utf8_lossy(&err_t.join().unwrap_or_default()).to_string();
    Ok(ProcOutput { stdout, stderr, code: status.code() })
}

impl Provider for AgentCli {
    fn name(&self) -> String {
        match self.kind {
            CliKind::Claude => "Claude Code CLI (your own login)".into(),
            CliKind::Codex => "Codex CLI (your own login)".into(),
            CliKind::Custom => format!("custom command '{}'", self.command),
        }
    }

    fn supports_images(&self) -> bool {
        matches!(self.kind, CliKind::Codex | CliKind::Claude)
    }

    fn complete(&self, req: &ChatRequest) -> Result<ChatResponse, String> {
        let prompt = Self::flatten_prompt(req);
        let workdir = Self::workdir();
        let timeout = Duration::from_secs(self.timeout_secs);
        match self.kind {
            CliKind::Claude => {
                // Claude Code reads images with its Read tool. With pictures in the request only that one tool is
                // enabled (in the scratch folder); otherwise every tool stays off.
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                let mut image_files: Vec<PathBuf> = Vec::new();
                for (i, png) in req.messages.iter().flat_map(|m| m.images.iter()).enumerate() {
                    let p = workdir.join(format!("claude_img_{}_{}.png", stamp, i));
                    if std::fs::write(&p, png).is_ok() {
                        image_files.push(p);
                    }
                }
                let mut prompt = prompt;
                let mut args: Vec<String> = ["-p", "--output-format", "text", "--no-session-persistence", "--disable-slash-commands"].iter().map(|s| s.to_string()).collect();
                if image_files.is_empty() {
                    args.extend(["--tools".to_string(), String::new()]);
                } else {
                    args.extend(["--tools".to_string(), "Read".to_string(), "--allowedTools".to_string(), "Read".to_string()]);
                    prompt.push_str("\n\n# Pictures\nLook at these image files with the Read tool before answering (oldest first):\n");
                    for p in &image_files {
                        prompt.push_str(&format!("- {}\n", p.display()));
                    }
                    prompt.push_str("Then reply exactly as requested, with no commentary about reading the files.\n");
                }
                if let Some(m) = &self.model {
                    args.push("--model".into());
                    args.push(m.clone());
                }
                args.extend(self.extra_args.iter().cloned());
                let out = run_process(&self.command, &args, &self.env, &prompt, &workdir, timeout);
                for p in &image_files {
                    let _ = std::fs::remove_file(p);
                }
                finish(out?, &self.command)
            }
            CliKind::Codex => {
                let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
                let outfile = workdir.join(format!("codex_reply_{}.txt", stamp));
                let mut args: Vec<String> = ["exec", "--skip-git-repo-check", "--ephemeral", "--sandbox", "read-only", "--color", "never", "-o"]
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                args.push(outfile.to_string_lossy().to_string());
                if let Some(m) = &self.model {
                    args.push("-m".into());
                    args.push(m.clone());
                }
                let mut image_files = vec![];
                for (i, png) in req.messages.iter().flat_map(|m| m.images.iter()).enumerate() {
                    let p = workdir.join(format!("codex_img_{}_{}.png", stamp, i));
                    if std::fs::write(&p, png).is_ok() {
                        args.push("-i".into());
                        args.push(p.to_string_lossy().to_string());
                        image_files.push(p);
                    }
                }
                args.extend(self.extra_args.iter().cloned());
                args.push("-".into());
                let out = run_process(&self.command, &args, &self.env, &prompt, &workdir, timeout);
                for p in &image_files {
                    let _ = std::fs::remove_file(p);
                }
                let out = out?;
                let last = std::fs::read_to_string(&outfile).unwrap_or_default();
                let _ = std::fs::remove_file(&outfile);
                if !last.trim().is_empty() {
                    return Ok(ChatResponse { text: last.trim().to_string(), usage: None });
                }
                finish(out, &self.command)
            }
            CliKind::Custom => {
                let out = run_process(&self.command, &self.extra_args, &self.env, &prompt, &workdir, timeout)?;
                finish(out, &self.command)
            }
        }
    }
}

fn finish(out: ProcOutput, command: &str) -> Result<ChatResponse, String> {
    if out.code != Some(0) {
        let detail: String = if out.stderr.trim().is_empty() { out.stdout.chars().take(400).collect() } else { out.stderr.chars().take(400).collect() };
        return Err(format!("'{}' failed (exit code {:?}): {}", command, out.code, detail.trim()));
    }
    let text = out.stdout.trim().to_string();
    if text.is_empty() {
        return Err(format!("'{}' returned an empty reply", command));
    }
    Ok(ChatResponse { text, usage: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn single_message_prompt_has_system_then_request() {
        let req = ChatRequest { system: "SYS".into(), messages: vec![Message::user("do it")], max_tokens: 10 };
        let p = AgentCli::flatten_prompt(&req);
        assert!(p.starts_with("SYS") && p.contains("# Request\ndo it") && !p.contains("[USER]"));
    }

    #[test]
    fn multi_turn_prompt_keeps_the_conversation() {
        let req = ChatRequest {
            system: "SYS".into(),
            messages: vec![Message::user("a"), Message::assistant("b"), Message::user("c")],
            max_tokens: 10,
        };
        let p = AgentCli::flatten_prompt(&req);
        let (a, b, c) = (p.find("\na\n").unwrap(), p.find("\nb\n").unwrap(), p.find("\nc\n").unwrap());
        assert!(a < b && b < c && p.contains("[ASSISTANT]"));
    }
}

#[cfg(all(test, windows))]
mod windows_tests {
    use super::*;

    #[test]
    fn cmd_shims_win_over_extensionless_scripts() {
        let dir = std::env::temp_dir().join(format!("oneiro_shim_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fakeagent"), "#!/bin/sh\n").unwrap();
        std::fs::write(dir.join("fakeagent.cmd"), "@echo off\n").unwrap();
        let found = resolve_program(&dir.join("fakeagent").to_string_lossy()).unwrap();
        assert_eq!(found.extension().and_then(|e| e.to_str()), Some("cmd"), "{:?}", found);
        assert!(resolve_program("definitely-not-a-real-agent-xyz").is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
