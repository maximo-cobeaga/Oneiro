//! Small, dependency-free audio effects used by the sound studio (trim, normalize, filters,
//! fades, waveform thumbnails...). Everything works on mono `f32` buffers.

use crate::wav::Audio;
use std::f32::consts::PI;

pub fn db_to_gain(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

pub fn gain_to_db(g: f32) -> f32 {
    20.0 * g.max(1e-9).log10()
}

pub fn peak(s: &[f32]) -> f32 {
    s.iter().fold(0.0f32, |m, v| m.max(v.abs()))
}

/// RMS level in dBFS over the non-silent part of the signal (gated at -50 dBFS per 50 ms block),
/// so a short voice line with silence around it is not reported as quiet.
pub fn loudness_db(a: &Audio) -> f32 {
    let block = (a.sr as usize / 20).max(1);
    let mut sum = 0.0f64;
    let mut n = 0usize;
    for ch in a.samples.chunks(block) {
        let ms = ch.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / ch.len() as f64;
        if gain_to_db(ms.sqrt() as f32) > -50.0 {
            sum += ms * ch.len() as f64;
            n += ch.len();
        }
    }
    if n == 0 {
        return -90.0;
    }
    gain_to_db((sum / n as f64).sqrt() as f32)
}

pub fn apply_gain(a: &mut Audio, gain: f32) {
    for s in a.samples.iter_mut() {
        *s *= gain;
    }
}

/// Scales so that the highest peak is `target_db` (e.g. -1.0).
pub fn normalize_peak(a: &mut Audio, target_db: f32) {
    let p = peak(&a.samples);
    if p > 1e-6 {
        apply_gain(a, db_to_gain(target_db) / p);
    }
}

/// Scales so that the speech-gated RMS reaches `target_db`, without letting any peak exceed -1 dBFS.
pub fn normalize_loudness(a: &mut Audio, target_db: f32) {
    let l = loudness_db(a);
    if l <= -89.0 {
        return;
    }
    let mut g = db_to_gain(target_db - l);
    let p = peak(&a.samples) * g;
    let ceiling = db_to_gain(-1.0);
    if p > ceiling {
        g *= ceiling / p;
    }
    apply_gain(a, g);
}

/// Removes leading and trailing silence below `threshold_db`, keeping `pad` seconds around the sound.
pub fn trim_silence(a: &Audio, threshold_db: f32, pad: f64) -> Audio {
    let th = db_to_gain(threshold_db);
    let first = a.samples.iter().position(|v| v.abs() > th);
    let last = a.samples.iter().rposition(|v| v.abs() > th);
    match (first, last) {
        (Some(f), Some(l)) => {
            let p = (pad * a.sr as f64) as usize;
            let s = f.saturating_sub(p);
            let e = (l + p + 1).min(a.samples.len());
            Audio { sr: a.sr, samples: a.samples[s..e].to_vec() }
        }
        _ => Audio { sr: a.sr, samples: vec![] },
    }
}

/// Cuts the part between `start` and `end` seconds.
pub fn cut(a: &Audio, start: f64, end: f64) -> Audio {
    let s = ((start.max(0.0)) * a.sr as f64) as usize;
    let e = ((end.max(start)) * a.sr as f64) as usize;
    let s = s.min(a.samples.len());
    let e = e.min(a.samples.len()).max(s);
    Audio { sr: a.sr, samples: a.samples[s..e].to_vec() }
}

pub fn fade_in(a: &mut Audio, secs: f64) {
    let n = ((secs * a.sr as f64) as usize).min(a.samples.len());
    for i in 0..n {
        a.samples[i] *= i as f32 / n as f32;
    }
}

pub fn fade_out(a: &mut Audio, secs: f64) {
    let total = a.samples.len();
    let n = ((secs * a.sr as f64) as usize).min(total);
    for i in 0..n {
        a.samples[total - 1 - i] *= i as f32 / n as f32;
    }
}

/// Removes any DC offset (cheap microphones add one).
pub fn remove_dc(a: &mut Audio) {
    if a.samples.is_empty() {
        return;
    }
    let mean = a.samples.iter().sum::<f32>() / a.samples.len() as f32;
    for s in a.samples.iter_mut() {
        *s -= mean;
    }
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

impl Biquad {
    fn new(b0: f32, b1: f32, b2: f32, a0: f32, a1: f32, a2: f32) -> Biquad {
        Biquad { b0: b0 / a0, b1: b1 / a0, b2: b2 / a0, a1: a1 / a0, a2: a2 / a0, z1: 0.0, z2: 0.0 }
    }
    fn lowpass(f: f32, sr: f32) -> Biquad {
        let w = 2.0 * PI * f / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * 0.707);
        Biquad::new((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }
    fn highpass(f: f32, sr: f32) -> Biquad {
        let w = 2.0 * PI * f / sr;
        let (s, c) = w.sin_cos();
        let alpha = s / (2.0 * 0.707);
        Biquad::new((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + alpha, -2.0 * c, 1.0 - alpha)
    }
    fn process(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

pub fn lowpass(a: &mut Audio, hz: f32) {
    let mut f = Biquad::lowpass(hz.clamp(20.0, a.sr as f32 * 0.45), a.sr as f32);
    for s in a.samples.iter_mut() {
        *s = f.process(*s);
    }
}

pub fn highpass(a: &mut Audio, hz: f32) {
    let mut f = Biquad::highpass(hz.clamp(10.0, a.sr as f32 * 0.45), a.sr as f32);
    for s in a.samples.iter_mut() {
        *s = f.process(*s);
    }
}

/// "Clean voice" chain for raw microphone takes: DC removal, rumble filter, trim, loudness.
pub fn clean_voice(a: &Audio) -> Audio {
    let mut x = a.clone();
    remove_dc(&mut x);
    highpass(&mut x, 80.0);
    let mut x = trim_silence(&x, -45.0, 0.08);
    fade_in(&mut x, 0.01);
    fade_out(&mut x, 0.03);
    normalize_loudness(&mut x, -20.0);
    x
}

/// Changes speed and pitch together by `factor` (2.0 = twice as fast, an octave higher).
pub fn speed(a: &Audio, factor: f64) -> Audio {
    let f = factor.clamp(0.25, 4.0);
    let fake = Audio { sr: (a.sr as f64 * f) as u32, samples: a.samples.clone() };
    let mut r = fake.resampled(a.sr);
    r.sr = a.sr;
    r
}

/// Repeats the audio until it is `secs` long (cutting the last repetition).
pub fn loop_to(a: &Audio, secs: f64) -> Audio {
    let n = (secs * a.sr as f64) as usize;
    if a.samples.is_empty() {
        return Audio { sr: a.sr, samples: vec![0.0; n] };
    }
    let samples = a.samples.iter().cycle().take(n).copied().collect();
    Audio { sr: a.sr, samples }
}

pub fn concat(parts: &[&Audio], gap: f64) -> Audio {
    let sr = parts.first().map_or(crate::wav::SR, |p| p.sr);
    let mut out = Vec::new();
    for (i, p) in parts.iter().enumerate() {
        if i > 0 {
            out.extend(std::iter::repeat(0.0).take((gap * sr as f64) as usize));
        }
        out.extend_from_slice(&p.resampled(sr).samples);
    }
    Audio { sr, samples: out }
}

/// Peak envelope with `n` buckets (0..1), for waveform thumbnails.
pub fn waveform(a: &Audio, n: usize) -> Vec<f32> {
    if n == 0 || a.samples.is_empty() {
        return vec![0.0; n];
    }
    let per = (a.samples.len() as f64 / n as f64).max(1.0);
    (0..n)
        .map(|i| {
            let s = (i as f64 * per) as usize;
            let e = (((i + 1) as f64 * per) as usize).min(a.samples.len()).max(s + 1).min(a.samples.len());
            if s >= a.samples.len() {
                0.0
            } else {
                peak(&a.samples[s..e])
            }
        })
        .collect()
}

/// Soft limiter: transparent below `ceiling`, rounds peaks above it instead of clipping hard.
pub fn limit(samples: &mut [f32], ceiling: f32) {
    let knee = ceiling * 0.8;
    for s in samples.iter_mut() {
        let a = s.abs();
        if a > knee {
            let over = (a - knee) / (ceiling - knee).max(1e-6);
            let shaped = knee + (ceiling - knee) * over.tanh();
            *s = shaped.min(ceiling) * s.signum();
        }
    }
}
