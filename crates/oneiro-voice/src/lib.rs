//! Voices for Oneiro: text-to-speech providers (ElevenLabs, Windows speech, OpenAI-compatible
//! servers, custom commands, demo babble) and the planner that finds the dialogue lines of a
//! script that have no audio yet, synthesizes them and writes the `audio "..."` clause back.
//!
//! Lipsync is never part of this: it is always computed later from the final audio file.

pub mod config;
pub mod providers;

pub use config::{ProviderKind, VoiceConfig};
pub use providers::{build, VoiceInfo, VoiceProvider, VoiceRequest};

use oneiro_audio::{dsp, import, write_wav, Audio, SR};
use oneiro_core::Project;
use oneiro_lang::{parse, Stmt, StmtKind};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

/// A dialogue line found in a script.
#[derive(Clone, Debug, PartialEq)]
pub struct DialogueLine {
    /// 1-based line in the script.
    pub line: u32,
    pub who: String,
    pub emotion: String,
    pub text: String,
    /// The path in the `audio "..."` clause, if any.
    pub audio: Option<String>,
}

pub fn dialogues(src: &str) -> Vec<DialogueLine> {
    fn walk(stmts: &[Stmt], out: &mut Vec<DialogueLine>) {
        for s in stmts {
            match &s.kind {
                StmtKind::Dialogue(d) => out.push(DialogueLine { line: s.pos.line, who: d.who.clone(), emotion: d.emotion.clone(), text: d.text.clone(), audio: d.audio.clone() }),
                StmtKind::Together(v) => walk(v, out),
                _ => {}
            }
        }
    }
    let (file, _) = parse(src, "script");
    let mut out = Vec::new();
    for sc in &file.scenes {
        walk(&sc.stmts, &mut out);
    }
    out
}

/// Sets (or replaces) the `audio "path"` clause on a dialogue line. Returns `None` if the line
/// does not look like a dialogue line.
pub fn set_audio_clause(line: &str, rel: &str) -> Option<String> {
    let colon = line.find("):")?;
    let after = &line[colon + 2..];
    let q = after.find('"')?;
    // End of the spoken text (the lexer allows \" escapes).
    let b: Vec<char> = after[q + 1..].chars().collect();
    let (mut i, mut closed) = (0, false);
    while i < b.len() {
        match b[i] {
            '\\' => i += 2,
            '"' => {
                closed = true;
                break;
            }
            _ => i += 1,
        }
    }
    if !closed {
        return None;
    }
    let text_end = colon + 2 + q + 1 + b[..i].iter().map(|c| c.len_utf8()).sum::<usize>() + 1;
    let head = &line[..text_end];
    let rest = &line[text_end..];
    let trimmed = rest.trim_start();
    let tail = if let Some(r) = trimmed.strip_prefix("audio") {
        // Replace the existing path.
        let r = r.trim_start();
        match r.strip_prefix('"').and_then(|s| s.find('"').map(|e| &s[e + 1..])) {
            Some(after_path) => after_path,
            None => return None,
        }
    } else {
        rest
    };
    Some(format!("{} audio \"{}\"{}", head, rel, tail))
}

/// A fresh `audio/<who>_NN.wav` that neither exists on disk nor appears in the script.
pub fn next_audio_path(root: &Path, who: &str, taken: &mut HashSet<String>) -> String {
    let base = who.to_lowercase();
    let mut n = 1;
    loop {
        let rel = format!("audio/{}_{:02}.wav", base, n);
        if !taken.contains(&rel) && !root.join(&rel).exists() {
            taken.insert(rel.clone());
            return rel;
        }
        n += 1;
    }
}

/// Which provider and request a character's line uses.
pub fn request_for(project: &Project, cfg: &VoiceConfig, who: &str, emotion: &str, text: &str, force: Option<ProviderKind>) -> (ProviderKind, VoiceRequest) {
    let c = project.characters.get(who);
    let kind = force.unwrap_or_else(|| match c.and_then(|c| ProviderKind::from_id(&c.voice_provider)) {
        Some(ProviderKind::File) | None => cfg.default_provider,
        Some(k) => k,
    });
    let kind = if kind == ProviderKind::File { ProviderKind::Demo } else { kind };
    let seed = oneiro_core::math::hash_str(&format!("{}:{}", who, text));
    (
        kind,
        VoiceRequest {
            text: text.to_string(),
            voice: c.map(|c| c.voice_id.clone()).unwrap_or_default(),
            emotion: emotion.to_string(),
            pitch: c.map_or(160.0, |c| c.voice_pitch),
            speed: c.map_or(1.0, |c| c.voice_speed),
            seed,
        },
    )
}

