//! Phase 3 tests. No real model is needed: providers are exercised against scripted/mock servers,
//! the director loop against a scripted provider, and the tools against a temporary copy of the example.

use oneiro_ai::b64;
use oneiro_ai::director::{direct_character, direct_scene, DirectorOpts, ProjectCtx, Turn};
use oneiro_ai::guide::{self, Mode};
use oneiro_ai::mcp::handle;
use oneiro_ai::providers::anthropic::AnthropicApi;
use oneiro_ai::providers::cli::AgentCli;
use oneiro_ai::providers::mock::MockProvider;
use oneiro_ai::providers::openai::OpenAiCompatible;
use oneiro_ai::tools::Toolbox;
use oneiro_ai::types::*;
use oneiro_core::lock::Lock;
use oneiro_core::Project;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn example() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap()
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let p = e.path();
        let t = to.join(e.file_name());
        if p.is_dir() {
            copy_dir(&p, &t);
        } else {
            std::fs::copy(&p, &t).unwrap();
        }
    }
}

/// A private copy of the example project (tests that write files must not touch the real one).
fn temp_project() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("oneiro_ai_test_{}_{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)));
    let _ = std::fs::remove_dir_all(&dir);
    copy_dir(&example(), &dir);
    let _ = std::fs::remove_dir_all(dir.join("out"));
    dir
}

fn script_text() -> String {
    std::fs::read_to_string(example().join("script.onr")).unwrap()
}

fn project() -> Project {
    let mut d = vec![];
    Project::load(&example(), &mut d)
}

fn fenced(script: &str) -> String {
    format!("Done.\n```oneiro\n{}\n```", script)
}

fn broken() -> String {
    script_text().replacen("Blob walks_to diner.counter", "Blob walk_to diner.counter", 1)
}

fn run_director(p: &dyn Provider, req: &str, opts: &DirectorOpts, hist: &[Turn]) -> oneiro_ai::SceneResult {
    let proj = project();
    let lock = Lock::default();
    let ctx = ProjectCtx { project: &proj, lock: &lock, script_label: "script.onr".into() };
    direct_scene(p, &ctx, &script_text(), hist, req, opts, &mut |_| {}).unwrap()
}

// ------------------------------------------------------------------ director loop

#[test]
fn the_director_feeds_engine_errors_back_until_the_script_is_valid() {
    let mock = MockProvider::new(vec![fenced(&broken()), fenced(&script_text())]);
    let r = run_director(&mock, "make Blob walk to the counter", &DirectorOpts::default(), &[]);
    assert!(r.valid, "{:?}", r.diagnostics);
    assert_eq!(r.rounds, 2);
    assert_eq!(r.notes.len(), 1);
    assert_eq!(r.script.as_deref().map(str::trim_end), Some(script_text().trim_end()));
    // The second request carries the engine's complaint, with code, suggestion and JSON.
    let reqs = mock.requests.lock().unwrap();
    assert_eq!(reqs.len(), 2);
    let last = &reqs[1].messages.last().unwrap().text;
    assert!(last.contains("E1042") && last.contains("walks_to") && last.contains("\"code\""), "{}", last);
    assert_eq!(reqs[1].messages.len(), 3, "user, assistant (the bad script), user (feedback)");
}

#[test]
fn the_director_gives_up_after_max_rounds_and_reports_what_is_left() {
    let mock = MockProvider::new(vec![fenced(&broken())]);
    let opts = DirectorOpts { max_rounds: 3, ..Default::default() };
    let r = run_director(&mock, "x", &opts, &[]);
    assert!(!r.valid);
    assert_eq!(r.rounds, 3);
    assert!(r.diagnostics.iter().any(|d| d.code == "E1042"));
    assert!(r.script.is_some());
}

#[test]
fn a_reply_without_a_script_is_just_an_answer() {
    let mock = MockProvider::new(vec!["The scene is about 30 seconds long.".into()]);
    let r = run_director(&mock, "how long is it?", &DirectorOpts::default(), &[]);
    assert!(r.valid && r.script.is_none() && r.reply.contains("30 seconds") && r.rounds == 1);
}

