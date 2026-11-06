//! A tiny formant synthesizer that turns text into speech-like babble. It exists so that the
//! examples have audio to lip-sync to without any external service; real projects use recorded
//! voices or a voice provider.

use std::f32::consts::PI;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        ((self.0 >> 33) as f32) / (u32::MAX >> 1) as f32 // 0..1
    }
    fn noise(&mut self) -> f32 {
        self.next() * 2.0 - 1.0
    }
}

struct Biquad {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}
impl Biquad {
    fn bandpass(f: f32, q: f32, sr: f32) -> Biquad {
        let w0 = 2.0 * PI * f / sr;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Biquad { b0: alpha / a0, b2: -alpha / a0, a1: -2.0 * w0.cos() / a0, a2: (1.0 - alpha) / a0, z1: 0.0, z2: 0.0 }
    }
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = -self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

enum Ph {
    Vowel { f: [f32; 3], dur: f32 },
    Fric { center: f32, dur: f32, voiced: bool },
    Stop { burst: f32, dur: f32 },
    Nasal { dur: f32 },
    Gap { dur: f32 },
}

fn phonemes(text: &str, rng: &mut Rng) -> Vec<Ph> {
    let mut out = Vec::new();
    for c in text.chars() {
        let j = 0.85 + rng.next() * 0.3;
        let ph = match c.to_ascii_lowercase() {
            'a' => Ph::Vowel { f: [800.0, 1200.0, 2500.0], dur: 0.12 * j },
            'e' => Ph::Vowel { f: [500.0, 1900.0, 2600.0], dur: 0.10 * j },
            'i' => Ph::Vowel { f: [300.0, 2300.0, 3000.0], dur: 0.09 * j },
            'o' => Ph::Vowel { f: [500.0, 900.0, 2500.0], dur: 0.12 * j },
            'u' => Ph::Vowel { f: [350.0, 700.0, 2400.0], dur: 0.11 * j },
            'y' => Ph::Vowel { f: [320.0, 2200.0, 2900.0], dur: 0.07 * j },
            's' | 'x' | 'c' => Ph::Fric { center: 5500.0, dur: 0.08 * j, voiced: false },
            'z' => Ph::Fric { center: 5000.0, dur: 0.07 * j, voiced: true },
            'f' => Ph::Fric { center: 3500.0, dur: 0.07 * j, voiced: false },
            'v' => Ph::Fric { center: 3000.0, dur: 0.06 * j, voiced: true },
            'h' => Ph::Fric { center: 1500.0, dur: 0.05 * j, voiced: false },
            'b' | 'p' | 'd' | 't' | 'k' | 'g' | 'q' => Ph::Stop { burst: 1200.0 + rng.next() * 2000.0, dur: 0.05 * j },
            'm' | 'n' | 'l' | 'r' | 'w' | 'j' => Ph::Nasal { dur: 0.07 * j },
            ' ' => Ph::Gap { dur: 0.07 },
            ',' | ';' | ':' => Ph::Gap { dur: 0.18 },
            '.' | '?' | '!' => Ph::Gap { dur: 0.28 },
            _ => continue,
        };
        out.push(ph);
    }
    out
}

/// Synthesizes `text` as mono babble at the given base pitch (Hz).
pub fn synth_speech(text: &str, pitch: f32, sr: u32, seed: u64) -> Vec<f32> {
    let srf = sr as f32;
    let mut rng = Rng(seed ^ 0x9E3779B97F4A7C15);
    let phs = phonemes(text, &mut rng);
    let is_question = text.trim_end().ends_with('?');

    let total_dur: f32 = phs
        .iter()
        .map(|p| match p {
            Ph::Vowel { dur, .. } | Ph::Fric { dur, .. } | Ph::Stop { dur, .. } | Ph::Nasal { dur } | Ph::Gap { dur } => *dur,
        })
        .sum::<f32>()
        .max(0.1);

    let mut out: Vec<f32> = Vec::new();
    let mut phase = 0.0f32;
    let mut time = 0.0f32;
    for ph in &phs {
        let dur = match ph {
            Ph::Vowel { dur, .. } | Ph::Fric { dur, .. } | Ph::Stop { dur, .. } | Ph::Nasal { dur } | Ph::Gap { dur } => *dur,
        };
        let n = (dur * srf) as usize;
        let mut filters: Vec<(Biquad, f32)> = match ph {
            Ph::Vowel { f, .. } => vec![
                (Biquad::bandpass(f[0], 6.0, srf), 1.0),
                (Biquad::bandpass(f[1], 8.0, srf), 0.7),
                (Biquad::bandpass(f[2], 10.0, srf), 0.35),
            ],
            Ph::Nasal { .. } => vec![(Biquad::bandpass(280.0, 5.0, srf), 0.8), (Biquad::bandpass(1100.0, 8.0, srf), 0.25)],
            Ph::Fric { center, .. } => vec![(Biquad::bandpass(*center, 1.5, srf), 1.6)],
            Ph::Stop { burst, .. } => vec![(Biquad::bandpass(*burst, 2.0, srf), 1.8)],
            Ph::Gap { .. } => vec![],
        };
        for i in 0..n {
            let t = time + i as f32 / srf;
            let progress = t / total_dur;
            // Intonation: gentle declination, rising at the end for questions.
            let mut contour = 1.08 - 0.18 * progress;
            if is_question && progress > 0.75 {
                contour += (progress - 0.75) * 1.2;
            }
            let vib = 1.0 + 0.02 * (2.0 * PI * 5.5 * t).sin();
            let f0 = pitch * contour * vib;
            phase = (phase + f0 / srf).fract();
            let saw = 2.0 * phase - 1.0;
            let env_pos = i as f32 / n.max(1) as f32;
            let env = (env_pos * 12.0).min(1.0) * ((1.0 - env_pos) * 8.0).min(1.0);
            let sample = match ph {
                Ph::Vowel { .. } => {
                    let mut s = 0.0;
                    for (bq, g) in filters.iter_mut() {
                        s += bq.process(saw) * *g;
                    }
                    s * 2.2 * env
                }
                Ph::Nasal { .. } => {
                    let mut s = 0.0;
                    for (bq, g) in filters.iter_mut() {
                        s += bq.process(saw) * *g;
                    }
                    s * 1.4 * env
                }
                Ph::Fric { voiced, .. } => {
                    let x = rng.noise();
                    let mut s = 0.0;
                    for (bq, g) in filters.iter_mut() {
                        s += bq.process(x) * *g;
                    }
                    let v = if *voiced { 0.25 * saw } else { 0.0 };
                    (s * 0.7 + v) * env
                }
                Ph::Stop { .. } => {
                    // Silence followed by a short burst.
                    if env_pos > 0.55 {
                        let x = rng.noise();
                        let mut s = 0.0;
                        for (bq, g) in filters.iter_mut() {
                            s += bq.process(x) * *g;
                        }
                        s * (1.0 - env_pos) * 2.5
                    } else {
                        0.0
                    }
                }
                Ph::Gap { .. } => 0.0,
            };
            out.push(sample);
        }
        time += dur;
    }
    let peak = out.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-6);
    for s in out.iter_mut() {
        *s = *s / peak * 0.6;
    }
    out
}

/// A two-tone door bell.
pub fn synth_door_bell(sr: u32) -> Vec<f32> {
    let n = (sr as f32 * 1.4) as usize;
    let mut out = vec![0.0f32; n];
    let tones = [(1318.5f32, 0.0f32), (1046.5f32, 0.28f32)];
    for (f, start) in tones {
        let s0 = (start * sr as f32) as usize;
        for i in s0..n {
            let t = (i - s0) as f32 / sr as f32;
            let env = (-t * 3.2).exp() * (t * 400.0).min(1.0);
            out[i] += (2.0 * PI * f * t).sin() * env * 0.35 + (2.0 * PI * f * 2.76 * t).sin() * env * 0.08;
        }
    }
    out
}

/// A soft thump (door, footstep, etc.).
pub fn synth_thud(sr: u32) -> Vec<f32> {
    let n = (sr as f32 * 0.35) as usize;
    let mut rng = Rng(7);
    (0..n)
        .map(|i| {
            let t = i as f32 / sr as f32;
            let env = (-t * 18.0).exp();
            ((2.0 * PI * 70.0 * t).sin() * 0.7 + rng.noise() * 0.1) * env
        })
        .collect()
}
