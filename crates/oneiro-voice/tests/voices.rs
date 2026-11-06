use oneiro_audio::{dsp, write_wav, Audio, SR};
use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Project};
use oneiro_voice::providers::{self, split_command, ElevenLabs, OpenAiTts, VoiceProvider};
use oneiro_voice::*;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, q) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &q);
        } else {
            std::fs::copy(&p, &q).unwrap();
        }
    }
}

fn project(tag: &str) -> (PathBuf, Project) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short");
    let root = std::env::temp_dir().join(format!("oneiro_voice_test_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&src, &root);
    let mut d = vec![];
    let p = Project::load(&root, &mut d);
    (root, p)
}

fn req(text: &str) -> VoiceRequest {
    VoiceRequest { text: text.into(), voice: "voice123".into(), emotion: "angry".into(), pitch: 200.0, speed: 1.0, seed: 1 }
}

/// A one-shot HTTP server: answers the first request with `status` and `body`, sends the raw
/// request (headers + body) to the returned channel.
fn mock_server(status: u16, content_type: &str, body: Vec<u8>) -> (String, mpsc::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = mpsc::channel();
    let ct = content_type.to_string();
    std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let mut buf = Vec::new();
        let mut tmp = [0u8; 4096];
        loop {
            let n = s.read(&mut tmp).unwrap();
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&tmp[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
            if let Some(h) = text.find("\r\n\r\n") {
                let len = text[..h].lines().find_map(|l| l.to_lowercase().strip_prefix("content-length:").map(|v| v.trim().parse::<usize>().unwrap_or(0))).unwrap_or(0);
                if buf.len() >= h + 4 + len {
                    break;
                }
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&buf).to_string());
        let head = format!("HTTP/1.1 {} X\r\ncontent-type: {}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n", status, ct, body.len());
        s.write_all(head.as_bytes()).unwrap();
        s.write_all(&body).unwrap();
    });
    (url, rx)
}

fn tone_pcm16(secs: f32) -> Vec<u8> {
    let n = (24000.0 * secs) as usize;
    (0..n).flat_map(|i| (((2.0 * std::f32::consts::PI * 220.0 * i as f32 / 24000.0).sin() * 12000.0) as i16).to_le_bytes()).collect()
}

fn wav_bytes(secs: f32) -> Vec<u8> {
    let p = std::env::temp_dir().join(format!("oneiro_voice_wavbytes_{}_{}.wav", std::process::id(), oneiro_audio::unique_id()));
    let n = (SR as f32 * secs) as usize;
    let s: Vec<f32> = (0..n).map(|i| (2.0 * std::f32::consts::PI * 260.0 * i as f32 / SR as f32).sin() * 0.4).collect();
    write_wav(&p, &s, SR).unwrap();
    let b = std::fs::read(&p).unwrap();
    let _ = std::fs::remove_file(&p);
    b
}

