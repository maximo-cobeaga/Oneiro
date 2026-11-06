//! Provider-independent lipsync. The viseme track is always computed from the final audio
//! (or, when a line has no audio, from its text), so recorded voices, TTS and imported files
//! all behave the same way.

use crate::wav::Audio;
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// Mouth closed (silence, M, B, P).
    Rest,
    /// Wide open (ah).
    A,
    /// Spread (ee, eh).
    E,
    /// Round (oh).
    O,
    /// Small round (oo).
    U,
    /// Teeth together (s, z, sh).
    S,
    /// Lip on teeth (f, v).
    FV,
}

#[derive(Clone, Copy, Debug)]
pub struct Viseme {
    pub shape: Shape,
    /// Mouth opening, 0..1.
    pub open: f32,
}

impl Viseme {
    pub const REST: Viseme = Viseme { shape: Shape::Rest, open: 0.0 };
}

#[derive(Clone, Debug)]
pub struct VisemeTrack {
    pub hop: f64,
    pub frames: Vec<Viseme>,
}

impl VisemeTrack {
    pub fn duration(&self) -> f64 {
        self.frames.len() as f64 * self.hop
    }

    /// Viseme at time `t` (seconds from the start of the audio). Majority vote in a +-20 ms
    /// window keeps the mouth from flickering at video frame rates.
    pub fn at(&self, t: f64) -> Viseme {
        if self.frames.is_empty() || t < 0.0 {
            return Viseme::REST;
        }
        let c = (t / self.hop) as i64;
        let r = ((0.02 / self.hop).round() as i64).max(1);
        let lo = (c - r).max(0) as usize;
        let hi = ((c + r) as usize).min(self.frames.len() - 1);
        if lo > hi {
            return Viseme::REST;
        }
        let mut counts = [0usize; 7];
        let mut open_sum = 0.0f32;
        for f in &self.frames[lo..=hi] {
            counts[f.shape as usize] += 1;
            open_sum += f.open;
        }
        let mut best = 0usize;
        for i in 1..7 {
            if counts[i] > counts[best] {
                best = i;
            }
        }
        let shape = match best {
            0 => Shape::Rest,
            1 => Shape::A,
            2 => Shape::E,
            3 => Shape::O,
            4 => Shape::U,
            5 => Shape::S,
            _ => Shape::FV,
        };
        let open = open_sum / (hi - lo + 1) as f32;
        Viseme { shape, open: if shape == Shape::Rest { 0.0 } else { open } }
    }
}

fn goertzel(x: &[f32], f: f32, sr: f32) -> f32 {
    let k = 2.0 * PI * f / sr;
    let coeff = 2.0 * k.cos();
    let (mut s1, mut s2) = (0.0f32, 0.0f32);
    for &v in x {
        let s = v + coeff * s1 - s2;
        s2 = s1;
        s1 = s;
    }
    (s1 * s1 + s2 * s2 - coeff * s1 * s2).max(0.0)
}

const FREQS: [f32; 11] = [250.0, 400.0, 600.0, 800.0, 1000.0, 1300.0, 1700.0, 2200.0, 2800.0, 3600.0, 4800.0];

