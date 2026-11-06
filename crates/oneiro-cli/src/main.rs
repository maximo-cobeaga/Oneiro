//! `oneiro` command line: check, render, still, demo-audio.


mod ai_cmds;
mod new_cmd;
mod audio_cmds;
mod cut_cmds;

use oneiro_audio::{synth, write_wav, SR};
use oneiro_core::lock::{self, Lock};
use oneiro_core::project::locate;
use oneiro_core::{compile, Episode, Project};
use oneiro_lang::{parse, to_json_array, Diagnostic, StmtKind};
use oneiro_render::{hash_pixmap, render_range, Renderer};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

const USAGE: &str = "\
oneiro 0.1 - a language and studio for animated series

USAGE:
  oneiro new <dir> [--name NAME]   scaffold a project that renders out of the box
  oneiro check <project|script.onr> [--json]
  oneiro render <project|script.onr> [-o out.mp4] [--preview] [--res WxH] [--fps N]
                [--no-audio] [--hash] [--png-dir DIR] [--no-write] [--3d cpu|gpu|auto]
  oneiro still <project|script.onr> --time SECONDS [-o frame.png] [--res WxH]
  oneiro demo-audio <project|script.onr> [--force]   placeholder voices/sounds for files a script references
  oneiro cut <image> [project] [--name Name] [--layout file.json] [--ai] [--overlay out.png] [--height 1.15] [--style S]
        cut an illustration (front view, plain background or transparent PNG) into a rigged character
  oneiro voice <project> [--provider demo|sapi|elevenlabs|openai|command] [--force] [--only A,B] [--dry-run]
        synthesize the dialogue lines that have no audio and write the audio clauses
  oneiro voices [provider]                       list the voices a provider offers
  oneiro sound sfx <preset|--list> [project] [--as NAME] [--seed N]
  oneiro sound music <cheerful|calm|tense|quirky|sad> [project] [--as NAME] [--bpm N] [--bars N] [--seed N] [--key N]
  oneiro import <project> <file> --kind voice|sfx|music|raw [--as NAME]    (mp3, ogg, flac, m4a... via ffmpeg)
  oneiro mix <project> [-o soundtrack.wav]       write the final soundtrack and print its levels
  oneiro vocab [--json]
  oneiro guide [project]                        write docs/ai/guide.md (language guide + library) for AI tools
  oneiro ai-test [--provider P] [--model M]     check that an AI provider answers
  oneiro ai <project> <request> [--provider claude-cli|codex-cli|anthropic|openai|custom-cli] [--model M]
        [--base-url URL] [--key-env VAR] [--command <cmd args>] [--self-check] [--rounds N] [--character [--svg]] [--apply]

NOTES:
  render also writes <script>.onlock (resolved camera/placement coordinates) and state.out.
  Set `pinned: true` on an entry in the .onlock file to keep a hand-edited value.
  mp4 export needs ffmpeg on PATH (or set ONEIRO_FFMPEG).";

struct Opts {
    path: String,
    json: bool,
    out: Option<String>,
    preview: bool,
    res: Option<(u32, u32)>,
    fps: Option<u32>,
    no_audio: bool,
    hash: bool,
    png_dir: Option<String>,
    no_write: bool,
    time: Option<f64>,
    force: bool,
}

fn parse_opts(args: &[String]) -> Result<Opts, String> {
    let mut o = Opts {
        path: String::new(),
        json: false,
        out: None,
        preview: false,
        res: None,
        fps: None,
        no_audio: false,
        hash: false,
        png_dir: None,
        no_write: false,
        time: None,
        force: false,
    };
    let mut i = 0;
    while i < args.len() {
        let a = &args[i];
        let mut val = |name: &str| -> Result<String, String> {
            i += 1;
            args.get(i).cloned().ok_or(format!("{} needs a value", name))
        };
        match a.as_str() {
            "--json" => o.json = true,
            "-o" | "--out" => o.out = Some(val("-o")?),
            "--preview" => o.preview = true,
            "--no-audio" => o.no_audio = true,
            "--3d" => {
                let v = val("--3d")?;
                if !["cpu", "gpu", "auto"].contains(&v.as_str()) {
                    return Err("--3d must be cpu, gpu or auto".into());
                }
                std::env::set_var("ONEIRO_3D", v);
            }
            "--hash" => o.hash = true,
            "--no-write" => o.no_write = true,
            "--force" => o.force = true,
            "--png-dir" => o.png_dir = Some(val("--png-dir")?),
            "--time" => o.time = Some(val("--time")?.parse().map_err(|_| "--time must be a number".to_string())?),
            "--fps" => o.fps = Some(val("--fps")?.parse().map_err(|_| "--fps must be an integer".to_string())?),
            "--res" => {
                let v = val("--res")?;
                let (w, h) = v.split_once('x').ok_or("--res must look like 1280x720")?;
                o.res = Some((
                    w.parse().map_err(|_| "bad width in --res".to_string())?,
                    h.parse().map_err(|_| "bad height in --res".to_string())?,
                ));
            }
            s if s.starts_with('-') => return Err(format!("unknown option {}", s)),
            _ => {
                if o.path.is_empty() {
                    o.path = a.clone();
                } else {
                    return Err(format!("unexpected argument {}", a));
                }
            }
        }
        i += 1;
    }
    if o.path.is_empty() {
        return Err("missing project path".into());
    }
    Ok(o)
}

