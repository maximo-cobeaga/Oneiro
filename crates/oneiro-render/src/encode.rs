//! Episode -> mp4 (frames rendered in parallel, mixed soundtrack, ffmpeg encoding).

use crate::export::Encoder;
use crate::{hash_pixmap, render_range, Renderer};
use oneiro_audio::{write_wav, SR};
use oneiro_core::Episode;
use std::path::Path;

pub struct EncodeOptions {
    pub width: u32,
    pub height: u32,
    pub preview: bool,
    pub audio: bool,
}

/// Renders the whole episode to `out`. `progress(done, total)` is called regularly; return
/// `false` from it to cancel. Returns a hash of all frames (for reproducibility checks).
pub fn encode_episode(
    ep: &Episode,
    out: &Path,
    opts: &EncodeOptions,
    progress: &mut dyn FnMut(usize, usize) -> bool,
) -> Result<u64, String> {
    let (w, h) = (opts.width & !1, opts.height & !1);
    let total = ep.frame_count();
    if let Some(parent) = out.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let audio_path = if opts.audio {
        let (samples, _report) = ep.render_audio();
        let p = out.with_extension("mix.wav");
        write_wav(&p, &samples, SR)?;
        Some(p)
    } else {
        None
    };
    let mut enc = Encoder::start(out, w, h, ep.fps, audio_path.as_deref(), opts.preview)?;
    let renderer = Renderer::new(ep, w, h);
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut cancelled = false;
    let result = render_range(&renderer, 0..total, |i, pm| {
        hash = (hash ^ hash_pixmap(pm)).wrapping_mul(0x100000001b3);
        enc.write_frame(pm.data())?;
        if !progress(i + 1, total) {
            cancelled = true;
            return Err("cancelled".into());
        }
        Ok(())
    });
    let finished = if result.is_ok() { enc.finish() } else { Ok(()) };
    if let Some(p) = &audio_path {
        let _ = std::fs::remove_file(p);
    }
    if cancelled {
        let _ = std::fs::remove_file(out);
        return Err("cancelled".into());
    }
    result?;
    finished?;
    Ok(hash)
}
