//! Text-to-speech engines behind one trait.

use crate::config::{ProviderKind, VoiceConfig};
use oneiro_audio::{import, synth, Audio, SR};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct VoiceRequest {
    pub text: String,
    /// Provider-specific voice (id or name). Empty: the provider's default.
    pub voice: String,
    pub emotion: String,
    /// Base pitch in Hz (only the demo voice uses it; real engines pick the pitch with the voice).
    pub pitch: f64,
    /// Speaking speed multiplier (1.0 = normal).
    pub speed: f64,
    pub seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceInfo {
    pub id: String,
    pub name: String,
    pub detail: String,
}

pub trait VoiceProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    fn synthesize(&self, req: &VoiceRequest) -> Result<Audio, String>;
    /// Voices the engine offers (for the voice picker and `oneiro voices`).
    fn voices(&self) -> Result<Vec<VoiceInfo>, String> {
        Ok(vec![])
    }
}

/// Builds the provider for `kind`. `session_key` (typed in the IDE) overrides the environment variable.
pub fn build(kind: ProviderKind, cfg: &VoiceConfig, session_key: Option<&str>) -> Result<Box<dyn VoiceProvider>, String> {
    let env_key = {
        let n = cfg.key_env(kind);
        if n.is_empty() {
            None
        } else {
            std::env::var(n).ok().filter(|k| !k.is_empty())
        }
    };
    let key = session_key.filter(|k| !k.is_empty()).map(|k| k.to_string()).or(env_key);
    Ok(match kind {
        ProviderKind::File => return Err("this voice uses recorded files; pick a synthesis provider (demo, sapi, elevenlabs, openai or command) to generate it".into()),
        ProviderKind::Demo => Box::new(Demo),
        ProviderKind::Sapi => Box::new(Sapi { default_voice: cfg.sapi_voice.clone() }),
        ProviderKind::ElevenLabs => {
            let key = key.ok_or_else(|| format!("no ElevenLabs key: set the {} environment variable or paste a key for this session", cfg.elevenlabs_key_env))?;
            Box::new(ElevenLabs { key, model: cfg.elevenlabs_model.clone(), base_url: "https://api.elevenlabs.io".into() })
        }
        ProviderKind::OpenAi => {
            if cfg.openai_base_url.is_empty() {
                return Err("set the base URL of the TTS server (for example http://localhost:8880/v1)".into());
            }
            Box::new(OpenAiTts { base_url: cfg.openai_base_url.trim_end_matches('/').to_string(), key, model: cfg.openai_model.clone(), default_voice: cfg.openai_voice.clone() })
        }
        ProviderKind::Command => {
            if cfg.command.trim().is_empty() {
                return Err("write the command template, for example: piper --model voice.onnx --output_file {out}".into());
            }
            Box::new(CommandTts { template: cfg.command.clone() })
        }
    })
}

fn agent(secs: u64) -> ureq::Agent {
    ureq::Agent::config_builder().timeout_global(Some(Duration::from_secs(secs))).http_status_as_error(false).build().into()
}

fn http_error(status: u16, body: &[u8]) -> String {
    let text = String::from_utf8_lossy(body);
    let detail = serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|v| {
            v.pointer("/detail/message")
                .or_else(|| v.pointer("/detail"))
                .or_else(|| v.pointer("/error/message"))
                .or_else(|| v.pointer("/error"))
                .or_else(|| v.pointer("/message"))
                .and_then(|m| m.as_str().map(|s| s.to_string()))
        })
        .unwrap_or_else(|| text.chars().take(300).collect());
    let hint = match status {
        401 | 403 => " (check the API key)",
        429 => " (rate limit or quota reached)",
        _ => "",
    };
    format!("HTTP {}: {}{}", status, detail, hint)
}

// ------------------------------------------------------------------ demo

pub struct Demo;

impl VoiceProvider for Demo {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Demo
    }
    fn synthesize(&self, req: &VoiceRequest) -> Result<Audio, String> {
        let samples = synth::synth_speech(&req.text, req.pitch as f32, SR, req.seed);
        let a = Audio { sr: SR, samples };
        Ok(if (req.speed - 1.0).abs() > 0.01 { oneiro_audio::dsp::speed(&a, req.speed) } else { a })
    }
}

// ------------------------------------------------------------------ Windows speech

pub struct Sapi {
    pub default_voice: String,
}