struct Loaded {
    project: Project,
    script: PathBuf,
    lock: Lock,
    episode: Episode,
    diags: Vec<Diagnostic>,
}

fn load(path: &str) -> Result<Loaded, String> {
    let (root, script) = locate(Path::new(path))?;
    let mut diags = Vec::new();
    let project = Project::load(&root, &mut diags);
    let lock = Lock::load(&script.with_extension("onlock"));
    let (episode, mut d2) = compile(&project, &script, &lock);
    diags.append(&mut d2);
    Ok(Loaded { project, script, lock, episode, diags })
}

fn print_diags(diags: &[Diagnostic], json: bool) {
    if json {
        println!("{}", to_json_array(diags));
    } else {
        for d in diags {
            eprintln!("{}", d.render());
        }
    }
}

fn has_errors(diags: &[Diagnostic]) -> bool {
    diags.iter().any(|d| d.is_error())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let Some(cmd) = args.first() else {
        println!("{}", USAGE);
        return ExitCode::SUCCESS;
    };
    let rest = &args[1..];
    let result = match cmd.as_str() {
        "new" => new_cmd::cmd_new(rest),
        "check" => cmd_check(rest),
        "render" => cmd_render(rest),
        "still" => cmd_still(rest),
        "demo-audio" => cmd_demo_audio(rest),
        "voice" => audio_cmds::cmd_voice(rest),
        "cut" => cut_cmds::cmd_cut(rest),
        "voices" => audio_cmds::cmd_voices(rest),
        "sound" => audio_cmds::cmd_sound(rest),
        "import" => audio_cmds::cmd_import(rest),
        "mix" => audio_cmds::cmd_mix(rest),
        "vocab" => cmd_vocab(rest),
        "ai" => ai_cmds::cmd_ai(rest),
        "ai-test" => ai_cmds::cmd_ai_test(rest),
        "guide" => ai_cmds::cmd_guide(rest),
        "help" | "--help" | "-h" => {
            println!("{}", USAGE);
            Ok(true)
        }
        other => Err(format!("unknown command '{}'\n\n{}", other, USAGE)),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::from(1),
        Err(e) => {
            eprintln!("error: {}", e);
            ExitCode::from(2)
        }
    }
}

fn cmd_check(args: &[String]) -> Result<bool, String> {
    let o = parse_opts(args)?;
    let l = load(&o.path)?;
    print_diags(&l.diags, o.json);
    let errors = l.diags.iter().filter(|d| d.is_error()).count();
    let warnings = l.diags.len() - errors;
    if !o.json {
        if errors == 0 {
            eprintln!(
                "ok: {} scene(s), {:.1}s, {} frames at {} fps ({} warning(s))",
                l.episode.scenes.len(),
                l.episode.duration,
                l.episode.frame_count(),
                l.episode.fps,
                warnings
            );
        } else {
            eprintln!("{} error(s), {} warning(s)", errors, warnings);
        }
    }
    Ok(errors == 0)
}

fn write_if_changed(path: &Path, text: &str) {
    if std::fs::read_to_string(path).map_or(true, |old| old != text) {
        let _ = std::fs::write(path, text);
    }
}