#[test]
fn history_and_context_reach_the_model() {
    let mock = MockProvider::new(vec!["ok".into()]);
    let hist = vec![Turn { user: "first ask".into(), assistant: "first answer".into() }];
    run_director(&mock, "second ask", &DirectorOpts::default(), &hist);
    let reqs = mock.requests.lock().unwrap();
    let r = &reqs[0];
    assert_eq!(r.messages.len(), 3);
    assert_eq!(r.messages[0].text, "first ask");
    assert!(r.messages[2].text.contains("second ask") && r.messages[2].text.contains("## Current script"));
    // The system prompt teaches the language and describes THIS project.
    for needle in ["walks_to", "over_shoulder", "diner.counter", "Marta", "Blob", "series bible", "never invent audio", "marta_01.wav"] {
        assert!(r.system.to_lowercase().contains(&needle.to_lowercase()), "system prompt lacks {:?}", needle);
    }
}

#[test]
fn visual_self_check_sends_a_contact_sheet_and_accepts_looks_good() {
    let mock = MockProvider::new(vec![fenced(&script_text()), "LOOKS GOOD".into()]).with_images();
    let opts = DirectorOpts { self_check: true, ..Default::default() };
    let r = run_director(&mock, "tidy up", &opts, &[]);
    assert!(r.valid && r.rounds == 2);
    assert!(r.notes.iter().any(|n| n.contains("framing fine")), "{:?}", r.notes);
    let reqs = mock.requests.lock().unwrap();
    let check = reqs[1].messages.last().unwrap();
    assert!(check.text.contains("contact sheet") && check.text.contains("over_shoulder"));
    assert_eq!(check.images.len(), 1);
    assert_eq!(&check.images[0][..4], &[0x89, b'P', b'N', b'G']);
}

#[test]
fn visual_self_check_can_adjust_the_script_but_never_keeps_a_broken_revision() {
    let adjusted = script_text().replacen("camera wide from east move dolly_out for 5s", "camera wide from east move dolly_out for 6s", 1);
    let mock = MockProvider::new(vec![fenced(&script_text()), format!("The last shot felt short.\n```oneiro\n{}\n```", adjusted)]).with_images();
    let opts = DirectorOpts { self_check: true, ..Default::default() };
    let r = run_director(&mock, "x", &opts, &[]);
    assert!(r.script.as_deref().unwrap().contains("for 6s") && r.reply.contains("last shot"));
    assert!(r.notes.iter().any(|n| n.contains("framing adjusted")));

    let mock2 = MockProvider::new(vec![fenced(&script_text()), fenced(&broken())]).with_images();
    let r2 = run_director(&mock2, "x", &opts, &[]);
    assert!(r2.valid && !r2.script.as_deref().unwrap().contains("walk_to"));
    assert!(r2.notes.iter().any(|n| n.contains("keeping the previous valid script")));
}

#[test]
fn self_check_is_skipped_for_providers_that_cannot_see() {
    let mock = MockProvider::new(vec![fenced(&script_text())]);
    let opts = DirectorOpts { self_check: true, ..Default::default() };
    let r = run_director(&mock, "x", &opts, &[]);
    assert!(r.valid && r.rounds == 1 && mock.request_count() == 1);
}

#[test]
fn characters_are_designed_with_the_same_validate_and_retry_loop() {
    let bad = "A sad clown.\n```onchar\ncharacter Pip {\n  build { head: library.heads.rond, body: library.bodies.kid }\n  wardrobe { default: [clown_nose] }\n}\n```";
    let good = "Fixed.\n```onchar\ncharacter Pip {\n  rig: library.rigs.biped_cutout\n  build {\n    head: library.heads.round { skin_tone: #F5E6D3, size: 1.2 }\n    eyes: library.eyes.wide\n    body: library.bodies.adult { height: 1.1 }\n  }\n  wardrobe { default: [yellow_jacket, brown_pants, black_shoes] }\n  voice { provider: file, pitch: 300 }\n}\n```";
    let mock = MockProvider::new(vec![bad.into(), good.into()]);
    let proj = project();
    let r = direct_character(&mock, &proj, "a sad clown named Pip", &[], &DirectorOpts::default(), &mut |_| {}).unwrap();
    assert!(r.valid && r.rounds == 2, "{:?}", r.diagnostics);
    let def = r.def.unwrap();
    assert_eq!(def.name, "Pip");
    assert_eq!(def.head.kind, "round");
    assert!((def.voice_pitch - 300.0).abs() < 1e-9);
    let reqs = mock.requests.lock().unwrap();
    let feedback = &reqs[1].messages.last().unwrap().text;
    assert!(feedback.contains("E1103") && feedback.contains("E1104"), "{}", feedback);
    assert!(r.source.unwrap().starts_with("character Pip {"));
}