/// Computes a viseme track from audio: loudness drives mouth opening, the spectral centroid
/// chooses the mouth shape.
pub fn from_audio(a: &Audio) -> VisemeTrack {
    let hop = 0.01f64;
    let sr = a.sr as f32;
    let hop_n = (a.sr as f64 * hop) as usize;
    let win = (a.sr as f64 * 0.03) as usize;
    let n_frames = a.samples.len() / hop_n.max(1) + 1;

    struct Feat {
        rms: f32,
        centroid: f32,
        hi: f32,
    }
    let mut feats = Vec::with_capacity(n_frames);
    for i in 0..n_frames {
        let center = i * hop_n;
        let lo = center.saturating_sub(win / 2);
        let hi = (center + win / 2).min(a.samples.len());
        if hi <= lo {
            feats.push(Feat { rms: 0.0, centroid: 0.0, hi: 0.0 });
            continue;
        }
        let seg = &a.samples[lo..hi];
        let rms = (seg.iter().map(|v| v * v).sum::<f32>() / seg.len() as f32).sqrt();
        let mut tot = 0.0f32;
        let mut wsum = 0.0f32;
        let mut hi_p = 0.0f32;
        if rms > 1e-4 {
            for &f in FREQS.iter() {
                let p = goertzel(seg, f, sr);
                tot += p;
                wsum += p * f;
                if f >= 2700.0 {
                    hi_p += p;
                }
            }
        }
        let centroid = if tot > 0.0 { wsum / tot } else { 0.0 };
        feats.push(Feat { rms, centroid, hi: if tot > 0.0 { hi_p / tot } else { 0.0 } });
    }

    let mut loud: Vec<f32> = feats.iter().map(|f| f.rms).filter(|r| *r > 1e-3).collect();
    loud.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let reference = if loud.is_empty() { 1.0 } else { loud[(loud.len() as f32 * 0.9) as usize % loud.len()] };

    let mut frames: Vec<Viseme> = Vec::with_capacity(n_frames);
    let mut open_smooth = 0.0f32;
    for f in &feats {
        let db = if f.rms > 1e-5 { 20.0 * (f.rms / reference).log10() } else { -90.0 };
        let open_raw = ((db + 24.0) / 24.0).clamp(0.0, 1.0);
        // Attack fast, release slower.
        let k = if open_raw > open_smooth { 0.6 } else { 0.25 };
        open_smooth += (open_raw - open_smooth) * k;
        let shape = if open_raw < 0.12 {
            Shape::Rest
        } else if f.centroid > 3000.0 && f.hi > 0.45 {
            Shape::S
        } else if f.centroid < 800.0 {
            if open_raw > 0.45 {
                Shape::O
            } else {
                Shape::U
            }
        } else if f.centroid < 1250.0 {
            Shape::A
        } else if open_raw > 0.8 && f.centroid < 1700.0 {
            Shape::A
        } else {
            Shape::E
        };
        let open = match shape {
            Shape::Rest => 0.0,
            Shape::S => open_smooth.min(0.35),
            _ => open_smooth,
        };
        frames.push(Viseme { shape, open });
    }
    VisemeTrack { hop, frames }
}

/// Fallback for lines without audio: spreads the letters of the text evenly over `duration`.
pub fn from_text(text: &str, duration: f64) -> VisemeTrack {
    let hop = 0.01f64;
    let chars: Vec<char> = text.chars().collect();
    let n = (duration / hop).ceil() as usize;
    let mut frames = Vec::with_capacity(n);
    for i in 0..n {
        if chars.is_empty() {
            frames.push(Viseme::REST);
            continue;
        }
        let ci = ((i as f64 / n as f64) * chars.len() as f64) as usize;
        let c = chars[ci.min(chars.len() - 1)].to_ascii_lowercase();
        let v = match c {
            'a' => Viseme { shape: Shape::A, open: 0.9 },
            'e' | 'i' | 'y' => Viseme { shape: Shape::E, open: 0.6 },
            'o' => Viseme { shape: Shape::O, open: 0.75 },
            'u' | 'w' | 'q' => Viseme { shape: Shape::U, open: 0.5 },
            's' | 'z' | 'c' | 'x' | 'j' => Viseme { shape: Shape::S, open: 0.3 },
            'f' | 'v' => Viseme { shape: Shape::FV, open: 0.3 },
            'b' | 'm' | 'p' => Viseme::REST,
            ' ' | '.' | ',' | '!' | '?' | '\'' | '-' => Viseme::REST,
            _ => Viseme { shape: Shape::E, open: 0.35 },
        };
        frames.push(v);
    }
    VisemeTrack { hop, frames }
}