fn cmd_render(args: &[String]) -> Result<bool, String> {
    let o = parse_opts(args)?;
    let mut l = load(&o.path)?;
    print_diags(&l.diags, false);
    if has_errors(&l.diags) {
        eprintln!("not rendering: fix the errors above first");
        return Ok(false);
    }
    if let Some(f) = o.fps {
        l.episode.fps = f;
    }
    let (w, h) = o.res.unwrap_or(if o.preview { (640, 360) } else { (l.episode.width, l.episode.height) });
    let (w, h) = (w & !1, h & !1); // yuv420p needs even sizes
    if l.episode.scenes.iter().any(|s| s.actors.iter().any(|a| a.def.style == "toon_3d")) {
        match oneiro_toon3d::gpu_name() {
            Some(n) => eprintln!("3D characters: GPU ({})", n),
            None => eprintln!("3D characters: CPU rasterizer"),
        }
    }

    if !o.no_write {
        let name = l.script.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let lock_text = lock::render(&l.episode.scenes, &l.lock, &name);
        write_if_changed(&l.script.with_extension("onlock"), &lock_text);
        if !l.episode.state.is_empty() {
            write_if_changed(&l.script.with_file_name("state.out"), &l.episode.state.render());
        }
    }

    let ep = &l.episode;
    let total = ep.frame_count();
    eprintln!(
        "rendering '{}': {} scene(s), {:.1}s, {} frames, {}x{} @ {} fps ({} threads)",
        ep.name,
        ep.scenes.len(),
        ep.duration,
        total,
        w,
        h,
        ep.fps,
        std::thread::available_parallelism().map_or(1, |n| n.get())
    );
    let renderer = Renderer::new(ep, w, h);
    let start = Instant::now();

    // Output sink
    let out_path = PathBuf::from(o.out.clone().unwrap_or_else(|| {
        let stem = l.script.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or("out".into());
        l.project.root.join("out").join(format!("{}.mp4", stem)).to_string_lossy().to_string()
    }));

    let mut combined_hash: u64 = 0xcbf29ce484222325;

    if let Some(dir) = &o.png_dir {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        render_range(&renderer, 0..total, |i, pm| {
            combined_hash = (combined_hash ^ hash_pixmap(pm)).wrapping_mul(0x100000001b3);
            pm.save_png(Path::new(dir).join(format!("frame_{:05}.png", i))).map_err(|e| e.to_string())
        })?;
    } else if o.hash {
        render_range(&renderer, 0..total, |_, pm| {
            combined_hash = (combined_hash ^ hash_pixmap(pm)).wrapping_mul(0x100000001b3);
            Ok(())
        })?;
    } else {
        let opts = oneiro_render::encode::EncodeOptions { width: w, height: h, preview: o.preview, audio: !o.no_audio };
        combined_hash = oneiro_render::encode::encode_episode(ep, &out_path, &opts, &mut |done, total| {
            if done % 48 == 0 || done == total {
                eprint!("{}  frame {}/{}", char::from(13u8), done, total);
            }
            true
        })?;
        eprintln!();
        eprintln!("wrote {}", out_path.display());
    }
    let secs = start.elapsed().as_secs_f64();
    eprintln!(
        "done in {:.1}s ({:.1} fps render speed, {:.1}x realtime)",
        secs,
        total as f64 / secs.max(1e-6),
        ep.duration / secs.max(1e-6)
    );
    println!("frames-hash: {:016x}", combined_hash);
    Ok(true)
}

fn cmd_still(args: &[String]) -> Result<bool, String> {
    let o = parse_opts(args)?;
    let l = load(&o.path)?;
    print_diags(&l.diags, false);
    if has_errors(&l.diags) {
        return Ok(false);
    }
    let t = o.time.ok_or("still needs --time SECONDS")?;
    let (w, h) = o.res.unwrap_or((l.episode.width, l.episode.height));
    let r = Renderer::new(&l.episode, w & !1, h & !1);
    let pm = r.render_at(t);
    let out = o.out.unwrap_or_else(|| format!("still_{:.2}.png", t));
    pm.save_png(&out).map_err(|e| e.to_string())?;
    eprintln!("wrote {}", out);
    Ok(true)
}