#[test]
fn audio_clause_is_inserted_or_replaced_on_a_dialogue_line() {
    let l = r#"  Marta (annoyed): "Where's my pie?"  # about the pie"#;
    assert_eq!(set_audio_clause(l, "audio/m.wav").unwrap(), r#"  Marta (annoyed): "Where's my pie?" audio "audio/m.wav"  # about the pie"#);
    let l2 = r#"  Blob (nervous): "It's... coming." audio "audio/old.wav""#;
    assert_eq!(set_audio_clause(l2, "audio/new.wav").unwrap(), r#"  Blob (nervous): "It's... coming." audio "audio/new.wav""#);
    let l3 = r#"  A (happy): "She said \"hi\"""#;
    assert_eq!(set_audio_clause(l3, "a.wav").unwrap(), r#"  A (happy): "She said \"hi\"" audio "a.wav""#);
    assert!(set_audio_clause("  Marta walks_to diner.door", "a.wav").is_none());
    // Result still parses as the same dialogue with the new audio.
    let d = dialogues(&format!("oneiro 0.1\nscene 1 \"x\" in s {{\n{}\n}}\n", set_audio_clause(l, "audio/m.wav").unwrap()));
    assert_eq!(d[0].audio.as_deref(), Some("audio/m.wav"));
}

#[test]
fn next_audio_path_skips_existing_files_and_script_references() {
    let (root, _) = project("paths");
    let mut taken: std::collections::HashSet<String> = ["audio/marta_04.wav".to_string()].into();
    let p = next_audio_path(&root, "Marta", &mut taken); // marta_01..03 exist on disk, 04 is in the script
    assert_eq!(p, "audio/marta_05.wav");
    assert_eq!(next_audio_path(&root, "Marta", &mut taken), "audio/marta_06.wav");
    assert_eq!(next_audio_path(&root, "Zed", &mut taken), "audio/zed_01.wav");
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn generating_a_script_fills_missing_voices_and_is_idempotent() {
    let (root, p) = project("gen");
    let script = std::fs::read_to_string(root.join("script.onr")).unwrap();
    // Two new lines without audio.
    let text = script.replacen("  Marta shakes_head", "  Marta (happy): \"Just kidding, stay a little.\"\n  Blob (nervous): \"Oh. Good.\"\n  Marta shakes_head", 1);
    let cfg = VoiceConfig { default_provider: ProviderKind::Demo, ..Default::default() };
    let opts = GenOptions::default();
    let mut seen = 0;
    let (out, reps) = generate_script(&root, &p, &text, &cfg, &opts, &mut |_, _, _| {
        seen += 1;
        true
    })
    .unwrap();
    assert!(seen > 0);
    assert_eq!(reps.len(), 7);
    let made: Vec<_> = reps.iter().filter(|r| !r.skipped).collect();
    assert_eq!(made.len(), 2, "{:#?}", reps);
    assert!(made.iter().all(|r| r.error.is_none() && r.seconds > 0.3 && root.join(&r.path).exists()));
    assert!(out.contains("audio \"audio/marta_04.wav\"") && out.contains("audio \"audio/blob_03.wav\""), "{}", out);
    // The patched script compiles with no errors and uses the new audio for timing.
    let (ep, d) = compile_source(&p, "script.onr", &out, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert_eq!(ep.audio.iter().filter(|e| e.kind == oneiro_core::AudioKind::Voice).count(), 7);
    // Second run changes nothing.
    let (again, reps2) = generate_script(&root, &p, &out, &cfg, &opts, &mut |_, _, _| true).unwrap();
    assert_eq!(again, out);
    assert!(reps2.iter().all(|r| r.skipped));
    // Voices are levelled for the mix.
    let a = oneiro_audio::read_wav(&root.join("audio/marta_04.wav")).unwrap();
    assert!((dsp::loudness_db(&a) + 20.0).abs() < 2.0, "{}", dsp::loudness_db(&a));
    // Force regenerates an existing file but keeps the script untouched.
    let (forced, reps3) = generate_script(&root, &p, &out, &cfg, &GenOptions { force: true, only: vec!["Blob".into()], ..Default::default() }, &mut |_, _, _| true).unwrap();
    assert_eq!(forced, out);
    assert!(reps3.iter().all(|r| !r.skipped && r.who == "Blob"));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_missing_file_for_an_existing_clause_is_regenerated_in_place() {
    let (root, p) = project("regen");
    std::fs::remove_file(root.join("audio/blob_01.wav")).unwrap();
    let script = std::fs::read_to_string(root.join("script.onr")).unwrap();
    let cfg = VoiceConfig { default_provider: ProviderKind::Demo, ..Default::default() };
    let (out, reps) = generate_script(&root, &p, &script, &cfg, &GenOptions::default(), &mut |_, _, _| true).unwrap();
    assert_eq!(out, script);
    assert_eq!(reps.iter().filter(|r| !r.skipped).count(), 1);
    assert!(root.join("audio/blob_01.wav").exists());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn provider_errors_are_reported_per_line_without_touching_the_script() {
    let (root, p) = project("err");
    let script = std::fs::read_to_string(root.join("script.onr")).unwrap().replacen("  Marta shakes_head", "  Marta (happy): \"New line.\"\n  Marta shakes_head", 1);
    let cfg = VoiceConfig { elevenlabs_key_env: "ONEIRO_TEST_NO_SUCH_KEY".into(), ..Default::default() };
    let (out, reps) = generate_script(&root, &p, &script, &cfg, &GenOptions { provider: Some(ProviderKind::ElevenLabs), ..Default::default() }, &mut |_, _, _| true).unwrap();
    assert_eq!(out, script);
    let r = reps.iter().find(|r| !r.skipped).unwrap();
    assert!(r.error.as_deref().unwrap().contains("ONEIRO_TEST_NO_SUCH_KEY"), "{:?}", r.error);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn elevenlabs_sends_the_right_request_and_decodes_pcm() {
    let (url, rx) = mock_server(200, "audio/pcm", tone_pcm16(1.0));
    let el = ElevenLabs { key: "sk_test_123".into(), model: "eleven_multilingual_v2".into(), base_url: url };
    let a = el.synthesize(&req("Where's my pie?")).unwrap();
    assert_eq!(a.sr, 24000);
    assert!((a.duration() - 1.0).abs() < 0.01 && dsp::peak(&a.samples) > 0.3);
    let raw = rx.recv().unwrap();
    let lower = raw.to_lowercase();
    assert!(raw.starts_with("POST /v1/text-to-speech/voice123?output_format=pcm_24000"), "{}", raw.lines().next().unwrap());
    assert!(lower.contains("xi-api-key: sk_test_123"));
    let body: serde_json::Value = serde_json::from_str(&raw[raw.find("\r\n\r\n").unwrap() + 4..]).unwrap();
    assert_eq!(body["text"], "Where's my pie?");
    assert_eq!(body["model_id"], "eleven_multilingual_v2");
    assert!(body["voice_settings"]["stability"].as_f64().unwrap() < 0.4, "angry lines are less stable: {}", body);
}

#[test]
fn elevenlabs_errors_are_readable() {
    let (url, _rx) = mock_server(401, "application/json", br#"{"detail":{"status":"invalid_api_key","message":"Invalid API key"}}"#.to_vec());
    let el = ElevenLabs { key: "bad".into(), model: "m".into(), base_url: url };
    let e = el.synthesize(&req("hi")).unwrap_err();
    assert!(e.contains("401") && e.contains("Invalid API key") && e.contains("check the API key"), "{}", e);
    let no_voice = ElevenLabs { key: "k".into(), model: "m".into(), base_url: "http://127.0.0.1:1".into() };
    let mut r = req("hi");
    r.voice.clear();
    assert!(no_voice.synthesize(&r).unwrap_err().contains("voice id"));
}

#[test]
fn elevenlabs_lists_voices() {
    let body = br#"{"voices":[{"voice_id":"abc","name":"Rachel","labels":{"accent":"american","gender":"female"}},{"voice_id":"def","name":"Adam"}]}"#.to_vec();
    let (url, rx) = mock_server(200, "application/json", body);
    let el = ElevenLabs { key: "k".into(), model: "m".into(), base_url: url };
    let v = el.voices().unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!((v[0].id.as_str(), v[0].name.as_str()), ("abc", "Rachel"));
    assert!(rx.recv().unwrap().starts_with("GET /v1/voices"));
}

#[test]
fn openai_compatible_server_returns_wav() {
    let (url, rx) = mock_server(200, "audio/wav", wav_bytes(0.8));
    let t = OpenAiTts { base_url: url, key: Some("local".into()), model: "kokoro".into(), default_voice: "af_bella".into() };
    let mut r = req("Hello there");
    r.voice.clear();
    let a = t.synthesize(&r).unwrap();
    assert!((a.duration() - 0.8).abs() < 0.02);
    let raw = rx.recv().unwrap();
    assert!(raw.starts_with("POST /audio/speech") && raw.to_lowercase().contains("authorization: bearer local"));
    let body: serde_json::Value = serde_json::from_str(&raw[raw.find("\r\n\r\n").unwrap() + 4..]).unwrap();
    assert_eq!((body["model"].as_str(), body["voice"].as_str(), body["input"].as_str()), (Some("kokoro"), Some("af_bella"), Some("Hello there")));
}

#[test]
fn command_template_splitting_keeps_quotes() {
    assert_eq!(split_command(r#"piper --model "C:\My Voices\a.onnx" --output_file {out}"#), vec!["piper", "--model", r"C:\My Voices\a.onnx", "--output_file", "{out}"]);
    assert_eq!(split_command("  a   b  "), vec!["a", "b"]);
}

#[test]
fn custom_command_provider_runs_a_program_and_reads_its_wav() {
    let dir = std::env::temp_dir().join(format!("oneiro_cmd_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let fixture = dir.join("fixture.wav");
    let s: Vec<f32> = (0..SR as usize / 2).map(|i| (2.0 * std::f32::consts::PI * 300.0 * i as f32 / SR as f32).sin() * 0.5).collect();
    write_wav(&fixture, &s, SR).unwrap();
    let template = if cfg!(windows) { format!("cmd /C copy /Y \"{}\" {{out}}", fixture.display()) } else { format!("cp \"{}\" {{out}}", fixture.display()) };
    let cfg = VoiceConfig { command: template, ..Default::default() };
    let p = providers::build(ProviderKind::Command, &cfg, None).unwrap();
    let a = p.synthesize(&req("anything")).unwrap();
    assert!((a.duration() - 0.5).abs() < 0.01);
    // Errors: missing {out}, unknown program.
    assert!(providers::build(ProviderKind::Command, &VoiceConfig { command: "foo {text}".into(), ..Default::default() }, None).unwrap().synthesize(&req("x")).unwrap_err().contains("{out}"));
    assert!(providers::build(ProviderKind::Command, &VoiceConfig { command: "definitely-not-a-program {out}".into(), ..Default::default() }, None).unwrap().synthesize(&req("x")).unwrap_err().contains("cannot run"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn windows_speech_really_speaks() {
    if !cfg!(windows) {
        return;
    }
    let sapi = providers::build(ProviderKind::Sapi, &VoiceConfig::default(), None).unwrap();
    let voices = match sapi.voices() {
        Ok(v) if !v.is_empty() => v,
        other => {
            eprintln!("no Windows voices available ({:?}): skipped", other.map(|v| v.len()));
            return;
        }
    };
    eprintln!("installed voices: {:?}", voices.iter().map(|v| v.name.clone()).collect::<Vec<_>>());
    let mut r = req("Where is my pie? It has been an hour.");
    r.voice = voices[0].id.clone();
    let a: Audio = sapi.synthesize(&r).unwrap();
    assert!(a.duration() > 1.0 && a.duration() < 8.0, "{}", a.duration());
    assert!(dsp::peak(&a.samples) > 0.1);
    // Faster speed gives a shorter clip.
    let mut fast = r.clone();
    fast.speed = 1.6;
    assert!(sapi.synthesize(&fast).unwrap().duration() < a.duration());
}

#[test]
fn config_round_trips_and_never_contains_keys() {
    let c = VoiceConfig { default_provider: ProviderKind::ElevenLabs, command: "piper --output_file {out}".into(), openai_model: "kokoro".into(), ..Default::default() };
    let t = c.to_text();
    assert!(t.contains("elevenlabs_key_env = ELEVENLABS_API_KEY") && !t.contains("sk_"));
    assert_eq!(VoiceConfig::parse(&t), c);
    assert_eq!(ProviderKind::from_id("ElevenLabs"), Some(ProviderKind::ElevenLabs));
    assert!(providers::build(ProviderKind::File, &c, None).is_err());
    assert!(providers::build(ProviderKind::ElevenLabs, &VoiceConfig { elevenlabs_key_env: "ONEIRO_TEST_NO_SUCH_KEY".into(), ..c.clone() }, None).is_err());
    assert!(providers::build(ProviderKind::ElevenLabs, &VoiceConfig { elevenlabs_key_env: "ONEIRO_TEST_NO_SUCH_KEY".into(), ..c }, Some("typed")).is_ok());
}

#[test]
fn cloning_a_voice_uploads_the_recording_as_multipart_and_returns_the_id() {
    use oneiro_voice::providers::ElevenLabs;
    let dir = std::env::temp_dir().join(format!("oneiro_clone_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("me.wav");
    std::fs::write(&wav, vec![7u8; 3000]).unwrap();
    let (url, rx) = mock_server(200, "application/json", br#"{"voice_id":"abc123"}"#.to_vec());
    let el = ElevenLabs { key: "k".into(), model: "m".into(), base_url: url };
    assert_eq!(el.clone_voice("Mi voz", &[wav.clone()]).unwrap(), "abc123");
    let req = rx.recv().unwrap();
    assert!(req.contains("/v1/voices/add") && req.to_lowercase().contains("multipart/form-data"), "{}", &req[..req.len().min(300)]);
    assert!(req.contains("name=\"name\"") && req.contains("Mi voz") && req.contains("filename=\"me.wav\""));
    assert!(el.clone_voice("", &[wav.clone()]).is_err() && el.clone_voice("x", &[]).is_err());
    let (url, _rx) = mock_server(401, "application/json", br#"{"detail":{"status":"invalid_api_key","message":"Invalid API key"}}"#.to_vec());
    let bad = ElevenLabs { key: "k".into(), model: "m".into(), base_url: url };
    assert!(bad.clone_voice("x", &[wav]).unwrap_err().contains("ElevenLabs"));
    let _ = std::fs::remove_dir_all(&dir);
}
