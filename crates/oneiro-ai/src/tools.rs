//! The engine's tools for agents: validate and write scripts, look at frames, list the library,
//! create characters, render video. Used by the MCP server (so Claude Code, Codex or any MCP client
//! can drive Oneiro) and by the CLI. Every call reads the project fresh from disk, so edits made in
//! the IDE and edits made by an agent never diverge.

use crate::director::validate_character;
use crate::guide::{self, Mode};
use oneiro_core::lock::Lock;
use oneiro_core::project::locate;
use oneiro_core::{compile_source, Episode, Project};
use oneiro_lang::{to_json_array, Diagnostic};
use oneiro_render::encode::{encode_episode, EncodeOptions};
use oneiro_render::Renderer;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub struct Toolbox {
    pub root: PathBuf,
    pub script: PathBuf,
}

pub struct ToolOutput {
    pub text: String,
    pub images: Vec<Vec<u8>>,
    pub is_error: bool,
}

impl ToolOutput {
    fn ok(text: impl Into<String>) -> ToolOutput {
        ToolOutput { text: text.into(), images: vec![], is_error: false }
    }
    fn err(text: impl Into<String>) -> ToolOutput {
        ToolOutput { text: text.into(), images: vec![], is_error: true }
    }
}

pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

struct Loaded {
    project: Project,
    lock: Lock,
    label: String,
}

fn count_errors(d: &[Diagnostic]) -> usize {
    d.iter().filter(|x| x.is_error()).count()
}

impl Toolbox {
    pub fn open(path: &Path) -> Result<Toolbox, String> {
        let (root, script) = locate(path)?;
        Ok(Toolbox { root, script })
    }

    fn load(&self) -> Loaded {
        let mut d = Vec::new();
        let project = Project::load(&self.root, &mut d);
        let lock = Lock::load(&self.script.with_extension("onlock"));
        let label = self.script.strip_prefix(&self.root).unwrap_or(&self.script).to_string_lossy().replace('\\', "/");
        Loaded { project, lock, label }
    }

    fn read_script(&self) -> Result<String, String> {
        std::fs::read_to_string(&self.script).map_err(|e| format!("{}: {}", self.script.display(), e))
    }

    fn compile_text(&self, l: &Loaded, text: &str) -> (Episode, Vec<Diagnostic>) {
        compile_source(&l.project, &l.label, text, &l.lock)
    }

