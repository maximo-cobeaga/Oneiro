//! Phase 4 commands: `voice`, `voices`, `sound`, `import`, `mix`.

use oneiro_audio::import::{self, ImportKind};
use oneiro_audio::musicgen::{self, Mood, MusicSpec};
use oneiro_audio::{dsp, sfxgen, write_wav, SR};
use oneiro_core::lock::Lock;
use oneiro_core::project::locate;
use oneiro_core::{compile, Project};
use oneiro_voice::{GenOptions, ProviderKind, VoiceConfig};
use std::path::{Path, PathBuf};

/// Splits arguments into positionals and `--flag value` pairs. `bool_flags` take no value.
struct Args {
    pos: Vec<String>,
    flags: Vec<(String, String)>,
}

impl Args {
    fn parse(args: &[String], bool_flags: &[&str]) -> Result<Args, String> {
        let mut a = Args { pos: vec![], flags: vec![] };
        let mut i = 0;
        while i < args.len() {
            let s = &args[i];
            if s == "-o" || s.starts_with("--") {
                if bool_flags.contains(&s.as_str()) {
                    a.flags.push((s.clone(), String::new()));
                } else {
                    i += 1;
                    let v = args.get(i).ok_or(format!("{} needs a value", s))?;
                    a.flags.push((s.clone(), v.clone()));
                }
            } else {
                a.pos.push(s.clone());
            }
            i += 1;
        }
        Ok(a)
    }
    fn get(&self, name: &str) -> Option<&str> {
        self.flags.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str())
    }
    fn has(&self, name: &str) -> bool {
        self.flags.iter().any(|(k, _)| k == name)
    }
    fn num<T: std::str::FromStr>(&self, name: &str) -> Result<Option<T>, String> {
        match self.get(name) {
            None => Ok(None),
            Some(v) => v.parse().map(Some).map_err(|_| format!("{} must be a number", name)),
        }
    }
}

fn project_of(path: &str) -> Result<(PathBuf, PathBuf, Project), String> {
    let (root, script) = locate(Path::new(path))?;
    let mut diags = Vec::new();
    let project = Project::load(&root, &mut diags);
    Ok((root, script, project))
}

fn safe_name(name: &str) -> Result<String, String> {
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        return Err(format!("'{}' is not a valid sound name: use letters, digits, '_' and '-'", name));
    }
    Ok(name.to_string())
}

/// `oneiro voice <project> [--provider P] [--force] [--only A,B] [--key-env VAR] [--dry-run]`
pub fn cmd_voice(args: &[String]) -> Result<bool, String> {
    let a = Args::parse(args, &["--force", "--dry-run"])?;
    let (root, script, project) = project_of(a.pos.first().map(|s| s.as_str()).unwrap_or("."))?;
    let mut cfg = VoiceConfig::load();
    if let Some(v) = a.get("--key-env") {
        cfg.elevenlabs_key_env = v.to_string();
    }
    if let Some(v) = a.get("--command") {
        cfg.command = v.to_string();
    }
    let provider = match a.get("--provider") {
        Some(p) => Some(ProviderKind::from_id(p).ok_or(format!("unknown provider '{}' (use demo, sapi, elevenlabs, openai or command)", p))?),
        None => None,
    };
    let opts = GenOptions { force: a.has("--force"), provider, only: a.get("--only").map(|s| s.split(',').map(|x| x.trim().to_string()).collect()).unwrap_or_default(), session_key: None };
    let src = std::fs::read_to_string(&script).map_err(|e| format!("{}: {}", script.display(), e))?;

    if a.has("--dry-run") {
        for l in oneiro_voice::dialogues(&src) {
            let (kind, _) = oneiro_voice::request_for(&project, &cfg, &l.who, &l.emotion, &l.text, opts.provider);
            let has = l.audio.as_ref().map_or(false, |p| root.join(p).exists());
            let state = if has && !opts.force { "has audio" } else { "would generate" };
            eprintln!("  line {:>3}  {:<8} {:<14} [{}] {}", l.line, l.who, state, kind.id(), l.text);
        }
        return Ok(true);
    }

    let (out, reports) = oneiro_voice::generate_script(&root, &project, &src, &cfg, &opts, &mut |done, total, label| {
        if done < total && !label.is_empty() {
            eprintln!("  [{}/{}] {}", done + 1, total, label);
        }
        true
    })?;
    let (mut made, mut skipped, mut failed) = (0, 0, 0);
    for r in &reports {
        if r.skipped {
            skipped += 1;
        } else if let Some(e) = &r.error {
            failed += 1;
            eprintln!("  line {} ({}): {}", r.line, r.who, e);
        } else {
            made += 1;
            eprintln!("  line {} ({}) -> {} ({:.1}s, {})", r.line, r.who, r.path, r.seconds, r.provider.id());
        }
    }
    if out != src {
        std::fs::write(&script, &out).map_err(|e| format!("{}: {}", script.display(), e))?;
        eprintln!("updated {} with the new audio clauses", script.display());
    }
    eprintln!("{} generated, {} already had audio, {} failed", made, skipped, failed);
    Ok(failed == 0)
}