#[test]
fn the_character_prompt_lists_the_catalog() {
    let s = guide::system_prompt(&project(), Mode::Character);
    for needle in ["red_beanie", "visemes_basic", "onchar", "pitch"] {
        assert!(s.contains(needle), "{}", needle);
    }
}

// ------------------------------------------------------------------ HTTP providers (mock server)

fn serve_once(status: u16, body: String) -> (String, std::thread::JoinHandle<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 8192];
        let mut header_end = None;
        let mut need = 0usize;
        loop {
            let n = s.read(&mut tmp).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            if header_end.is_none() {
                if let Some(p) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    header_end = Some(p + 4);
                    let head = String::from_utf8_lossy(&buf[..p]).to_lowercase();
                    need = head.lines().find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0))).unwrap_or(0);
                }
            }
            if let Some(he) = header_end {
                if buf.len() >= he + need {
                    break;
                }
            }
        }
        let resp = format!("HTTP/1.1 {} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", status, body.len(), body);
        let _ = s.write_all(resp.as_bytes());
        String::from_utf8_lossy(&buf).to_string()
    });
    (format!("http://127.0.0.1:{}", port), h)
}

fn body_of(request: &str) -> Value {
    serde_json::from_str(request.split("\r\n\r\n").nth(1).unwrap()).unwrap()
}

fn req_with_image() -> ChatRequest {
    ChatRequest { system: "SYS".into(), messages: vec![Message::user("hello").with_images(vec![vec![0x89, b'P', b'N', b'G', 1, 2, 3]])], max_tokens: 123 }
}

#[test]
fn anthropic_api_request_and_response_shapes() {
    let reply = json!({"content": [{"type": "text", "text": "Hi "}, {"type": "text", "text": "there"}], "usage": {"input_tokens": 11, "output_tokens": 7}});
    let (url, h) = serve_once(200, reply.to_string());
    let p = AnthropicApi::new("secret-key".into(), Some("claude-test".into()), Some(url));
    let r = p.complete(&req_with_image()).unwrap();
    assert_eq!(r.text, "Hi there");
    assert_eq!(r.usage, Some((11, 7)));
    let raw = h.join().unwrap();
    let lower = raw.to_lowercase();
    assert!(raw.starts_with("POST /v1/messages"));
    assert!(lower.contains("x-api-key: secret-key") && lower.contains("anthropic-version: 2023-06-01"));
    let b = body_of(&raw);
    assert_eq!(b["model"], "claude-test");
    assert_eq!(b["max_tokens"], 123);
    assert_eq!(b["system"], "SYS");
    let content = &b["messages"][0]["content"];
    assert_eq!(content[0]["type"], "image");
    assert_eq!(content[0]["source"]["media_type"], "image/png");
    assert_eq!(b64::decode(content[0]["source"]["data"].as_str().unwrap()).unwrap(), vec![0x89, b'P', b'N', b'G', 1, 2, 3]);
    assert_eq!(content[1]["text"], "hello");
}

#[test]
fn anthropic_errors_are_readable_and_a_missing_key_is_explained() {
    let (url, h) = serve_once(401, json!({"type": "error", "error": {"type": "authentication_error", "message": "invalid x-api-key"}}).to_string());
    let e = AnthropicApi::new("bad".into(), None, Some(url)).complete(&req_with_image()).unwrap_err();
    assert!(e.contains("401") && e.contains("invalid x-api-key"), "{}", e);
    let _ = h.join();
    let e = AnthropicApi::new(String::new(), None, None).complete(&req_with_image()).unwrap_err();
    assert!(e.contains("ANTHROPIC_API_KEY"));
}

#[test]
fn openai_compatible_request_and_response_shapes() {
    let reply = json!({"choices": [{"message": {"role": "assistant", "content": "local says hi"}}], "usage": {"prompt_tokens": 5, "completion_tokens": 3}});
    let (url, h) = serve_once(200, reply.to_string());
    let p = OpenAiCompatible::new(&format!("{}/v1", url), Some("tok".into()), "llama3.1", true);
    let r = p.complete(&req_with_image()).unwrap();
    assert_eq!(r.text, "local says hi");
    assert_eq!(r.usage, Some((5, 3)));
    let raw = h.join().unwrap();
    assert!(raw.starts_with("POST /v1/chat/completions"));
    assert!(raw.to_lowercase().contains("authorization: bearer tok"));
    let b = body_of(&raw);
    assert_eq!(b["model"], "llama3.1");
    assert_eq!(b["messages"][0]["role"], "system");
    assert_eq!(b["messages"][0]["content"], "SYS");
    let content = &b["messages"][1]["content"];
    assert_eq!(content[0]["text"], "hello");
    assert!(content[1]["image_url"]["url"].as_str().unwrap().starts_with("data:image/png;base64,"));
}