    pub fn definitions() -> Vec<ToolDef> {
        vec![
            ToolDef {
                name: "get_guide",
                description: "Read this first. Returns the Oneiro language guide, the exact vocabulary the engine accepts, and a summary of this project's library (characters, sets with their anchors, sounds) and series notes.",
                schema: json!({"type": "object", "properties": {}}),
            },
            ToolDef {
                name: "read_script",
                description: "Returns the current script of the project.",
                schema: json!({"type": "object", "properties": {}}),
            },
            ToolDef {
                name: "check_script",
                description: "Validates a script with the real engine and returns structured diagnostics (code, line, column, message, suggestion) plus a summary (scenes, duration, shots). Pass `script` to check text you have not saved yet, or omit it to check the saved script.",
                schema: json!({"type": "object", "properties": {"script": {"type": "string", "description": "Full script text to validate (optional)."}}}),
            },
            ToolDef {
                name: "write_script",
                description: "Validates the COMPLETE script and, if it has no errors, saves it as the project's script (the previous version is kept as script.onr.bak). Rejects scripts with errors unless allow_errors is true.",
                schema: json!({"type": "object", "properties": {"script": {"type": "string", "description": "The complete script file."}, "allow_errors": {"type": "boolean"}}, "required": ["script"]}),
            },
            ToolDef {
                name: "describe_timeline",
                description: "Lists every script statement with its start/end time and line, and every camera shot with its time range and whether it is pinned. Use it to reason about pacing and to pick times for render_still.",
                schema: json!({"type": "object", "properties": {"script": {"type": "string", "description": "Optional script text (defaults to the saved script)."}}}),
            },
            ToolDef {
                name: "render_still",
                description: "Renders one frame of the episode at `time` seconds and returns it as an image, so you can SEE the result (framing, who is visible, expressions).",
                schema: json!({"type": "object", "properties": {"time": {"type": "number"}, "width": {"type": "integer", "description": "Pixels, default 960."}, "script": {"type": "string"}}, "required": ["time"]}),
            },
            ToolDef {
                name: "contact_sheet",
                description: "Returns one image with a frame from the middle of every camera shot (row-major grid) plus captions. The fastest way to check the framing of a whole scene.",
                schema: json!({"type": "object", "properties": {"max_shots": {"type": "integer", "description": "Default 6."}, "script": {"type": "string"}}}),
            },
            ToolDef {
                name: "render_video",
                description: "Renders the episode to an mp4 file (needs ffmpeg). Preview quality (640x360) by default.",
                schema: json!({"type": "object", "properties": {"output": {"type": "string", "description": "Path of the mp4 (default out/<script>.mp4 in the project)."}, "full_quality": {"type": "boolean"}}}),
            },
            ToolDef {
                name: "list_library",
                description: "Lists the project's characters, sets (with anchors and props) and sounds.",
                schema: json!({"type": "object", "properties": {}}),
            },
            ToolDef {
                name: "generate_sound",
                description: "Creates a royalty-free sound in the project library from the built-in generators, so a script can use it: kind \"sfx\" with a `preset` (door_bell, ding, knock, thud, punch, click, pop, whoosh, boing, buzzer, splash, crash, footstep, sparkle, wah_wah, slide_up, slide_down, honk, zap, coin, phone_ring, clock_tick, scratch, applause) saves library/sounds/<name>.wav; kind \"music\" with a `mood` (cheerful, calm, tense, quirky, sad) saves a seamless loop to library/music/<name>.wav. Then use it with `sfx <name>` or `music <name>`.",
                schema: json!({"type": "object", "properties": {
                    "kind": {"type": "string", "enum": ["sfx", "music"]},
                    "preset": {"type": "string", "description": "sfx preset name."},
                    "mood": {"type": "string", "description": "music mood."},
                    "name": {"type": "string", "description": "File name to save as (letters, digits, _ and -). Defaults to the preset or mood."},
                    "seed": {"type": "integer", "description": "Variation (default 1)."},
                    "bpm": {"type": "number"}, "bars": {"type": "integer"}
                }, "required": ["kind"]}),
            },
            ToolDef {
                name: "generate_voices",
                description: "Synthesizes the voice of every dialogue line that has no audio file yet (text-to-speech with each character's voice settings; engines: sapi = Windows offline, elevenlabs, openai, command, demo) and adds the `audio \"...\"` clauses to the saved script. Returns what it did per line. Lines that already have audio are left alone unless force is true.",
                schema: json!({"type": "object", "properties": {
                    "provider": {"type": "string", "description": "Override the engine for all lines: demo, sapi, elevenlabs, openai, command. Default: each character's voice provider, or the configured default."},
                    "only": {"type": "array", "items": {"type": "string"}, "description": "Only these characters."},
                    "force": {"type": "boolean"}
                }}),
            },
            ToolDef {
                name: "mix_audio",
                description: "Builds the final soundtrack of the saved script and reports its peak and loudness (dBFS), how many voices, effects and music tracks it has, and warnings (limiter engaged, too quiet). Optionally writes it to a WAV.",
                schema: json!({"type": "object", "properties": {"output": {"type": "string", "description": "Optional path of a WAV to write."}}}),
            },            ToolDef {
                name: "write_character_parts",
                description: "Creates a character DRAWN as vector parts (see the guide's 'vector characters' section): validates the `character` source and the four SVG parts (head, body, arm, leg) with the real engine, saves the SVGs to library/parts/<Name>/ and the character to library/characters/<Name>.onchar. Returns every problem found so you can fix and call it again. Render a still afterwards to look at the result.",
                schema: json!({"type": "object", "properties": {
                    "source": {"type": "string", "description": "A `character Name { ... }` block (the `parts` block may omit `file`)."},
                    "svgs": {"type": "object", "description": "SVG text per part.", "properties": {"head": {"type": "string"}, "body": {"type": "string"}, "arm": {"type": "string"}, "leg": {"type": "string"}}, "required": ["head", "body", "arm", "leg"]}
                }, "required": ["source", "svgs"]}),
            },
            ToolDef {
                name: "write_character",
                description: "Validates a `character` definition (see the guide) and saves it to library/characters/<Name>.onchar. The name inside the source decides the file name.",
                schema: json!({"type": "object", "properties": {"source": {"type": "string", "description": "A complete `character Name { ... }` block."}}, "required": ["source"]}),
            },
        ]
    }