const SAPI_SCRIPT: &str = r#"
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Speech
$s = New-Object System.Speech.Synthesis.SpeechSynthesizer
if ($env:ONEIRO_LIST) {
  $s.GetInstalledVoices() | Where-Object { $_.Enabled } | ForEach-Object { $_.VoiceInfo.Name + '|' + $_.VoiceInfo.Culture.Name + '|' + $_.VoiceInfo.Gender }
  exit 0
}
if ($env:ONEIRO_VOICE) { $s.SelectVoice($env:ONEIRO_VOICE) }
$s.Rate = [int]$env:ONEIRO_RATE
$s.SetOutputToWaveFile($env:ONEIRO_OUT)
$s.Speak($env:ONEIRO_TEXT)
$s.Dispose()
"#;

impl Sapi {
    fn run(&self, envs: &[(&str, String)]) -> Result<std::process::Output, String> {
        if !cfg!(windows) {
            return Err("Windows speech is only available on Windows; use elevenlabs, openai or command here".into());
        }
        let dir = std::env::temp_dir().join("oneiro-voice");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let script = dir.join("sapi.ps1");
        std::fs::write(&script, SAPI_SCRIPT).map_err(|e| e.to_string())?;
        let mut cmd = Command::new("powershell.exe");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000);
        }
        cmd.args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File"]).arg(&script);
        for (k, v) in envs {
            cmd.env(k, v);
        }
        let out = cmd.stdin(Stdio::null()).output().map_err(|e| format!("cannot run PowerShell: {}", e))?;
        if !out.status.success() {
            let err = String::from_utf8_lossy(&out.stderr);
            let msg = err.lines().find(|l| !l.trim().is_empty()).unwrap_or("unknown error").trim().to_string();
            return Err(format!("Windows speech failed: {}", msg));
        }
        Ok(out)
    }
}

impl VoiceProvider for Sapi {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Sapi
    }

    fn synthesize(&self, req: &VoiceRequest) -> Result<Audio, String> {
        let out = std::env::temp_dir().join("oneiro-voice").join(format!("sapi_{}_{}.wav", std::process::id(), oneiro_audio::unique_id()));
        std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
        let voice = if req.voice.is_empty() { self.default_voice.clone() } else { req.voice.clone() };
        let rate = ((req.speed - 1.0) * 10.0).round().clamp(-10.0, 10.0) as i32;
        let r = self.run(&[("ONEIRO_TEXT", req.text.clone()), ("ONEIRO_OUT", out.display().to_string()), ("ONEIRO_VOICE", voice), ("ONEIRO_RATE", rate.to_string())]);
        let a = r.and_then(|_| oneiro_audio::read_wav(&out));
        let _ = std::fs::remove_file(&out);
        a
    }

    fn voices(&self) -> Result<Vec<VoiceInfo>, String> {
        let out = self.run(&[("ONEIRO_LIST", "1".into())])?;
        Ok(String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|l| {
                let mut p = l.trim().split('|');
                let name = p.next()?.to_string();
                if name.is_empty() {
                    return None;
                }
                Some(VoiceInfo { id: name.clone(), name, detail: format!("{} {}", p.next().unwrap_or(""), p.next().unwrap_or("")).trim().to_string() })
            })
            .collect())
    }
}

// ------------------------------------------------------------------ ElevenLabs

pub struct ElevenLabs {
    pub key: String,
    pub model: String,
    pub base_url: String,
}

/// (stability, style) for an emotion: lively emotions get a less stable, more expressive delivery.
pub fn eleven_settings(emotion: &str) -> (f64, f64) {
    match emotion {
        "angry" | "furious" | "annoyed" => (0.30, 0.60),
        "happy" | "excited" | "laughing" | "proud" => (0.35, 0.50),
        "surprised" | "scared" | "nervous" | "worried" => (0.30, 0.35),
        "sad" | "tired" | "bored" => (0.65, 0.25),
        _ => (0.50, 0.0),
    }
}

impl ElevenLabs {
    pub fn request_body(&self, req: &VoiceRequest) -> Value {
        let (stability, style) = eleven_settings(&req.emotion);
        json!({
            "text": req.text,
            "model_id": self.model,
            "voice_settings": {
                "stability": stability,
                "similarity_boost": 0.75,
                "style": style,
                "speed": req.speed.clamp(0.7, 1.2),
            }
        })
    }
}

impl VoiceProvider for ElevenLabs {
    fn kind(&self) -> ProviderKind {
        ProviderKind::ElevenLabs
    }

