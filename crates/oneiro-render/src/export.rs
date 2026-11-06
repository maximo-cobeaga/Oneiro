//! mp4 export through ffmpeg (raw RGBA frames piped to its stdin).

use std::io::Write;
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};

pub struct Encoder {
    child: Child,
    stdin: Option<ChildStdin>,
}

fn ffmpeg_bin() -> String {
    std::env::var("ONEIRO_FFMPEG").unwrap_or_else(|_| "ffmpeg".into())
}

impl Encoder {
    pub fn start(out: &Path, w: u32, h: u32, fps: u32, audio: Option<&Path>, preview: bool) -> Result<Encoder, String> {
        let mut cmd = Command::new(ffmpeg_bin());
        cmd.args(["-y", "-hide_banner", "-loglevel", "error"]);
        cmd.args(["-f", "rawvideo", "-pix_fmt", "rgba", "-s", &format!("{}x{}", w, h), "-r", &fps.to_string(), "-i", "-"]);
        if let Some(a) = audio {
            cmd.arg("-i").arg(a);
        }
        cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p"]);
        if preview {
            cmd.args(["-preset", "veryfast", "-crf", "26"]);
        } else {
            cmd.args(["-preset", "medium", "-crf", "17"]);
        }
        if audio.is_some() {
            cmd.args(["-c:a", "aac", "-b:a", "192k", "-shortest"]);
        }
        cmd.args(["-movflags", "+faststart"]).arg(out);
        cmd.stdin(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            format!(
                "could not start ffmpeg ({}). Install ffmpeg and put it on PATH, or set ONEIRO_FFMPEG to its full path. \
                 Use --png-dir to export frames without ffmpeg.",
                e
            )
        })?;
        let stdin = child.stdin.take();
        Ok(Encoder { child, stdin })
    }

    pub fn write_frame(&mut self, data: &[u8]) -> Result<(), String> {
        self.stdin
            .as_mut()
            .ok_or("encoder closed")?
            .write_all(data)
            .map_err(|e| format!("ffmpeg stopped accepting frames: {}", e))
    }

    pub fn finish(mut self) -> Result<(), String> {
        drop(self.stdin.take());
        let status = self.child.wait().map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("ffmpeg exited with {}", status))
        }
    }
}