/// Synthesizes with `provider`, cleans the result (rumble, silence, level) and writes it to `dest`.
/// Returns the duration in seconds.
pub fn synthesize_to(provider: &dyn VoiceProvider, req: &VoiceRequest, dest: &Path) -> Result<f64, String> {
    if req.text.trim().is_empty() {
        return Err("the line has no text".into());
    }
    let raw = provider.synthesize(req)?;
    if raw.samples.is_empty() {
        return Err("the provider returned empty audio".into());
    }
    let clean: Audio = import::process(&raw, import::ImportKind::Voice);
    if clean.samples.is_empty() || dsp::peak(&clean.samples) < 1e-4 {
        return Err("the provider returned silence".into());
    }
    write_wav(dest, &clean.samples, SR)?;
    Ok(clean.duration())
}

#[derive(Clone, Debug, Default)]
pub struct GenOptions {
    /// Regenerate lines even when their audio file already exists.
    pub force: bool,
    /// Use this provider for every line instead of the characters' own.
    pub provider: Option<ProviderKind>,
    /// Only these characters (empty: all).
    pub only: Vec<String>,
    /// API key typed in the IDE (overrides the environment variable).
    pub session_key: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LineReport {
    pub line: u32,
    pub who: String,
    pub text: String,
    pub path: String,
    pub provider: ProviderKind,
    pub seconds: f64,
    pub error: Option<String>,
    /// The line was left alone (audio already exists).
    pub skipped: bool,
}

/// Generates the missing voices of a script. Returns the script text with `audio` clauses filled in
/// and one report per dialogue line. `progress(done, total, label)` may return `false` to stop.
pub fn generate_script(
    root: &Path,
    project: &Project,
    src: &str,
    cfg: &VoiceConfig,
    opts: &GenOptions,
    progress: &mut dyn FnMut(usize, usize, &str) -> bool,
) -> Result<(String, Vec<LineReport>), String> {
    let lines = dialogues(src);
    let mut taken: HashSet<String> = lines.iter().filter_map(|l| l.audio.clone()).collect();
    let mut providers: BTreeMap<&'static str, Result<Box<dyn VoiceProvider>, String>> = BTreeMap::new();
    let mut text_lines: Vec<String> = src.split_inclusive('\n').map(|s| s.to_string()).collect();
    let mut reports = Vec::new();
    let todo: Vec<&DialogueLine> = lines.iter().filter(|l| opts.only.is_empty() || opts.only.iter().any(|o| *o == l.who)).collect();
    let total = todo.len();

    for (n, l) in todo.iter().enumerate() {
        let (kind, req) = request_for(project, cfg, &l.who, &l.emotion, &l.text, opts.provider);
        let existing = l.audio.as_ref().filter(|p| root.join(p).exists());
        let mut rep = LineReport { line: l.line, who: l.who.clone(), text: l.text.clone(), path: l.audio.clone().unwrap_or_default(), provider: kind, seconds: 0.0, error: None, skipped: false };
        if existing.is_some() && !opts.force {
            rep.skipped = true;
            reports.push(rep);
            if !progress(n + 1, total, "") {
                break;
            }
            continue;
        }
        if !progress(n, total, &format!("{}: {}", l.who, l.text)) {
            break;
        }
        let rel = l.audio.clone().unwrap_or_else(|| next_audio_path(root, &l.who, &mut taken));
        rep.path = rel.clone();
        let provider = providers.entry(kind.id()).or_insert_with(|| build(kind, cfg, opts.session_key.as_deref()));
        match provider {
            Err(e) => rep.error = Some(e.clone()),
            Ok(p) => match synthesize_to(p.as_ref(), &req, &root.join(&rel)) {
                Ok(secs) => {
                    rep.seconds = secs;
                    if l.audio.as_deref() != Some(rel.as_str()) {
                        if let Some(idx) = (l.line as usize).checked_sub(1).filter(|i| *i < text_lines.len()) {
                            let (body, eol) = match text_lines[idx].strip_suffix("\r\n") {
                                Some(b) => (b.to_string(), "\r\n"),
                                None => match text_lines[idx].strip_suffix('\n') {
                                    Some(b) => (b.to_string(), "\n"),
                                    None => (text_lines[idx].clone(), ""),
                                },
                            };
                            match set_audio_clause(&body, &rel) {
                                Some(new) => text_lines[idx] = format!("{}{}", new, eol),
                                None => rep.error = Some(format!("voice saved at {} but the script line {} could not be updated; add: audio \"{}\"", rel, l.line, rel)),
                            }
                        }
                    }
                }
                Err(e) => rep.error = Some(e),
            },
        }
        reports.push(rep);
    }
    progress(total, total, "done");
    Ok((text_lines.concat(), reports))
}
