//! Procedural sound effects: a built-in library of cartoon-style sounds synthesized from scratch,
//! so a project has usable SFX with no external assets (and no licensing questions).

use crate::dsp;
use crate::wav::{Audio, SR};
use std::f32::consts::PI;

/// (name, description) of every preset.
pub const PRESETS: &[(&str, &str)] = &[
    ("door_bell", "two-tone door chime"),
    ("ding", "single bright bell"),
    ("knock", "three knocks on wood"),
    ("thud", "heavy low impact"),
    ("punch", "cartoon punch impact"),
    ("click", "short mechanical click"),
    ("pop", "bubble pop"),
    ("whoosh", "fast air sweep"),
    ("boing", "springy bounce"),
    ("buzzer", "wrong-answer buzzer"),
    ("splash", "water splash"),
    ("crash", "glass/metal crash"),
    ("footstep", "single footstep"),
    ("sparkle", "magic sparkle"),
    ("wah_wah", "sad trombone"),
    ("slide_up", "slide whistle going up"),
    ("slide_down", "slide whistle going down"),
    ("honk", "clown horn"),
    ("zap", "electric zap"),
    ("coin", "pick-up coin"),
    ("phone_ring", "old telephone ring"),
    ("clock_tick", "clock tick-tock"),
    ("scratch", "record scratch"),
    ("applause", "small crowd applause"),
];

pub fn names() -> Vec<&'static str> {
    PRESETS.iter().map(|p| p.0).collect()
}

struct Rng(u64);
impl Rng {
    fn noise(&mut self) -> f32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (((self.0 >> 33) as f32) / (u32::MAX >> 1) as f32) * 2.0 - 1.0
    }
}

fn buf(secs: f32) -> Vec<f32> {
    vec![0.0; (secs * SR as f32) as usize]
}

/// Adds a decaying sine partial.
fn partial(out: &mut [f32], start: f32, freq: f32, amp: f32, decay: f32) {
    let s0 = (start * SR as f32) as usize;
    for i in s0..out.len() {
        let t = (i - s0) as f32 / SR as f32;
        let env = (-t * decay).exp();
        if env < 0.001 {
            break;
        }
        out[i] += (2.0 * PI * freq * t).sin() * amp * env;
    }
}

fn bell(out: &mut [f32], start: f32, f: f32, amp: f32, decay: f32) {
    for (ratio, a) in [(1.0, 1.0), (2.0, 0.45), (2.76, 0.3), (5.4, 0.12)] {
        partial(out, start, f * ratio, amp * a, decay * (1.0 + ratio * 0.15));
    }
}

fn noise_burst(rng: &mut Rng, out: &mut [f32], start: f32, dur: f32, amp: f32, decay: f32) {
    let s0 = (start * SR as f32) as usize;
    let n = (dur * SR as f32) as usize;
    for k in 0..n {
        if s0 + k >= out.len() {
            break;
        }
        let t = k as f32 / SR as f32;
        out[s0 + k] += rng.noise() * amp * (-t * decay).exp();
    }
}

/// Frequency-swept oscillator; `f(t)` gives the frequency at normalized time 0..1.
fn sweep(out: &mut [f32], start: f32, dur: f32, amp: f32, f: impl Fn(f32) -> f32, square: bool) {
    let s0 = (start * SR as f32) as usize;
    let n = (dur * SR as f32) as usize;
    let mut phase = 0.0f32;
    for k in 0..n {
        if s0 + k >= out.len() {
            break;
        }
        let u = k as f32 / n as f32;
        phase += 2.0 * PI * f(u) / SR as f32;
        let env = (u * 30.0).min(1.0) * (1.0 - u).powf(0.5);
        let v = if square { phase.sin().signum() * 0.6 + phase.sin() * 0.4 } else { phase.sin() };
        out[s0 + k] += v * amp * env;
    }
}

fn finish(samples: Vec<f32>, peak_db: f32) -> Audio {
    let mut a = Audio { sr: SR, samples };
    dsp::fade_in(&mut a, 0.002);
    dsp::fade_out(&mut a, 0.01);
    dsp::normalize_peak(&mut a, peak_db);
    a
}