    fn synthesize(&self, req: &VoiceRequest) -> Result<Audio, String> {
        if req.voice.is_empty() {
            return Err("this character has no ElevenLabs voice: add `voice: \"<voice id>\"` to its `voice { ... }` block (list them with `oneiro voices elevenlabs`)".into());
        }
        let url = format!("{}/v1/text-to-speech/{}?output_format=pcm_24000", self.base_url, req.voice);
        let mut resp = agent(120)
            .post(&url)
            .header("xi-api-key", self.key.as_str())
            .header("content-type", "application/json")
            .header("accept", "audio/pcm")
            .send(self.request_body(req).to_string())
            .map_err(|e| format!("could not reach ElevenLabs: {}", e))?;
        let status = resp.status().as_u16();
        let bytes = resp.body_mut().read_to_vec().map_err(|e| format!("could not read the ElevenLabs response: {}", e))?;
        if status != 200 {
            return Err(format!("ElevenLabs {}", http_error(status, &bytes)));
        }
        if bytes.len() < 200 {
            return Err("ElevenLabs returned no audio".into());
        }
        Ok(import::from_pcm16(&bytes, 24_000))
    }

    fn voices(&self) -> Result<Vec<VoiceInfo>, String> {
        let url = format!("{}/v1/voices", self.base_url);
        let mut resp = agent(30).get(&url).header("xi-api-key", self.key.as_str()).call().map_err(|e| format!("could not reach ElevenLabs: {}", e))?;
        let status = resp.status().as_u16();
        let bytes = resp.body_mut().read_to_vec().map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(format!("ElevenLabs {}", http_error(status, &bytes)));
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON from ElevenLabs: {}", e))?;
        Ok(v.get("voices")
            .and_then(|a| a.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| {
                        let labels = x.get("labels").and_then(|l| l.as_object()).map(|o| o.values().filter_map(|s| s.as_str()).collect::<Vec<_>>().join(", ")).unwrap_or_default();
                        Some(VoiceInfo { id: x.get("voice_id")?.as_str()?.to_string(), name: x.get("name")?.as_str()?.to_string(), detail: labels })
                    })
                    .collect()
            })
            .unwrap_or_default())
    }
}

// ------------------------------------------------------------------ OpenAI-compatible TTS

pub struct OpenAiTts {
    pub base_url: String,
    pub key: Option<String>,
    pub model: String,
    pub default_voice: String,
}

impl VoiceProvider for OpenAiTts {
    fn kind(&self) -> ProviderKind {
        ProviderKind::OpenAi
    }

    fn synthesize(&self, req: &VoiceRequest) -> Result<Audio, String> {
        let voice = if req.voice.is_empty() { self.default_voice.clone() } else { req.voice.clone() };
        let url = format!("{}/audio/speech", self.base_url);
        let body = json!({"model": self.model, "input": req.text, "voice": voice, "response_format": "wav", "speed": req.speed.clamp(0.25, 4.0)});
        let mut r = agent(180).post(&url).header("content-type", "application/json");
        if let Some(k) = &self.key {
            r = r.header("authorization", format!("Bearer {}", k).as_str());
        }
        let mut resp = r.send(body.to_string()).map_err(|e| format!("could not reach {}: {}", url, e))?;
        let status = resp.status().as_u16();
        let bytes = resp.body_mut().read_to_vec().map_err(|e| format!("could not read the response from {}: {}", url, e))?;
        if !(200..300).contains(&status) {
            return Err(http_error(status, &bytes));
        }
        import::decode_bytes(&bytes, "wav")
    }

    fn voices(&self) -> Result<Vec<VoiceInfo>, String> {
        // Not part of the OpenAI API, but Kokoro-FastAPI and others expose it.
        let url = format!("{}/audio/voices", self.base_url);
        let mut resp = agent(15).get(&url).call().map_err(|e| format!("could not reach {}: {}", url, e))?;
        let status = resp.status().as_u16();
        let bytes = resp.body_mut().read_to_vec().map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(format!("this server does not list its voices ({})", http_error(status, &bytes)));
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        let arr = v.get("voices").and_then(|a| a.as_array()).cloned().unwrap_or_default();
        Ok(arr.iter().filter_map(|x| x.as_str().map(|s| VoiceInfo { id: s.to_string(), name: s.to_string(), detail: String::new() })).collect())
    }
}

// ------------------------------------------------------------------ custom command

pub struct CommandTts {
    pub template: String,
}

/// Splits a command line on spaces, keeping double-quoted parts together.
pub fn split_command(s: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in s.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any || !cur.is_empty() {
                    out.push(std::mem::take(&mut cur));
                    any = false;
                }
            }
            c => cur.push(c),
        }
    }
    if any || !cur.is_empty() {
        out.push(cur);
    }
    out
}

