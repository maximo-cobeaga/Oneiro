//! Speaker playback and microphone recording (cpal). Only compiled with the `device` feature.

use crate::wav::Audio;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample, SampleFormat, SizedSample};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

// ------------------------------------------------------------------ playback

struct PlayState {
    samples: Mutex<Arc<Vec<f32>>>,
    /// Position in samples at the device sample rate.
    pos: AtomicUsize,
    playing: AtomicBool,
}

pub struct Player {
    _stream: cpal::Stream,
    state: Arc<PlayState>,
    pub device_sr: u32,
    pub device_name: String,
}

impl Player {
    pub fn new() -> Result<Player, String> {
        let host = cpal::default_host();
        let dev = host.default_output_device().ok_or("no audio output device found")?;
        let cfg = dev.default_output_config().map_err(|e| format!("output device: {}", e))?;
        let fmt = cfg.sample_format();
        let config: cpal::StreamConfig = cfg.into();
        let channels = config.channels as usize;
        let state = Arc::new(PlayState { samples: Mutex::new(Arc::new(vec![])), pos: AtomicUsize::new(0), playing: AtomicBool::new(false) });
        let st = state.clone();
        let stream = match fmt {
            SampleFormat::F32 => build_out::<f32>(&dev, &config, channels, st),
            SampleFormat::I16 => build_out::<i16>(&dev, &config, channels, st),
            SampleFormat::U16 => build_out::<u16>(&dev, &config, channels, st),
            other => Err(format!("unsupported output sample format {:?}", other)),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Player { _stream: stream, state, device_sr: config.sample_rate.0, device_name: dev.name().unwrap_or_default() })
    }

    /// Replaces the audio to play (converted to the device rate). Keeps the position.
    pub fn load(&self, a: &Audio) {
        let r = a.resampled(self.device_sr);
        *self.state.samples.lock().unwrap() = Arc::new(r.samples);
    }

    pub fn play_from(&self, secs: f64) {
        self.state.pos.store((secs.max(0.0) * self.device_sr as f64) as usize, Ordering::SeqCst);
        self.state.playing.store(true, Ordering::SeqCst);
    }

    pub fn pause(&self) {
        self.state.playing.store(false, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.pause();
        self.state.pos.store(0, Ordering::SeqCst);
    }

    pub fn is_playing(&self) -> bool {
        self.state.playing.load(Ordering::SeqCst)
    }

    pub fn position(&self) -> f64 {
        self.state.pos.load(Ordering::SeqCst) as f64 / self.device_sr as f64
    }

    /// Plays `a` once from the start (a preview), replacing whatever was loaded.
    pub fn play_clip(&self, a: &Audio) {
        self.pause();
        self.load(a);
        self.play_from(0.0);
    }
}

fn build_out<T>(dev: &cpal::Device, config: &cpal::StreamConfig, channels: usize, st: Arc<PlayState>) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let mut cached: Arc<Vec<f32>> = Arc::new(vec![]);
    dev.build_output_stream(
        config,
        move |data: &mut [T], _| {
            if let Ok(g) = st.samples.try_lock() {
                if !Arc::ptr_eq(&g, &cached) {
                    cached = g.clone();
                }
            }
            let playing = st.playing.load(Ordering::Relaxed);
            let mut pos = st.pos.load(Ordering::Relaxed);
            for frame in data.chunks_mut(channels) {
                let v = if playing && pos < cached.len() {
                    let s = cached[pos];
                    pos += 1;
                    s
                } else {
                    0.0
                };
                for out in frame.iter_mut() {
                    *out = T::from_sample(v);
                }
            }
            if playing {
                st.pos.store(pos, Ordering::Relaxed);
                if pos >= cached.len() {
                    st.playing.store(false, Ordering::Relaxed);
                }
            }
        },
        |e| eprintln!("audio output error: {}", e),
        None,
    )
    .map_err(|e| format!("cannot open the output device: {}", e))
}

// ------------------------------------------------------------------ recording

pub struct Recorder {
    _stream: cpal::Stream,
    buf: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    pub sr: u32,
    pub device_name: String,
    started: Instant,
}

impl Recorder {
    pub fn start() -> Result<Recorder, String> {
        Recorder::start_named(None)
    }

    /// Starts recording from the input device called `name` (`None`: the system default).
    pub fn start_named(name: Option<&str>) -> Result<Recorder, String> {
        let host = cpal::default_host();
        let dev = match name {
            Some(n) => host
                .input_devices()
                .map_err(|e| e.to_string())?
                .find(|d| d.name().map_or(false, |x| x == n))
                .ok_or_else(|| format!("microphone '{}' not found (unplugged?)", n))?,
            None => host.default_input_device().ok_or("no microphone found (check Windows sound settings and privacy permissions)")?,
        };
        let cfg = dev.default_input_config().map_err(|e| format!("microphone: {}", e))?;
        let fmt = cfg.sample_format();
        let config: cpal::StreamConfig = cfg.into();
        let channels = config.channels as usize;
        let buf = Arc::new(Mutex::new(Vec::<f32>::new()));
        let level = Arc::new(AtomicU32::new(0));
        let stream = match fmt {
            SampleFormat::F32 => build_in::<f32>(&dev, &config, channels, buf.clone(), level.clone()),
            SampleFormat::I16 => build_in::<i16>(&dev, &config, channels, buf.clone(), level.clone()),
            SampleFormat::U16 => build_in::<u16>(&dev, &config, channels, buf.clone(), level.clone()),
            other => Err(format!("unsupported microphone sample format {:?}", other)),
        }?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(Recorder { _stream: stream, buf, level, sr: config.sample_rate.0, device_name: dev.name().unwrap_or_default(), started: Instant::now() })
    }

    /// Current input peak, 0..1 (for the level meter).
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    pub fn seconds(&self) -> f64 {
        self.buf.lock().map(|b| b.len() as f64 / self.sr as f64).unwrap_or(0.0)
    }

    pub fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }

    pub fn stop(self) -> Audio {
        let samples = self.buf.lock().map(|b| b.clone()).unwrap_or_default();
        Audio { sr: self.sr, samples }
    }
}

fn build_in<T>(dev: &cpal::Device, config: &cpal::StreamConfig, channels: usize, buf: Arc<Mutex<Vec<f32>>>, level: Arc<AtomicU32>) -> Result<cpal::Stream, String>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    dev.build_input_stream(
        config,
        move |data: &[T], _| {
            let mut peak = 0.0f32;
            if let Ok(mut b) = buf.lock() {
                for frame in data.chunks(channels) {
                    let mut sum = 0.0;
                    for s in frame {
                        sum += f32::from_sample(*s);
                    }
                    let m = sum / frame.len() as f32;
                    peak = peak.max(m.abs());
                    b.push(m);
                }
            }
            // Smooth decay so the meter is readable.
            let old = f32::from_bits(level.load(Ordering::Relaxed));
            level.store(peak.max(old * 0.85).to_bits(), Ordering::Relaxed);
        },
        |e| eprintln!("microphone error: {}", e),
        None,
    )
    .map_err(|e| format!("cannot open the microphone: {}", e))
}

/// Names of the available input devices.
pub fn input_devices() -> Vec<String> {
    cpal::default_host().input_devices().map(|d| d.filter_map(|x| x.name().ok()).collect()).unwrap_or_default()
}

pub fn output_devices() -> Vec<String> {
    cpal::default_host().output_devices().map(|d| d.filter_map(|x| x.name().ok()).collect()).unwrap_or_default()
}

// `Sample` is imported for the trait bounds of `FromSample`.
#[allow(dead_code)]
fn _assert_sample<T: Sample>() {}