/// Generates a preset. `seed` varies the noise and small pitch details.
pub fn generate(name: &str, seed: u64) -> Option<Audio> {
    let mut rng = Rng(seed.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xA5A5_5A5A);
    let a = match name {
        "door_bell" => {
            let mut o = buf(1.9);
            bell(&mut o, 0.0, 784.0, 0.6, 2.6);
            bell(&mut o, 0.45, 622.0, 0.6, 2.2);
            finish(o, -4.0)
        }
        "ding" => {
            let mut o = buf(1.4);
            bell(&mut o, 0.0, 1318.0, 0.7, 3.0);
            finish(o, -4.0)
        }
        "knock" => {
            let mut o = buf(0.9);
            for k in 0..3 {
                let t = 0.05 + k as f32 * 0.27;
                partial(&mut o, t, 190.0, 0.8, 32.0);
                partial(&mut o, t, 420.0, 0.35, 45.0);
                noise_burst(&mut rng, &mut o, t, 0.02, 0.3, 120.0);
            }
            let mut a = Audio { sr: SR, samples: o };
            dsp::lowpass(&mut a, 2500.0);
            finish(a.samples, -3.0)
        }
        "thud" => {
            let mut o = buf(0.7);
            sweep(&mut o, 0.0, 0.5, 0.9, |u| 110.0 - 70.0 * u, false);
            noise_burst(&mut rng, &mut o, 0.0, 0.05, 0.3, 60.0);
            let mut a = Audio { sr: SR, samples: o };
            dsp::lowpass(&mut a, 600.0);
            finish(a.samples, -2.0)
        }
        "punch" => {
            let mut o = buf(0.5);
            sweep(&mut o, 0.0, 0.25, 0.9, |u| 180.0 - 120.0 * u, false);
            noise_burst(&mut rng, &mut o, 0.0, 0.12, 0.8, 38.0);
            let mut a = Audio { sr: SR, samples: o };
            dsp::lowpass(&mut a, 3500.0);
            finish(a.samples, -2.0)
        }
        "click" => {
            let mut o = buf(0.12);
            noise_burst(&mut rng, &mut o, 0.0, 0.01, 0.8, 500.0);
            partial(&mut o, 0.0, 2400.0, 0.5, 250.0);
            finish(o, -6.0)
        }
        "pop" => {
            let mut o = buf(0.25);
            sweep(&mut o, 0.0, 0.08, 0.9, |u| 900.0 - 600.0 * u, false);
            noise_burst(&mut rng, &mut o, 0.0, 0.02, 0.2, 200.0);
            finish(o, -4.0)
        }
        "whoosh" => {
            let mut a = Audio { sr: SR, samples: buf(0.8) };
            let n = a.samples.len();
            for (i, s) in a.samples.iter_mut().enumerate() {
                let u = i as f32 / n as f32;
                *s = rng.noise() * (PI * u).sin().powf(1.5);
            }
            // Fake a sweeping band by mixing a low-passed and high-passed copy over time.
            let mut lo = a.clone();
            dsp::lowpass(&mut lo, 900.0);
            let mut hi = a.clone();
            dsp::highpass(&mut hi, 2500.0);
            for i in 0..n {
                let u = i as f32 / n as f32;
                a.samples[i] = lo.samples[i] * (1.0 - u) + hi.samples[i] * u * 0.7;
            }
            finish(a.samples, -4.0)
        }
        "boing" => {
            let mut o = buf(0.8);
            sweep(&mut o, 0.0, 0.75, 0.8, |u| 260.0 + 220.0 * (u * 20.0).sin() * (1.0 - u) + 120.0 * (1.0 - u), false);
            finish(o, -4.0)
        }
        "buzzer" => {
            let mut o = buf(0.7);
            sweep(&mut o, 0.0, 0.65, 0.7, |_| 120.0, true);
            sweep(&mut o, 0.0, 0.65, 0.5, |_| 123.0, true);
            let mut a = Audio { sr: SR, samples: o };
            for s in a.samples.iter_mut() {
                *s = s.clamp(-0.5, 0.5);
            }
            finish(a.samples, -5.0)
        }
        "splash" => {
            let mut o = buf(1.1);
            noise_burst(&mut rng, &mut o, 0.0, 0.9, 0.9, 5.0);
            for k in 0..6 {
                let t = 0.05 + k as f32 * 0.07;
                sweep(&mut o, t, 0.06, 0.25, |u| 500.0 + 900.0 * u + k as f32 * 90.0, false);
            }
            let mut a = Audio { sr: SR, samples: o };
            dsp::highpass(&mut a, 400.0);
            dsp::lowpass(&mut a, 6500.0);
            finish(a.samples, -4.0)
        }
        "crash" => {
            let mut o = buf(1.6);
            noise_burst(&mut rng, &mut o, 0.0, 1.5, 0.8, 3.2);
            for (k, f) in [2150.0, 3010.0, 4330.0, 5520.0].iter().enumerate() {
                partial(&mut o, k as f32 * 0.03, *f, 0.25, 4.0);
            }
            let mut a = Audio { sr: SR, samples: o };
            dsp::highpass(&mut a, 600.0);
            finish(a.samples, -3.0)
        }
        "footstep" => {
            let mut o = buf(0.3);
            partial(&mut o, 0.0, 85.0, 0.9, 38.0);
            noise_burst(&mut rng, &mut o, 0.0, 0.05, 0.5, 70.0);
            let mut a = Audio { sr: SR, samples: o };
            dsp::lowpass(&mut a, 900.0);
            finish(a.samples, -4.0)
        }
        "sparkle" => {
            let mut o = buf(1.3);
            for k in 0..9 {
                let f = 1500.0 * 2f32.powf((k as f32 * 3.0 % 12.0) / 12.0) * (1.0 + (k / 4) as f32);
                bell(&mut o, k as f32 * 0.07, f.min(7000.0), 0.25, 6.0);
            }
            finish(o, -5.0)
        }
        "wah_wah" => {
            let mut o = buf(2.4);
            for (k, (f, d)) in [(233.0, 0.5), (220.0, 0.5), (208.0, 0.5), (196.0, 0.9)].iter().enumerate() {
                let t0 = [0.0, 0.5, 1.0, 1.5][k];
                let wob = if k == 3 { 6.0 } else { 0.0 };
                sweep(&mut o, t0, *d, 0.5, move |u| f - 6.0 * u + wob * (u * 40.0).sin(), true);
            }
            let mut a = Audio { sr: SR, samples: o };
            dsp::lowpass(&mut a, 1400.0);
            finish(a.samples, -5.0)
        }
        "slide_up" => {
            let mut o = buf(0.9);
            sweep(&mut o, 0.0, 0.85, 0.8, |u| 500.0 + 1700.0 * u * u + 18.0 * (u * 90.0).sin(), false);
            finish(o, -5.0)
        }
        "slide_down" => {
            let mut o = buf(0.9);
            sweep(&mut o, 0.0, 0.85, 0.8, |u| 2100.0 - 1650.0 * u.sqrt() + 18.0 * (u * 90.0).sin(), false);
            finish(o, -5.0)
        }
        "honk" => {
            let mut o = buf(0.5);
            sweep(&mut o, 0.0, 0.4, 0.7, |_| 330.0, true);
            sweep(&mut o, 0.0, 0.4, 0.5, |_| 392.0, true);
            let mut a = Audio { sr: SR, samples: o };
            dsp::lowpass(&mut a, 2600.0);
            finish(a.samples, -4.0)
        }
        "zap" => {
            let mut o = buf(0.5);
            sweep(&mut o, 0.0, 0.35, 0.7, |u| 2800.0 * (1.0 - u).powi(2) + 150.0, true);
            noise_burst(&mut rng, &mut o, 0.0, 0.3, 0.25, 12.0);
            finish(o, -4.0)
        }
        "coin" => {
            let mut o = buf(0.6);
            sweep(&mut o, 0.0, 0.07, 0.6, |_| 988.0, true);
            sweep(&mut o, 0.07, 0.4, 0.6, |_| 1319.0, true);
            finish(o, -6.0)
        }
        "phone_ring" => {
            let mut o = buf(2.6);
            for burst in 0..2 {
                let t0 = burst as f32 * 1.2;
                let n = (0.8 * SR as f32) as usize;
                let s0 = (t0 * SR as f32) as usize;
                for k in 0..n {
                    let t = k as f32 / SR as f32;
                    let gate = if (t * 22.0) as i32 % 2 == 0 { 1.0 } else { 0.0 };
                    let v = (2.0 * PI * 1400.0 * t).sin() * 0.5 + (2.0 * PI * 2000.0 * t).sin() * 0.5;
                    if s0 + k < o.len() {
                        o[s0 + k] += v * gate * 0.6;
                    }
                }
            }
            finish(o, -5.0)
        }
        "clock_tick" => {
            let mut o = buf(1.1);
            for (k, f) in [1800.0, 1300.0].iter().enumerate() {
                let t = k as f32 * 0.5;
                noise_burst(&mut rng, &mut o, t, 0.012, 0.7, 400.0);
                partial(&mut o, t, *f, 0.5, 110.0);
            }
            finish(o, -6.0)
        }
        "scratch" => {
            let mut a = Audio { sr: SR, samples: buf(0.55) };
            let n = a.samples.len();
            for (i, s) in a.samples.iter_mut().enumerate() {
                let u = i as f32 / n as f32;
                let f = 300.0 + 2500.0 * (1.0 - u) * (u * 9.0).sin().abs();
                *s = (rng.noise() * 0.5 + (2.0 * PI * f * i as f32 / SR as f32).sin() * 0.5) * (1.0 - u).powf(0.3);
            }
            dsp::highpass(&mut a, 300.0);
            finish(a.samples, -4.0)
        }
        "applause" => {
            let mut a = Audio { sr: SR, samples: buf(2.5) };
            for _ in 0..170 {
                let t = (rng.noise().abs() * 2.2).min(2.3);
                let mut tmp = buf(0.06);
                noise_burst(&mut rng, &mut tmp, 0.0, 0.05, 0.6, 70.0);
                let s0 = (t * SR as f32) as usize;
                for (k, v) in tmp.iter().enumerate() {
                    if s0 + k < a.samples.len() {
                        a.samples[s0 + k] += v;
                    }
                }
            }
            let n = a.samples.len();
            for (i, s) in a.samples.iter_mut().enumerate() {
                let u = i as f32 / n as f32;
                *s *= (u * 8.0).min(1.0) * (1.0 - u).min(0.3) / 0.3;
            }
            dsp::highpass(&mut a, 700.0);
            dsp::lowpass(&mut a, 7500.0);
            finish(a.samples, -6.0)
        }
        _ => return None,
    };
    Some(a)
}