/// Synthesizes placeholder voices and sounds for every `audio` / `sfx` reference that has no file yet.
fn cmd_demo_audio(args: &[String]) -> Result<bool, String> {
    let o = parse_opts(args)?;
    let (root, script) = locate(Path::new(&o.path))?;
    let mut diags = Vec::new();
    let project = Project::load(&root, &mut diags);
    let src = std::fs::read_to_string(&script).map_err(|e| e.to_string())?;
    let (file, _) = parse(&src, "script");

    fn walk(stmts: &[oneiro_lang::Stmt], out: &mut Vec<(String, String, String)>, sfx: &mut Vec<String>) {
        for s in stmts {
            match &s.kind {
                StmtKind::Dialogue(d) => {
                    if let Some(a) = &d.audio {
                        out.push((d.who.clone(), d.text.clone(), a.clone()));
                    }
                }
                StmtKind::Sfx(x) => sfx.push(x.name.clone()),
                StmtKind::Together(v) => walk(v, out, sfx),
                _ => {}
            }
        }
    }
    let mut lines = Vec::new();
    let mut sfx = Vec::new();
    for sc in &file.scenes {
        walk(&sc.stmts, &mut lines, &mut sfx);
    }
    let mut made = 0;
    for (who, text, rel) in lines {
        let path = root.join(&rel);
        if path.exists() && !o.force {
            continue;
        }
        let pitch = project.characters.get(&who).map(|c| c.voice_pitch).unwrap_or(150.0) as f32;
        let seed = oneiro_core::math::hash_str(&format!("{}:{}", who, text));
        let samples = synth::synth_speech(&text, pitch, SR, seed);
        write_wav(&path, &samples, SR)?;
        eprintln!("  voice  {}  ({:.1}s)", rel, samples.len() as f64 / SR as f64);
        made += 1;
    }
    for name in sfx {
        let path = root.join("library").join("sounds").join(format!("{}.wav", name));
        if path.exists() && !o.force {
            continue;
        }
        let samples = match oneiro_audio::sfxgen::generate(&name, 1) {
            Some(a) => a.samples,
            None if name.contains("bell") => synth::synth_door_bell(SR),
            None => synth::synth_thud(SR),
        };
        write_wav(&path, &samples, SR)?;
        eprintln!("  sound  library/sounds/{}.wav", name);
        made += 1;
    }
    eprintln!("{} placeholder file(s) created", made);
    Ok(true)
}

/// Prints the vocabulary the engine currently understands, so an AI (or a person) never has to guess.
fn cmd_vocab(args: &[String]) -> Result<bool, String> {
    use oneiro_core::catalog::*;
    let json = args.iter().any(|a| a == "--json");
    let exprs: Vec<&str> = EXPRESSIONS.to_vec();
    let wardrobe = wardrobe_names();
    let groups: Vec<(&str, Vec<&str>)> = vec![
        ("verbs", IMPLEMENTED_VERBS.to_vec()),
        ("verbs_planned", PLANNED_VERBS.to_vec()),
        ("shots", SHOTS.to_vec()),
        ("angles", ANGLES.to_vec()),
        ("moves", MOVES.to_vec()),
        ("sides", SIDES.to_vec()),
        ("relations", RELATIONS.to_vec()),
        ("emotions", exprs),
        ("times", TIMES.to_vec()),
        ("manner_words", MANNER_WORDS.to_vec()),
        ("heads", HEADS.to_vec()),
        ("eyes", EYES.to_vec()),
        ("mouths", MOUTHS.to_vec()),
        ("bodies", BODIES.to_vec()),
        ("rigs", RIGS.to_vec()),
        ("wardrobe", wardrobe),
        ("props", PROP_KINDS.to_vec()),
        ("transitions", TRANSITIONS.to_vec()),
        ("styles", STYLES.to_vec()),
        ("styles_planned", PLANNED_STYLES.to_vec()),
        ("svg_parts", SVG_PARTS.to_vec()),
    ];
    if json {
        let items: Vec<String> = groups
            .iter()
            .map(|(k, v)| format!("\"{}\":[{}]", k, v.iter().map(|s| format!("\"{}\"", s)).collect::<Vec<_>>().join(",")))
            .collect();
        println!("{{{}}}", items.join(","));
    } else {
        println!("# Oneiro vocabulary (generated by `oneiro vocab`)\n");
        for (k, v) in groups {
            println!("## {}\n{}\n", k, v.join(", "));
        }
        println!("## notes\n- `camera` and `cut to` statements take no time; `for <dur>` sets the length of the camera move.\n- Statements run in sequence; join parallel actions with `and` or use `together {{ }}`.\n- Anchors are (x, z) or (x, y, z) in meters: x east, z north, origin at the center of the set.");
    }
    Ok(true)
}