/// `oneiro voices [provider]`
pub fn cmd_voices(args: &[String]) -> Result<bool, String> {
    let a = Args::parse(args, &[])?;
    let cfg = VoiceConfig::load();
    let kind = match a.pos.first() {
        Some(p) => ProviderKind::from_id(p).ok_or(format!("unknown provider '{}'", p))?,
        None => cfg.default_provider,
    };
    let p = oneiro_voice::build(kind, &cfg, None)?;
    let voices = p.voices()?;
    if voices.is_empty() {
        eprintln!("{} did not list any voices", kind.label());
    }
    for v in voices {
        println!("{}\t{}\t{}", v.id, v.name, v.detail);
    }
    Ok(true)
}

/// `oneiro sound sfx <name> [project] [--as NAME] [--seed N]`, `sound sfx --list`,
/// `sound music <mood> [project] --as NAME [--bpm N] [--bars N] [--seed N] [--key N]`.
pub fn cmd_sound(args: &[String]) -> Result<bool, String> {
    let a = Args::parse(args, &["--list"])?;
    let what = a.pos.first().map(|s| s.as_str()).unwrap_or("");
    match what {
        "sfx" => {
            if a.has("--list") || a.pos.len() < 2 {
                for (n, d) in sfxgen::PRESETS {
                    println!("{:<12} {}", n, d);
                }
                return Ok(true);
            }
            let preset = a.pos[1].as_str();
            let seed = a.num::<u64>("--seed")?.unwrap_or(1);
            let audio = sfxgen::generate(preset, seed).ok_or(format!("unknown sound '{}'. Presets: {}", preset, sfxgen::names().join(", ")))?;
            let name = safe_name(a.get("--as").unwrap_or(preset))?;
            let (root, _, _) = project_of(a.pos.get(2).map(|s| s.as_str()).unwrap_or("."))?;
            let path = root.join("library").join("sounds").join(format!("{}.wav", name));
            write_wav(&path, &audio.samples, SR)?;
            eprintln!("wrote {} ({:.1}s). Use it with: sfx {}", path.display(), audio.duration(), name);
            Ok(true)
        }
        "music" => {
            let mood_id = a.pos.get(1).map(|s| s.as_str()).unwrap_or("");
            let mood = Mood::from_id(mood_id).ok_or(format!("choose a mood: {}", Mood::ALL.iter().map(|m| m.id()).collect::<Vec<_>>().join(", ")))?;
            let mut spec = MusicSpec::new(mood, a.num::<u64>("--seed")?.unwrap_or(1));
            if let Some(b) = a.num::<f32>("--bpm")? {
                spec.bpm = b.clamp(40.0, 220.0);
            }
            if let Some(b) = a.num::<usize>("--bars")? {
                spec.bars = b.clamp(1, 64);
            }
            if let Some(k) = a.num::<i32>("--key")? {
                spec.key = k;
            }
            let audio = musicgen::generate(&spec);
            let name = safe_name(a.get("--as").unwrap_or(&format!("{}_{}", mood.id(), spec.seed)))?;
            let (root, _, _) = project_of(a.pos.get(2).map(|s| s.as_str()).unwrap_or("."))?;
            let path = root.join("library").join("music").join(format!("{}.wav", name));
            write_wav(&path, &audio.samples, SR)?;
            eprintln!("wrote {} ({:.1}s loop). Use it with: music {}", path.display(), audio.duration(), name);
            Ok(true)
        }
        _ => Err("usage: oneiro sound sfx <preset|--list> [project] [--as NAME]  |  oneiro sound music <mood> [project] [--as NAME] [--bpm N] [--bars N] [--seed N] [--key N]".into()),
    }
}