impl VoiceProvider for CommandTts {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Command
    }

    fn synthesize(&self, req: &VoiceRequest) -> Result<Audio, String> {
        if !self.template.contains("{out}") {
            return Err("the command must contain {out}, the WAV file it has to write".into());
        }
        let out: PathBuf = std::env::temp_dir().join("oneiro-voice").join(format!("cmd_{}_{}.wav", std::process::id(), oneiro_audio::unique_id()));
        std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
        let text_in_args = self.template.contains("{text}");
        let parts: Vec<String> = split_command(&self.template)
            .into_iter()
            .map(|p| p.replace("{text}", &req.text).replace("{out}", &out.display().to_string()).replace("{voice}", &req.voice).replace("{speed}", &format!("{}", req.speed)))
            .collect();
        let (program, args) = parts.split_first().ok_or("empty command")?;
        let mut cmdp = Command::new(program);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmdp.creation_flags(0x0800_0000);
        }
        let mut child = cmdp
            .args(args)
            .stdin(if text_in_args { Stdio::null() } else { Stdio::piped() })
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run '{}': {}", program, e))?;
        if !text_in_args {
            use std::io::Write;
            if let Some(mut si) = child.stdin.take() {
                let _ = si.write_all(req.text.as_bytes());
            }
        }
        let output = child.wait_with_output().map_err(|e| e.to_string())?;
        if !output.status.success() {
            let _ = std::fs::remove_file(&out);
            return Err(format!("'{}' failed: {}", program, String::from_utf8_lossy(&output.stderr).trim().chars().take(300).collect::<String>()));
        }
        let a = import::read_any(&out);
        let _ = std::fs::remove_file(&out);
        a.map_err(|e| format!("the command did not produce a readable file at {{out}}: {}", e))
    }
}

// ------------------------------------------------------------------ voice cloning (ElevenLabs)

/// A multipart/form-data body: text fields, then audio files.
pub fn multipart_body(boundary: &str, fields: &[(&str, &str)], files: &[(&str, String, Vec<u8>)]) -> Vec<u8> {
    let mut b: Vec<u8> = Vec::new();
    for (k, v) in fields {
        b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"\r\n\r\n{v}\r\n").as_bytes());
    }
    for (k, name, data) in files {
        b.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"{k}\"; filename=\"{name}\"\r\nContent-Type: application/octet-stream\r\n\r\n").as_bytes());
        b.extend_from_slice(data);
        b.extend_from_slice(b"\r\n");
    }
    b.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());
    b
}

impl ElevenLabs {
    /// Instant voice cloning: uploads one or more recordings of the same person and returns the new voice id.
    /// Needs an ElevenLabs plan that includes voice cloning, and a voice you have the right to clone.
    pub fn clone_voice(&self, name: &str, files: &[std::path::PathBuf]) -> Result<String, String> {
        if name.trim().is_empty() {
            return Err("give the voice a name".into());
        }
        if files.is_empty() {
            return Err("choose at least one recording (a minute or more of clean speech works best)".into());
        }
        let mut parts = vec![];
        for f in files {
            let data = std::fs::read(f).map_err(|e| format!("{}: {}", f.display(), e))?;
            if data.len() > 10_000_000 {
                return Err(format!("{} is larger than 10 MB; use a shorter clip", f.display()));
            }
            parts.push(("files", f.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "sample.wav".into()), data));
        }
        let boundary = format!("oneiro-{:016x}", oneiro_core::math::hash_str(name) ^ parts.len() as u64);
        let body = multipart_body(&boundary, &[("name", name.trim())], &parts);
        let url = format!("{}/v1/voices/add", self.base_url);
        let mut resp = agent(180)
            .post(&url)
            .header("xi-api-key", self.key.as_str())
            .header("content-type", &format!("multipart/form-data; boundary={}", boundary))
            .send(&body[..])
            .map_err(|e| format!("could not reach ElevenLabs: {}", e))?;
        let status = resp.status().as_u16();
        let bytes = resp.body_mut().read_to_vec().map_err(|e| e.to_string())?;
        if status != 200 {
            return Err(format!("ElevenLabs {}", http_error(status, &bytes)));
        }
        let v: Value = serde_json::from_slice(&bytes).map_err(|e| format!("invalid JSON from ElevenLabs: {}", e))?;
        v.get("voice_id").and_then(|x| x.as_str()).map(|s| s.to_string()).ok_or_else(|| "ElevenLabs answered without a voice id".into())
    }
}

/// Clones a voice with the configured ElevenLabs account.
pub fn clone_voice(cfg: &crate::VoiceConfig, key: Option<&str>, name: &str, files: &[std::path::PathBuf]) -> Result<String, String> {
    let key = key.map(|k| k.to_string()).or_else(|| std::env::var(&cfg.elevenlabs_key_env).ok().filter(|k| !k.is_empty())).ok_or_else(|| format!("no ElevenLabs key: set the {} environment variable or paste a key in the voice settings", cfg.elevenlabs_key_env))?;
    ElevenLabs { key, model: cfg.elevenlabs_model.clone(), base_url: "https://api.elevenlabs.io".into() }.clone_voice(name, files)
}