    pub fn call(&self, name: &str, args: &Value) -> ToolOutput {
        match self.call_inner(name, args) {
            Ok(o) => o,
            Err(e) => ToolOutput::err(e),
        }
    }

    fn script_arg(&self, args: &Value) -> Result<String, String> {
        match args.get("script").and_then(|s| s.as_str()) {
            Some(s) => Ok(s.to_string()),
            None => self.read_script(),
        }
    }

    fn summary(ep: &Episode, diags: &[Diagnostic]) -> Value {
        let shots: Vec<Value> = ep
            .scenes
            .iter()
            .flat_map(|sc| {
                sc.cameras.iter().map(move |c| json!({"scene": sc.number, "index": c.index, "from": round2(sc.t0 + c.t0), "to": round2(sc.t0 + c.t1), "camera": c.source}))
            })
            .collect();
        json!({
            "valid": count_errors(diags) == 0,
            "errors": count_errors(diags),
            "warnings": diags.len() - count_errors(diags),
            "scenes": ep.scenes.len(),
            "duration_seconds": round2(ep.duration),
            "frames": ep.frame_count(),
            "shots": shots,
            "diagnostics": serde_json::from_str::<Value>(&to_json_array(diags)).unwrap_or(Value::Null),
        })
    }

    fn call_inner(&self, name: &str, args: &Value) -> Result<ToolOutput, String> {
        match name {
            "get_guide" => {
                let l = self.load();
                let mut s = guide::system_prompt(&l.project, Mode::Scene);
                s.push_str("\n\n");
                s.push_str(guide::CHARACTER_GUIDE);
                s.push_str("\n\n");
                s.push_str(guide::PARTS_GUIDE);
                s.push_str("\n\n## Tools\nUse check_script after every change, contact_sheet / render_still to look at the result, write_script to save, write_character to create characters.\n");
                Ok(ToolOutput::ok(s))
            }
            "read_script" => Ok(ToolOutput::ok(self.read_script()?)),
            "check_script" => {
                let l = self.load();
                let text = self.script_arg(args)?;
                let (ep, d) = self.compile_text(&l, &text);
                Ok(ToolOutput::ok(serde_json::to_string_pretty(&Self::summary(&ep, &d)).unwrap()))
            }
            "write_script" => {
                let l = self.load();
                let text = args.get("script").and_then(|s| s.as_str()).ok_or("`script` is required")?.to_string();
                let allow = args.get("allow_errors").and_then(|b| b.as_bool()).unwrap_or(false);
                let (ep, d) = self.compile_text(&l, &text);
                if count_errors(&d) > 0 && !allow {
                    let mut o = ToolOutput::err(format!(
                        "Not saved: the script has errors. Fix them and call write_script again.\n{}",
                        serde_json::to_string_pretty(&Self::summary(&ep, &d)).unwrap()
                    ));
                    o.is_error = true;
                    return Ok(o);
                }
                let bak = self.script.with_extension("onr.bak");
                if let Ok(old) = std::fs::read_to_string(&self.script) {
                    let _ = std::fs::write(&bak, old);
                }
                std::fs::write(&self.script, &text).map_err(|e| format!("{}: {}", self.script.display(), e))?;
                Ok(ToolOutput::ok(format!("Saved {}.\n{}", self.script.display(), serde_json::to_string_pretty(&Self::summary(&ep, &d)).unwrap())))
            }
            "describe_timeline" => {
                let l = self.load();
                let text = self.script_arg(args)?;
                let (ep, d) = self.compile_text(&l, &text);
                let mut out = String::new();
                for sc in &ep.scenes {
                    out.push_str(&format!("Scene {} \"{}\" in {} ({:.1}s, starts at {:.1}s)\n", sc.number, sc.title, sc.set.name, sc.dur, sc.t0));
                    for s in &sc.spans {
                        out.push_str(&format!("  line {:>3}  {:>6.2}s - {:>6.2}s  {}\n", s.line, sc.t0 + s.t0, sc.t0 + s.t1, s.label));
                    }
                    for c in &sc.cameras {
                        let pinned = l.lock.shots.get(&(sc.number, c.index)).map_or(false, |s| s.pinned);
                        out.push_str(&format!("  shot {} {:>6.2}s - {:>6.2}s  {}{}\n", c.index, sc.t0 + c.t0, sc.t0 + c.t1, c.source, if pinned { "  [pinned]" } else { "" }));
                    }
                }
                if count_errors(&d) > 0 {
                    out.push_str(&format!("\nThe script has {} error(s); use check_script for details.\n", count_errors(&d)));
                }
                Ok(ToolOutput::ok(out))
            }
            "render_still" => {
                let l = self.load();
                let text = self.script_arg(args)?;
                let (ep, d) = self.compile_text(&l, &text);
                if count_errors(&d) > 0 {
                    return Ok(ToolOutput::err(format!("The script has errors, nothing to render:\n{}", serde_json::to_string_pretty(&Self::summary(&ep, &d)).unwrap())));
                }
                let t = args.get("time").and_then(|t| t.as_f64()).ok_or("`time` is required (seconds)")?.clamp(0.0, ep.duration);
                let w = (args.get("width").and_then(|w| w.as_u64()).unwrap_or(960).clamp(240, 1920) as u32) & !1;
                let h = (w as f64 * ep.height as f64 / ep.width as f64) as u32 & !1;
                let pm = Renderer::new(&ep, w, h).render_at(t);
                let png = pm.encode_png().map_err(|e| e.to_string())?;
                let caption = ep
                    .locate(t)
                    .and_then(|(si, lt)| {
                        let sc = &ep.scenes[si];
                        sc.cameras.iter().rev().find(|c| c.t0 <= lt).map(|c| format!("scene {} shot {}: {}", sc.number, c.index, c.source))
                    })
                    .unwrap_or_default();
                Ok(ToolOutput { text: format!("Frame at t={:.2}s of {:.1}s. {}", t, ep.duration, caption), images: vec![png], is_error: false })
            }
            "contact_sheet" => {
                let l = self.load();
                let text = self.script_arg(args)?;
                let (ep, d) = self.compile_text(&l, &text);
                if count_errors(&d) > 0 {
                    return Ok(ToolOutput::err(format!("The script has errors, nothing to render:\n{}", serde_json::to_string_pretty(&Self::summary(&ep, &d)).unwrap())));
                }
                let max = args.get("max_shots").and_then(|m| m.as_u64()).unwrap_or(6).clamp(1, 12) as usize;
                let (sheet, captions) = oneiro_render::preview::contact_sheet(&ep, max, 480, 2).ok_or("the episode has no shots")?;
                let png = sheet.encode_png().map_err(|e| e.to_string())?;
                Ok(ToolOutput { text: format!("Contact sheet (left to right, top to bottom):\n{}", captions.join("\n")), images: vec![png], is_error: false })
            }
            "render_video" => {
                let l = self.load();
                let text = self.read_script()?;
                let (ep, d) = self.compile_text(&l, &text);
                if count_errors(&d) > 0 {
                    return Ok(ToolOutput::err("The script has errors; fix them before rendering (use check_script)."));
                }
                let stem = self.script.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("out".into());
                let out = match args.get("output").and_then(|o| o.as_str()) {
                    Some(p) => PathBuf::from(p),
                    None => self.root.join("out").join(format!("{}.mp4", stem)),
                };
                let full = args.get("full_quality").and_then(|b| b.as_bool()).unwrap_or(false);
                let (w, h) = if full { (ep.width, ep.height) } else { (640, 360) };
                encode_episode(&ep, &out, &EncodeOptions { width: w, height: h, preview: !full, audio: true }, &mut |_, _| true)?;
                Ok(ToolOutput::ok(format!("Rendered {} ({:.1}s, {}x{}).", out.display(), ep.duration, w & !1, h & !1)))
            }
            "list_library" => {
                let l = self.load();
                Ok(ToolOutput::ok(guide::library_summary(&l.project)))
            }
            "write_character" => {
                let l = self.load();
                let source = args.get("source").and_then(|s| s.as_str()).ok_or("`source` is required")?;
                let (def, diags) = validate_character(source);
                match def {
                    Some(def) => {
                        let path = l.project.root.join("library").join("characters").join(format!("{}.onchar", def.name));
                        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                        std::fs::write(&path, def.to_source()).map_err(|e| e.to_string())?;
                        Ok(ToolOutput::ok(format!("Saved {}. Declare it in a script with `use character {}`.", path.display(), def.name)))
                    }
                    None => Ok(ToolOutput::err(format!("Not saved. Problems:\n{}", diags.iter().map(|d| d.render()).collect::<Vec<_>>().join("\n")))),
                }
            }
            "generate_sound" => {
                use oneiro_audio::musicgen::{self, Mood, MusicSpec};
                let kind = args.get("kind").and_then(|s| s.as_str()).ok_or("`kind` is required: sfx or music")?;
                let seed = args.get("seed").and_then(|s| s.as_u64()).unwrap_or(1);
                let clean = |s: &str| -> Result<String, String> {
                    if s.is_empty() || !s.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
                        Err(format!("'{}' is not a valid name: use letters, digits, '_' and '-'", s))
                    } else {
                        Ok(s.to_string())
                    }
                };
                match kind {
                    "sfx" => {
                        let preset = args.get("preset").and_then(|s| s.as_str()).ok_or("`preset` is required for sfx")?;
                        let a = oneiro_audio::sfxgen::generate(preset, seed).ok_or(format!("unknown preset '{}'. Presets: {}", preset, oneiro_audio::sfxgen::names().join(", ")))?;
                        let name = clean(args.get("name").and_then(|s| s.as_str()).unwrap_or(preset))?;
                        let path = self.root.join("library").join("sounds").join(format!("{}.wav", name));
                        oneiro_audio::write_wav(&path, &a.samples, oneiro_audio::SR)?;
                        Ok(ToolOutput::ok(format!("Created library/sounds/{}.wav ({:.1}s). Use it with: sfx {}", name, a.duration(), name)))
                    }
                    "music" => {
                        let mood_id = args.get("mood").and_then(|s| s.as_str()).ok_or("`mood` is required for music")?;
                        let mood = Mood::from_id(mood_id).ok_or(format!("unknown mood '{}'. Moods: {}", mood_id, Mood::ALL.iter().map(|m| m.id()).collect::<Vec<_>>().join(", ")))?;
                        let mut spec = MusicSpec::new(mood, seed);
                        if let Some(b) = args.get("bpm").and_then(|b| b.as_f64()) {
                            spec.bpm = (b as f32).clamp(40.0, 220.0);
                        }
                        if let Some(b) = args.get("bars").and_then(|b| b.as_u64()) {
                            spec.bars = (b as usize).clamp(1, 64);
                        }
                        let a = musicgen::generate(&spec);
                        let name = clean(args.get("name").and_then(|s| s.as_str()).unwrap_or(mood.id()))?;
                        let path = self.root.join("library").join("music").join(format!("{}.wav", name));
                        oneiro_audio::write_wav(&path, &a.samples, oneiro_audio::SR)?;
                        Ok(ToolOutput::ok(format!("Created library/music/{}.wav ({:.1}s loop). Use it with: music {}", name, a.duration(), name)))
                    }
                    other => Err(format!("unknown kind '{}': use sfx or music", other)),
                }
            }
            "generate_voices" => {
                let l = self.load();
                let text = self.read_script()?;
                let cfg = oneiro_voice::VoiceConfig::load();
                let provider = match args.get("provider").and_then(|p| p.as_str()) {
                    Some(p) => Some(oneiro_voice::ProviderKind::from_id(p).ok_or(format!("unknown provider '{}': use demo, sapi, elevenlabs, openai or command", p))?),
                    None => None,
                };
                let only = args.get("only").and_then(|o| o.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(|s| s.to_string())).collect()).unwrap_or_default();
                let opts = oneiro_voice::GenOptions { force: args.get("force").and_then(|b| b.as_bool()).unwrap_or(false), provider, only, session_key: None };
                let (out, reports) = oneiro_voice::generate_script(&self.root, &l.project, &text, &cfg, &opts, &mut |_, _, _| true)?;
                let mut s = String::new();
                for r in &reports {
                    let what = if r.skipped {
                        "already has audio".to_string()
                    } else if let Some(e) = &r.error {
                        format!("FAILED: {}", e)
                    } else {
                        format!("{} ({:.1}s, {})", r.path, r.seconds, r.provider.id())
                    };
                    s.push_str(&format!("line {:>3} {:<8} {}\n", r.line, r.who, what));
                }
                if out != text {
                    let bak = self.script.with_extension("onr.bak");
                    let _ = std::fs::write(&bak, &text);
                    std::fs::write(&self.script, &out).map_err(|e| format!("{}: {}", self.script.display(), e))?;
                    s.push_str("\nThe script was updated with the new audio clauses.\n");
                }
                let failed = reports.iter().filter(|r| r.error.is_some()).count();
                if failed > 0 {
                    return Ok(ToolOutput::err(format!("{}\n{} line(s) failed. Set up the engine (Voice settings in the IDE, or an API key variable) or pass provider \"sapi\" or \"demo\".", s, failed)));
                }
                Ok(ToolOutput::ok(s))
            }
            "mix_audio" => {
                let l = self.load();
                let text = self.read_script()?;
                let (ep, d) = self.compile_text(&l, &text);
                if count_errors(&d) > 0 {
                    return Ok(ToolOutput::err(format!("The script has errors, nothing to mix:\n{}", serde_json::to_string_pretty(&Self::summary(&ep, &d)).unwrap())));
                }
                let (samples, rep) = ep.render_audio();
                let count = |k| ep.audio.iter().filter(|e| e.kind == k).count();
                let mut s = format!(
                    "Soundtrack {:.1}s: {} voice(s), {} effect(s), {} music track(s). Peak {:.1} dBFS, loudness {:.1} dBFS.",
                    ep.duration,
                    count(oneiro_core::AudioKind::Voice),
                    count(oneiro_core::AudioKind::Sfx),
                    count(oneiro_core::AudioKind::Music),
                    rep.peak_db,
                    rep.loudness_db
                );
                if rep.limited {
                    s.push_str(" Warning: the limiter had to work; lower some `volume` values.");
                }
                if rep.loudness_db < -32.0 {
                    s.push_str(" Warning: very quiet (is there any audio?).");
                }
                let missing = ep.audio.len() == 0;
                if missing {
                    s.push_str(" The episode has no audio events yet.");
                }
                if let Some(out) = args.get("output").and_then(|o| o.as_str()) {
                    oneiro_audio::write_wav(Path::new(out), &samples, oneiro_audio::SR)?;
                    s.push_str(&format!(" Wrote {}.", out));
                }
                Ok(ToolOutput::ok(s))
            }            "write_character_parts" => {
                let source = args.get("source").and_then(|s| s.as_str()).ok_or("`source` is required")?;
                let svgs: std::collections::BTreeMap<String, String> = args
                    .get("svgs")
                    .and_then(|o| o.as_object())
                    .ok_or("`svgs` is required: an object with head, body, arm and leg")?
                    .iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect();
                match crate::parts::validate_parts(source, &svgs) {
                    Ok((def, src, files)) => {
                        let path = crate::parts::save_files(&self.root, &def.name, &src, &files)?;
                        Ok(ToolOutput::ok(format!("Saved {} and {} SVG parts. Use it with `use character {}` and look at it with render_still.", path.display(), files.len(), def.name)))
                    }
                    Err(diags) => Ok(ToolOutput::err(format!("Not saved. Problems:\n{}", diags.iter().filter(|d| d.is_error()).map(|d| d.render()).collect::<Vec<_>>().join("\n")))),
                }
            }
            other => Err(format!("unknown tool '{}'", other)),
        }
    }
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}