/// `oneiro import <project> <file> --kind voice|sfx|music|raw --as NAME`
pub fn cmd_import(args: &[String]) -> Result<bool, String> {
    let a = Args::parse(args, &[])?;
    let (project, file) = match a.pos.as_slice() {
        [p, f] => (p.as_str(), f.as_str()),
        [f] => (".", f.as_str()),
        _ => return Err("usage: oneiro import <project> <file> --kind voice|sfx|music|raw [--as NAME]".into()),
    };
    let kind = match a.get("--kind").unwrap_or("sfx") {
        "voice" => ImportKind::Voice,
        "sfx" => ImportKind::Sfx,
        "music" => ImportKind::Music,
        "raw" => ImportKind::Raw,
        other => return Err(format!("unknown kind '{}' (voice, sfx, music or raw)", other)),
    };
    let src = Path::new(file);
    let stem = src.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "imported".into());
    let name = safe_name(&a.get("--as").map(|s| s.to_string()).unwrap_or_else(|| stem.replace(' ', "_")))?;
    let (root, _, _) = project_of(project)?;
    let folder = match kind {
        ImportKind::Voice => "audio",
        ImportKind::Music => "library/music",
        _ => "library/sounds",
    };
    let out = import::import_into(&root, src, &format!("{}/{}", folder, name), kind)?;
    let a2 = oneiro_audio::read_wav(&out)?;
    eprintln!("imported {} ({:.1}s, {:.1} dBFS)", out.display(), a2.duration(), dsp::loudness_db(&a2));
    match kind {
        ImportKind::Voice => eprintln!("use it on a line with: audio \"audio/{}.wav\"", name),
        ImportKind::Music => eprintln!("use it with: music {}", name),
        _ => eprintln!("use it with: sfx {}", name),
    }
    Ok(true)
}

/// `oneiro mix <project> [-o out.wav]` writes the final soundtrack and prints its levels.
pub fn cmd_mix(args: &[String]) -> Result<bool, String> {
    let a = Args::parse(args, &[])?;
    let (root, script, project) = project_of(a.pos.first().map(|s| s.as_str()).unwrap_or("."))?;
    let lock = Lock::load(&script.with_extension("onlock"));
    let (ep, diags) = compile(&project, &script, &lock);
    for d in diags.iter().filter(|d| d.is_error()) {
        eprintln!("{}", d.render());
    }
    if diags.iter().any(|d| d.is_error()) {
        return Ok(false);
    }
    let (samples, report) = ep.render_audio();
    let out = a.get("-o").map(PathBuf::from).unwrap_or_else(|| root.join("out").join("soundtrack.wav"));
    write_wav(&out, &samples, SR)?;
    let count = |k| ep.audio.iter().filter(|e| e.kind == k).count();
    eprintln!(
        "wrote {} ({:.1}s): {} voice(s), {} effect(s), {} music track(s)",
        out.display(),
        ep.duration,
        count(oneiro_core::AudioKind::Voice),
        count(oneiro_core::AudioKind::Sfx),
        count(oneiro_core::AudioKind::Music)
    );
    eprintln!("peak {:.1} dBFS, loudness {:.1} dBFS{}", report.peak_db, report.loudness_db, if report.limited { " (the limiter had to work: lower some volumes)" } else { "" });
    if report.loudness_db < -32.0 {
        eprintln!("warning: the soundtrack is very quiet");
    }
    Ok(true)
}
