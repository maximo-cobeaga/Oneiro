//! Audio support: WAV I/O and import, effects, the episode mixer, procedural SFX and music,
//! provider-independent lipsync, a tiny demo speech synthesizer and (feature `device`) microphone
//! recording and speaker playback.

pub mod dsp;
pub mod import;
pub mod lipsync;
pub mod mix;
pub mod musicgen;
pub mod sfxgen;
pub mod synth;
pub mod wav;

#[cfg(feature = "device")]
pub mod device;

pub use lipsync::{Shape, Viseme, VisemeTrack};
pub use wav::{read_wav, write_wav, Audio, SR};

/// A process-unique number (temporary file names).
pub fn unique_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}
