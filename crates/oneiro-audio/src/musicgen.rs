//! Procedural background music: short seamless loops (chords, bass, melody, light drums)
//! synthesized from a mood, a tempo and a seed. Meant as royalty-free temp/ambient tracks.

use crate::dsp;
use crate::wav::{Audio, SR};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mood {
    Cheerful,
    Calm,
    Tense,
    Quirky,
    Sad,
}

impl Mood {
    pub const ALL: [Mood; 5] = [Mood::Cheerful, Mood::Calm, Mood::Tense, Mood::Quirky, Mood::Sad];

    pub fn id(self) -> &'static str {
        match self {
            Mood::Cheerful => "cheerful",
            Mood::Calm => "calm",
            Mood::Tense => "tense",
            Mood::Quirky => "quirky",
            Mood::Sad => "sad",
        }
    }

    pub fn from_id(s: &str) -> Option<Mood> {
        Mood::ALL.into_iter().find(|m| m.id() == s.to_lowercase())
    }

    pub fn default_bpm(self) -> f32 {
        match self {
            Mood::Cheerful => 120.0,
            Mood::Calm => 70.0,
            Mood::Tense => 96.0,
            Mood::Quirky => 132.0,
            Mood::Sad => 62.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MusicSpec {
    pub mood: Mood,
    pub bpm: f32,
    pub bars: usize,
    pub seed: u64,
    /// Transposition in semitones from C.
    pub key: i32,
}

impl MusicSpec {
    pub fn new(mood: Mood, seed: u64) -> MusicSpec {
        MusicSpec { mood, bpm: mood.default_bpm(), bars: 8, seed, key: 0 }
    }
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
    fn chance(&mut self, p: f32) -> bool {
        (self.next() % 1000) as f32 / 1000.0 < p
    }
    fn noise(&mut self) -> f32 {
        (self.next() as f32 / (u32::MAX >> 1) as f32) * 2.0 - 1.0
    }
}

fn freq(midi: f32) -> f32 {
    440.0 * 2f32.powf((midi - 69.0) / 12.0)
}

#[derive(Clone, Copy)]
enum Inst {
    Pluck,
    Pad,
    Bass,
    Bell,
}

fn note(out: &mut [f32], start_s: f32, dur: f32, midi: f32, amp: f32, inst: Inst) {
    let f = freq(midi);
    let s0 = (start_s * SR as f32) as usize;
    let n = ((dur + 0.4) * SR as f32) as usize;
    for k in 0..n {
        let i = s0 + k;
        if i >= out.len() {
            break;
        }
        let t = k as f32 / SR as f32;
        let (env, v) = match inst {
            Inst::Pluck => {
                let e = (-t * 7.0).exp();
                (e, (2.0 * PI * f * t).sin() + 0.5 * (2.0 * PI * 2.0 * f * t).sin() * (-t * 14.0).exp() + 0.25 * (2.0 * PI * 3.0 * f * t).sin() * (-t * 20.0).exp())
            }
            Inst::Bell => {
                let e = (-t * 4.0).exp();
                (e, (2.0 * PI * f * t).sin() + 0.4 * (2.0 * PI * 2.76 * f * t).sin() * (-t * 9.0).exp())
            }
            Inst::Pad => {
                let a = (t / 0.25).min(1.0);
                let r = if t > dur { (1.0 - (t - dur) / 0.4).max(0.0) } else { 1.0 };
                let det = 1.004;
                (a * r, ((2.0 * PI * f * t).sin() + (2.0 * PI * f * det * t).sin() + 0.3 * (2.0 * PI * 2.0 * f * t).sin()) * 0.5)
            }
            Inst::Bass => {
                let a = (t / 0.01).min(1.0);
                let r = if t > dur { (1.0 - (t - dur) / 0.2).max(0.0) } else { 1.0 };
                (a * r * (-t * 1.6).exp().max(0.35), (2.0 * PI * f * t).sin() + 0.35 * (2.0 * PI * 2.0 * f * t).sin())
            }
        };
        out[i] += v * env * amp;
    }
}

fn kick(out: &mut [f32], start_s: f32, amp: f32) {
    let s0 = (start_s * SR as f32) as usize;
    let mut phase = 0.0f32;
    for k in 0..(0.25 * SR as f32) as usize {
        if s0 + k >= out.len() {
            break;
        }
        let t = k as f32 / SR as f32;
        let f = 45.0 + 110.0 * (-t * 28.0).exp();
        phase += 2.0 * PI * f / SR as f32;
        out[s0 + k] += phase.sin() * (-t * 11.0).exp() * amp;
    }
}

fn hat(out: &mut [f32], start_s: f32, amp: f32, rng: &mut Rng) {
    let s0 = (start_s * SR as f32) as usize;
    let mut prev = 0.0f32;
    for k in 0..(0.05 * SR as f32) as usize {
        if s0 + k >= out.len() {
            break;
        }
        let t = k as f32 / SR as f32;
        let n = rng.noise();
        let hp = n - prev; // crude high-pass
        prev = n;
        out[s0 + k] += hp * (-t * 90.0).exp() * amp;
    }
}

const MAJOR: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];
const MINOR: [i32; 7] = [0, 2, 3, 5, 7, 8, 10];

/// Generates a loop. Always `bars * 4` beats long (4/4); the tail of the last notes wraps to the start.
pub fn generate(spec: &MusicSpec) -> Audio {
    let mut rng = Rng(spec.seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0x1234_5678_9ABC_DEF0);
    let beat = 60.0 / spec.bpm.max(30.0);
    let bars = spec.bars.clamp(1, 64);
    let total = bars as f32 * 4.0 * beat;
    let tail = 0.5;
    let mut out = vec![0.0f32; ((total + tail) * SR as f32) as usize];

    let (scale, progs): (&[i32; 7], &[&[usize]]) = match spec.mood {
        Mood::Cheerful => (&MAJOR, &[&[0, 4, 5, 3], &[0, 3, 4, 4], &[0, 5, 3, 4]]),
        Mood::Calm => (&MAJOR, &[&[0, 3, 0, 4], &[0, 5, 3, 0]]),
        Mood::Tense => (&MINOR, &[&[0, 5, 0, 6], &[0, 0, 5, 4]]),
        Mood::Quirky => (&MAJOR, &[&[0, 2, 3, 4], &[0, 5, 1, 4]]),
        Mood::Sad => (&MINOR, &[&[0, 5, 2, 6], &[0, 3, 5, 4]]),
    };
    let prog = progs[rng.below(progs.len() as u32) as usize];
    let root = 48 + spec.key.rem_euclid(12);
    let deg = |d: usize, octave: i32| -> f32 {
        let o = (d / 7) as i32;
        (root + octave * 12 + o * 12 + scale[d % 7]) as f32
    };

    // Melody: a random walk on the scale, with a motif repeated every two bars and varied slowly.
    let motif_len = 8; // eighth notes per bar
    let mut motif: Vec<Option<i32>> = Vec::new();
    let mut cur = 7 + rng.below(3) as i32;
    for step in 0..motif_len {
        let rest_p = match spec.mood {
            Mood::Calm | Mood::Sad => 0.5,
            Mood::Tense => 0.4,
            _ => 0.25,
        };
        if step % 4 != 0 && rng.chance(rest_p) {
            motif.push(None);
        } else {
            cur = (cur + rng.below(5) as i32 - 2).clamp(5, 13);
            motif.push(Some(cur));
        }
    }

    for bar in 0..bars {
        let t0 = bar as f32 * 4.0 * beat;
        let chord = prog[bar % prog.len()];
        // Pad chord (root, third, fifth, one octave up).
        let pad_amp = match spec.mood {
            Mood::Calm | Mood::Sad => 0.16,
            _ => 0.10,
        };
        for off in [0usize, 2, 4] {
            note(&mut out, t0, 4.0 * beat, deg(chord + off, 1), pad_amp, Inst::Pad);
        }
        // Bass: root on the beat, with the fifth on the "and" of 2 for movement.
        match spec.mood {
            Mood::Calm | Mood::Sad => note(&mut out, t0, 3.8 * beat, deg(chord, 0), 0.30, Inst::Bass),
            Mood::Tense => {
                for k in 0..8 {
                    note(&mut out, t0 + k as f32 * 0.5 * beat, 0.4 * beat, deg(chord, 0), 0.22, Inst::Bass);
                }
            }
            _ => {
                for b in 0..4 {
                    let d = if b == 2 && rng.chance(0.5) { chord + 4 } else { chord };
                    note(&mut out, t0 + b as f32 * beat, 0.8 * beat, deg(d, 0), 0.28, Inst::Bass);
                }
            }
        }
        // Melody: motif, transposed to follow the chord, an octave of pluck or bell.
        let inst = if matches!(spec.mood, Mood::Calm | Mood::Sad) { Inst::Bell } else { Inst::Pluck };
        for (k, n) in motif.iter().enumerate() {
            if let Some(d) = n {
                // Vary every other bar so the loop doesn't feel mechanical.
                let shift = if bar % 2 == 1 && k >= 4 { 1 } else { 0 };
                let degree = (*d + shift) as usize;
                let adj = if k % 4 == 0 { chord } else { 0 };
                let midi = deg((degree + adj / 2).min(13), 0) + if matches!(spec.mood, Mood::Calm | Mood::Sad) { 12.0 } else { 0.0 };
                note(&mut out, t0 + k as f32 * 0.5 * beat, 0.9 * beat, midi, 0.22, inst);
            }
        }
        // Drums for the livelier moods.
        if matches!(spec.mood, Mood::Cheerful | Mood::Quirky | Mood::Tense) {
            for b in 0..4 {
                if b % 2 == 0 || spec.mood == Mood::Quirky {
                    kick(&mut out, t0 + b as f32 * beat, 0.5);
                }
                hat(&mut out, t0 + (b as f32 + 0.5) * beat, 0.10, &mut rng);
            }
        }
    }

    // Wrap the tail around so the loop is seamless.
    let body = (total * SR as f32) as usize;
    let (head, tl) = out.split_at_mut(body);
    for (i, v) in tl.iter().enumerate() {
        if i < head.len() {
            head[i] += *v;
        }
    }
    out.truncate(body);
    let mut a = Audio { sr: SR, samples: out };
    dsp::lowpass(&mut a, 9000.0);
    dsp::normalize_peak(&mut a, -3.0);
    a
}
