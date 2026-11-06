//! Importing audio files of any common format. WAV is read directly; mp3, ogg, flac, m4a, aac,
//! opus and the audio track of videos are decoded with `ffmpeg` (which the exporter needs anyway).

use crate::wav::{read_wav, write_wav, Audio, SR};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub fn ffmpeg_available() -> bool {
    Command::new("ffmpeg").arg("-version").stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false)
}

/// Decodes raw 16-bit little-endian mono PCM (what several voice APIs return).
pub fn from_pcm16(bytes: &[u8], sr: u32) -> Audio {
    let samples = bytes.chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0).collect();
    Audio { sr, samples }
}

/// Reads any audio file as mono.
pub fn read_any(path: &Path) -> Result<Audio, String> {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    if ext == "wav" {
        if let Ok(a) = read_wav(path) {
            return Ok(a);
        }
    }
    decode_with_ffmpeg(path)
}

/// Decodes audio bytes of a known container (e.g. mp3 from a TTS service) via a temporary file.
pub fn decode_bytes(bytes: &[u8], ext: &str) -> Result<Audio, String> {
    let dir = std::env::temp_dir().join("oneiro-import");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let name = format!("in_{}_{}.{}", std::process::id(), crate::unique_id(), ext.trim_start_matches('.'));
    let p = dir.join(name);
    std::fs::write(&p, bytes).map_err(|e| e.to_string())?;
    let r = read_any(&p);
    let _ = std::fs::remove_file(&p);
    r
}

fn decode_with_ffmpeg(path: &Path) -> Result<Audio, String> {
    if !path.exists() {
        return Err(format!("{} does not exist", path.display()));
    }
    let dir = std::env::temp_dir().join("oneiro-import");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tmp = dir.join(format!("out_{}_{}.wav", std::process::id(), crate::unique_id()));
    let out = Command::new("ffmpeg")
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(path)
        .args(["-vn", "-ac", "1", "-ar", &SR.to_string(), "-c:a", "pcm_f32le"])
        .arg(&tmp)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run ffmpeg ({}). Install ffmpeg and put it on PATH to import this format, or convert the file to WAV", e))?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        return Err(format!("ffmpeg could not read {}: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim()));
    }
    let r = read_wav(&tmp);
    let _ = std::fs::remove_file(&tmp);
    r
}

/// How an imported file is processed before it lands in the project.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImportKind {
    /// Spoken line: clean (DC, rumble, trim silence) and level to about -20 dBFS RMS.
    Voice,
    /// Sound effect: trim silence and peak-normalize.
    Sfx,
    /// Music: peak-normalize only, keep everything.
    Music,
    /// Keep the audio exactly as it is (only converted to the project format).
    Raw,
}

pub fn process(a: &Audio, kind: ImportKind) -> Audio {
    let a = a.resampled(SR);
    match kind {
        ImportKind::Voice => crate::dsp::clean_voice(&a),
        ImportKind::Sfx => {
            let mut x = crate::dsp::trim_silence(&a, -50.0, 0.02);
            crate::dsp::normalize_peak(&mut x, -3.0);
            x
        }
        ImportKind::Music => {
            let mut x = a;
            crate::dsp::normalize_peak(&mut x, -3.0);
            x
        }
        ImportKind::Raw => a,
    }
}

/// Imports `src` into the project at `dest` (path relative to `root`, `.wav` enforced). Returns the
/// final absolute path. The result is always a 48 kHz mono WAV.
pub fn import_into(root: &Path, src: &Path, dest: &str, kind: ImportKind) -> Result<PathBuf, String> {
    let a = read_any(src)?;
    if a.samples.is_empty() {
        return Err(format!("{} contains no audio", src.display()));
    }
    let out = process(&a, kind);
    if out.samples.is_empty() {
        return Err(format!("{} is silent", src.display()));
    }
    let mut rel = dest.replace('\\', "/");
    if !rel.to_lowercase().ends_with(".wav") {
        rel.push_str(".wav");
    }
    let path = root.join(&rel);
    write_wav(&path, &out.samples, out.sr)?;
    Ok(path)
}
