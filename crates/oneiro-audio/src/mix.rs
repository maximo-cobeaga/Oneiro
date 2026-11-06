//! The episode mixer: voices, sound effects and music with fades, looping, an optional stop time,
//! automatic ducking of the music under speech, and a soft limiter on the master.

use crate::dsp;
use crate::wav::{Audio, SR};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Voice,
    Sfx,
    Music,
}

#[derive(Clone)]
pub struct Event {
    pub kind: Kind,
    /// Start time in seconds.
    pub start: f64,
    pub audio: Audio,
    pub gain: f32,
    pub fade_in: f64,
    pub fade_out: f64,
    /// The event is cut (after its fade out) at this time. `None`: plays to its end.
    pub stop: Option<f64>,
    /// Repeats until it is stopped or the episode ends.
    pub looped: bool,
}

impl Event {
    pub fn new(kind: Kind, start: f64, audio: Audio, gain: f32) -> Event {
        Event { kind, start, audio, gain, fade_in: 0.0, fade_out: 0.0, stop: None, looped: false }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MixOptions {
    /// How far the music drops while someone speaks (dB, 0 disables ducking).
    pub duck_db: f32,
    /// Seconds to reach the ducked level.
    pub duck_attack: f64,
    /// Seconds to come back after the speech ends.
    pub duck_release: f64,
    pub master_gain: f32,
}

impl Default for MixOptions {
    fn default() -> Self {
        MixOptions { duck_db: -9.0, duck_attack: 0.12, duck_release: 0.6, master_gain: 1.0 }
    }
}

#[derive(Clone, Debug, Default)]
pub struct MixReport {
    pub peak_db: f32,
    pub loudness_db: f32,
    /// The summed signal went above 0 dBFS before the limiter, so the limiter had to work.
    pub limited: bool,
}

/// Mixes events into a single mono buffer at `SR`.
pub fn mix(events: &[Event], total_seconds: f64) -> Vec<f32> {
    mix_with(events, total_seconds, &MixOptions::default()).0
}

pub fn mix_with(events: &[Event], total_seconds: f64, opts: &MixOptions) -> (Vec<f32>, MixReport) {
    let n = (total_seconds * SR as f64).ceil() as usize;
    let mut voices = vec![0.0f32; n];
    let mut sfx = vec![0.0f32; n];
    let mut music = vec![0.0f32; n];
    // Speech activity (1 where somebody talks) drives the ducking of the music.
    let mut speech = vec![0.0f32; n];

    for e in events {
        let a = e.audio.resampled(SR);
        if a.samples.is_empty() {
            continue;
        }
        let start = (e.start.max(0.0) * SR as f64) as usize;
        if start >= n {
            continue;
        }
        let end_by_stop = e.stop.map(|s| ((s * SR as f64) as usize).min(n)).unwrap_or(n);
        let natural_end = if e.looped { n } else { (start + a.samples.len()).min(n) };
        let end = natural_end.min(end_by_stop).max(start);
        let len = end - start;
        let fade_in = (e.fade_in * SR as f64) as usize;
        let fade_out = ((e.fade_out * SR as f64) as usize).min(len);
        let dst = match e.kind {
            Kind::Voice => &mut voices,
            Kind::Sfx => &mut sfx,
            Kind::Music => &mut music,
        };
        for i in 0..len {
            let s = a.samples[i % a.samples.len()];
            let mut f = 1.0f32;
            if fade_in > 0 && i < fade_in {
                f *= i as f32 / fade_in as f32;
            }
            if fade_out > 0 && i >= len - fade_out {
                f *= (len - i) as f32 / fade_out as f32;
            }
            dst[start + i] += s * e.gain * f;
        }
        if e.kind == Kind::Voice {
            // Mark only the stretches where the voice is actually audible (not the silences).
            let block = (SR as usize / 50).max(1);
            let mut i = 0;
            while i < len {
                let j = (i + block).min(len);
                let loud = (i..j).any(|k| a.samples[k % a.samples.len()].abs() * e.gain > 0.02);
                if loud {
                    for k in i..j {
                        speech[start + k] = 1.0;
                    }
                }
                i = j;
            }
        }
    }

    // Smooth duck envelope: attack towards 1 while speech is on, release when off.
    if opts.duck_db < -0.1 {
        let duck = dsp::db_to_gain(opts.duck_db);
        let att = 1.0 - (-1.0 / (opts.duck_attack.max(0.001) * SR as f64)).exp();
        let rel = 1.0 - (-1.0 / (opts.duck_release.max(0.001) * SR as f64)).exp();
        let (att, rel) = (att as f32, rel as f32);
        let mut env = 0.0f32;
        for i in 0..n {
            let target = speech[i];
            env += (target - env) * if target > env { att } else { rel };
            music[i] *= 1.0 - env * (1.0 - duck);
        }
    }

    let mut out: Vec<f32> = (0..n).map(|i| (voices[i] + sfx[i] + music[i]) * opts.master_gain).collect();
    let mut report = MixReport { peak_db: dsp::gain_to_db(dsp::peak(&out)), ..Default::default() };
    report.limited = dsp::peak(&out) > 1.0;
    dsp::limit(&mut out, 0.98);
    for s in out.iter_mut() {
        *s = s.clamp(-1.0, 1.0);
    }
    report.loudness_db = dsp::loudness_db(&Audio { sr: SR, samples: out.clone() });
    (out, report)
}
