//! `oneiro doctor`: what works on this machine and what is missing.

use std::process::{Command, Stdio};

fn runs(cmd: &str, arg: &str) -> bool {
    Command::new(cmd).arg(arg).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

fn line(ok: bool, what: &str, detail: &str) {
    println!("  [{}] {:<22} {}", if ok { "ok" } else { "--" }, what, detail);
}

pub fn cmd_doctor(_args: &[String]) -> Result<bool, String> {
    println!("oneiro {} on {}", env!("CARGO_PKG_VERSION"), std::env::consts::OS);
    println!("\nRendering");
    line(true, "cpu renderer", &format!("{} threads", std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1)));
    match oneiro_toon3d::gpu_name() {
        Some(n) => line(true, "3d on gpu", &n),
        None => line(true, "3d on cpu", "no usable graphics adapter (or ONEIRO_3D=cpu); slower but identical output"),
    }
    let ff = std::env::var("ONEIRO_FFMPEG").unwrap_or_else(|_| "ffmpeg".into());
    let ff_ok = runs(&ff, "-version");
    line(ff_ok, "ffmpeg", if ff_ok { "mp4 export and mp3/ogg import available" } else { "missing: renders can still write PNGs (--png-dir) and WAV; install ffmpeg or set ONEIRO_FFMPEG" });

    println!("\nVoices");
    let sapi = cfg!(windows);
    line(sapi, "windows voices (sapi)", if sapi { "offline text-to-speech built in" } else { "windows only" });
    for (var, what) in [("ELEVENLABS_API_KEY", "elevenlabs"), ("OPENAI_API_KEY", "openai voices + ai")] {
        let set = std::env::var(var).map(|v| !v.is_empty()).unwrap_or(false);
        let hint = format!("set {} to use it", var);
        line(set, what, if set { "key found" } else { &hint });
    }

    println!("\nAI directors");
    for (cmd, what) in [("claude", "claude-cli"), ("codex", "codex-cli")] {
        let ok = runs(cmd, "--version");
        line(ok, what, if ok { "found on PATH (uses your own login)" } else { "not on PATH" });
    }
    let key = std::env::var("ANTHROPIC_API_KEY").map(|v| !v.is_empty()).unwrap_or(false);
    line(key, "anthropic api", if key { "key found" } else { "set ANTHROPIC_API_KEY to use it" });
    println!("\nNothing marked [--] is required: the language, the renderer and the IDE work without it.");
    Ok(true)
}