#[test]
fn openai_compatible_without_vision_sends_plain_text_and_no_auth_header() {
    let reply = json!({"choices": [{"message": {"content": "ok"}}]});
    let (url, h) = serve_once(200, reply.to_string());
    let p = OpenAiCompatible::new(&url, None, "m", false);
    p.complete(&req_with_image()).unwrap();
    let raw = h.join().unwrap();
    assert!(!raw.to_lowercase().contains("authorization:"));
    assert_eq!(body_of(&raw)["messages"][1]["content"], "hello");
    let (url2, h2) = serve_once(500, "upstream exploded".into());
    let e = OpenAiCompatible::new(&url2, None, "m", false).complete(&req_with_image()).unwrap_err();
    assert!(e.contains("500") && e.contains("upstream exploded"), "{}", e);
    let _ = h2.join();
    // Nothing listening: a clear connection error, not a hang.
    let e = OpenAiCompatible::new("http://127.0.0.1:9", None, "m", false).complete(&req_with_image()).unwrap_err();
    assert!(e.contains("could not reach"), "{}", e);
}

// ------------------------------------------------------------------ agent CLI bridge

#[test]
fn the_cli_bridge_pipes_the_prompt_to_stdin_and_returns_stdout() {
    let tmp = std::env::temp_dir().join(format!("oneiro_agent_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let reply_file = tmp.join("reply.txt");
    let dump = tmp.join("stdin.txt");
    std::fs::write(&reply_file, "Sure.\n```oneiro\noneiro 0.1\n```").unwrap();
    let mut agent = AgentCli::custom(env!("CARGO_BIN_EXE_fake_agent"), vec![]);
    agent.env = vec![("FAKE_AGENT_REPLY_FILE".into(), reply_file.to_string_lossy().to_string()), ("FAKE_AGENT_DUMP_STDIN".into(), dump.to_string_lossy().to_string())];
    let r = agent.complete(&ChatRequest { system: "SYSTEM TEXT".into(), messages: vec![Message::user("USER ASK")], max_tokens: 10 }).unwrap();
    assert!(r.text.starts_with("Sure.") && r.text.contains("```oneiro"));
    let stdin = std::fs::read_to_string(&dump).unwrap();
    assert!(stdin.contains("SYSTEM TEXT") && stdin.contains("USER ASK"));
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn the_cli_bridge_reports_missing_commands_and_failures() {
    let e = AgentCli::custom("definitely-not-a-real-agent-xyz", vec![]).complete(&req_with_image()).unwrap_err();
    assert!(e.contains("could not start"), "{}", e);
}

#[test]
fn the_director_works_end_to_end_through_the_cli_bridge() {
    let tmp = std::env::temp_dir().join(format!("oneiro_agent_e2e_{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    let reply_file = tmp.join("reply.txt");
    std::fs::write(&reply_file, fenced(&script_text())).unwrap();
    let mut agent = AgentCli::custom(env!("CARGO_BIN_EXE_fake_agent"), vec![]);
    agent.env = vec![("FAKE_AGENT_REPLY_FILE".into(), reply_file.to_string_lossy().to_string())];
    let r = run_director(&agent, "x", &DirectorOpts::default(), &[]);
    assert!(r.valid && r.script.is_some());
    let _ = std::fs::remove_dir_all(&tmp);
}

// ------------------------------------------------------------------ tools and MCP

fn text_of(v: &Value) -> String {
    v["result"]["content"][0]["text"].as_str().unwrap_or_default().to_string()
}

#[test]
fn tools_validate_save_and_show_the_script() {
    let dir = temp_project();
    let tb = Toolbox::open(&dir).unwrap();

    let ok = tb.call("check_script", &json!({}));
    assert!(!ok.is_error);
    let v: Value = serde_json::from_str(&ok.text).unwrap();
    assert_eq!(v["valid"], true);
    assert!(v["shots"].as_array().unwrap().len() >= 6 && v["duration_seconds"].as_f64().unwrap() > 28.0);

    let bad = tb.call("check_script", &json!({"script": broken()}));
    let v: Value = serde_json::from_str(&bad.text).unwrap();
    assert_eq!(v["valid"], false);
    assert_eq!(v["diagnostics"][0]["code"], "E1042");

    // write_script refuses errors and leaves the file alone...
    let before = std::fs::read_to_string(dir.join("script.onr")).unwrap();
    let w = tb.call("write_script", &json!({"script": broken()}));
    assert!(w.is_error && w.text.contains("Not saved"));
    assert_eq!(std::fs::read_to_string(dir.join("script.onr")).unwrap(), before);
    // ...and saves valid ones, keeping a backup.
    let edited = before.replacen("wait 3.2s", "wait 4s", 1);
    let w = tb.call("write_script", &json!({"script": edited}));
    assert!(!w.is_error, "{}", w.text);
    assert!(std::fs::read_to_string(dir.join("script.onr")).unwrap().contains("wait 4s"));
    assert_eq!(std::fs::read_to_string(dir.join("script.onr.bak")).unwrap(), before);

    let tl = tb.call("describe_timeline", &json!({}));
    assert!(tl.text.contains("Blob walks_to") && tl.text.contains("shot 1") && tl.text.contains("line"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tools_let_the_ai_see_the_result() {
    let dir = temp_project();
    let tb = Toolbox::open(&dir).unwrap();
    let still = tb.call("render_still", &json!({"time": 8.5, "width": 640}));
    assert!(!still.is_error, "{}", still.text);
    assert_eq!(&still.images[0][..4], &[0x89, b'P', b'N', b'G']);
    assert!(still.text.contains("over_shoulder"), "{}", still.text);
    let sheet = tb.call("contact_sheet", &json!({"max_shots": 4}));
    assert_eq!(sheet.images.len(), 1);
    assert_eq!(sheet.text.lines().filter(|l| l.contains("t=")).count(), 4);
    // a script with errors cannot be rendered
    let bad = tb.call("render_still", &json!({"time": 1.0, "script": broken()}));
    assert!(bad.is_error && bad.images.is_empty());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn tools_manage_the_library_and_render_video() {
    let dir = temp_project();
    let tb = Toolbox::open(&dir).unwrap();
    let lib = tb.call("list_library", &json!({}));
    assert!(lib.text.contains("diner.counter") && lib.text.contains("Blob"));
    let source = "character Zed {\n  rig: library.rigs.biped_cutout\n  build { head: library.heads.square { size: 1.1 }, body: library.bodies.adult }\n  wardrobe { default: [brown_jacket] }\n}";
    let w = tb.call("write_character", &json!({"source": source}));
    assert!(!w.is_error, "{}", w.text);
    assert!(dir.join("library/characters/Zed.onchar").exists());
    let bad = tb.call("write_character", &json!({"source": "character zed { build { head: library.heads.rond } }"}));
    assert!(bad.is_error && bad.text.contains("E1103"));
    // The saved character is usable from a script.
    let script = script_text().replacen("use character Blob", "use character Blob\nuse character Zed", 1);
    let chk = tb.call("check_script", &json!({"script": script}));
    assert_eq!(serde_json::from_str::<Value>(&chk.text).unwrap()["valid"], true, "{}", chk.text);

    let out = dir.join("out").join("ai_test.mp4");
    let v = tb.call("render_video", &json!({"output": out.to_string_lossy()}));
    assert!(!v.is_error, "{}", v.text);
    assert!(out.exists() && std::fs::metadata(&out).unwrap().len() > 10_000);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn the_guide_tool_contains_everything_an_agent_needs() {
    let tb = Toolbox::open(&example()).unwrap();
    let g = tb.call("get_guide", &json!({}));
    for needle in ["Oneiro language guide", "walks_to", "diner.stool_2", "series bible", "character Luna", "check_script"] {
        assert!(g.text.contains(needle), "guide lacks {:?}", needle);
    }
}

#[test]
fn mcp_handshake_tool_listing_and_calls() {
    let dir = temp_project();
    let tb = Toolbox::open(&dir).unwrap();
    let init = handle(&tb, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-03-26", "capabilities": {}, "clientInfo": {"name": "t", "version": "1"}}})).unwrap();
    assert_eq!(init["id"], 1);
    assert_eq!(init["result"]["protocolVersion"], "2025-03-26");
    assert_eq!(init["result"]["serverInfo"]["name"], "oneiro");
    assert!(init["result"]["instructions"].as_str().unwrap().contains("get_guide"));
    assert!(handle(&tb, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());

    let list = handle(&tb, &json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"})).unwrap();
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    for n in ["get_guide", "read_script", "check_script", "write_script", "describe_timeline", "render_still", "contact_sheet", "render_video", "list_library", "write_character"] {
        assert!(names.contains(&n), "missing tool {}", n);
    }
    assert!(list["result"]["tools"][0]["inputSchema"]["type"] == "object");

    let chk = handle(&tb, &json!({"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "check_script", "arguments": {}}})).unwrap();
    assert_eq!(chk["result"]["isError"], false);
    assert!(text_of(&chk).contains("\"valid\": true"));

    let img = handle(&tb, &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "render_still", "arguments": {"time": 3.0, "width": 480}}})).unwrap();
    let c = &img["result"]["content"];
    assert_eq!(c[1]["type"], "image");
    assert_eq!(c[1]["mimeType"], "image/png");
    assert_eq!(&b64::decode(c[1]["data"].as_str().unwrap()).unwrap()[..4], &[0x89, b'P', b'N', b'G']);

    let unknown_tool = handle(&tb, &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "nope"}})).unwrap();
    assert_eq!(unknown_tool["error"]["code"], -32602);
    let unknown_method = handle(&tb, &json!({"jsonrpc": "2.0", "id": 6, "method": "bogus"})).unwrap();
    assert_eq!(unknown_method["error"]["code"], -32601);

    let res = handle(&tb, &json!({"jsonrpc": "2.0", "id": 7, "method": "resources/read", "params": {"uri": "oneiro://guide"}})).unwrap();
    assert!(res["result"]["contents"][0]["text"].as_str().unwrap().contains("Oneiro language guide"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn audio_tools_create_sounds_voices_and_report_the_mix() {
    let dir = temp_project();
    let tb = Toolbox::open(&dir).unwrap();

    let sfx = tb.call("generate_sound", &json!({"kind": "sfx", "preset": "boing", "name": "bounce"}));
    assert!(!sfx.is_error, "{}", sfx.text);
    assert!(dir.join("library/sounds/bounce.wav").exists());
    let music = tb.call("generate_sound", &json!({"kind": "music", "mood": "quirky", "name": "bgm", "bars": 2}));
    assert!(!music.is_error && dir.join("library/music/bgm.wav").exists(), "{}", music.text);
    let bad = tb.call("generate_sound", &json!({"kind": "sfx", "preset": "kazoo"}));
    assert!(bad.is_error && bad.text.contains("Presets:"));
    let evil = tb.call("generate_sound", &json!({"kind": "sfx", "preset": "boing", "name": "../escape"}));
    assert!(evil.is_error && !dir.join("escape.wav").exists());

    // The new sounds are usable, and the guide lists them for the next request.
    let script = script_text().replacen("  sfx door_bell", "  music bgm volume 0.5\n  sfx door_bell\n  sfx bounce", 1).replacen("  Marta shakes_head", "  Marta (happy): \"One more thing.\"\n  Marta shakes_head", 1);
    let w = tb.call("write_script", &json!({"script": script}));
    assert!(!w.is_error, "{}", w.text);
    let g = tb.call("get_guide", &json!({}));
    assert!(g.text.contains("bounce.wav") && g.text.contains("music stop") && g.text.contains("generate_sound"), "guide must teach the audio statements");

    // Voices: the new line gets audio and a clause in the script.
    let v = tb.call("generate_voices", &json!({"provider": "demo"}));
    assert!(!v.is_error, "{}", v.text);
    assert!(v.text.contains("marta_04.wav") && v.text.contains("already has audio"), "{}", v.text);
    let saved = std::fs::read_to_string(dir.join("script.onr")).unwrap();
    assert!(saved.contains("One more thing.\" audio \"audio/marta_04.wav\""), "{}", saved);
    let again = tb.call("generate_voices", &json!({"provider": "demo"}));
    assert!(!again.text.contains("marta_04.wav (") && !again.is_error);
    assert!(tb.call("generate_voices", &json!({"provider": "nope"})).is_error);

    let m = tb.call("mix_audio", &json!({}));
    assert!(!m.is_error && m.text.contains("6 voice(s)") && m.text.contains("2 effect(s)"), "{}", m.text);
    assert!(m.text.contains("1 music track(s)") && m.text.contains("loudness"), "{}", m.text);
    let _ = std::fs::remove_dir_all(&dir);
}