use oneiro_audio::import::{self, ImportKind};
use oneiro_audio::mix::{self, Event, Kind, MixOptions};
use oneiro_audio::musicgen::{self, Mood, MusicSpec};
use oneiro_audio::{dsp, sfxgen, write_wav, Audio, SR};

fn tone(freq: f32, secs: f32, amp: f32) -> Audio {
    let n = (secs * SR as f32) as usize;
    Audio { sr: SR, samples: (0..n).map(|i| (2.0 * std::f32::consts::PI * freq * i as f32 / SR as f32).sin() * amp).collect() }
}

fn rms(s: &[f32]) -> f32 {
    (s.iter().map(|v| v * v).sum::<f32>() / s.len().max(1) as f32).sqrt()
}

#[test]
fn every_sfx_preset_generates_audible_bounded_audio() {
    for (name, _) in sfxgen::PRESETS {
        let a = sfxgen::generate(name, 1).unwrap_or_else(|| panic!("preset {} missing", name));
        assert!(a.duration() > 0.05 && a.duration() < 4.0, "{} lasts {}", name, a.duration());
        let p = dsp::peak(&a.samples);
        assert!(p > 0.3 && p <= 1.0, "{} peak {}", name, p);
        assert!(a.samples.iter().all(|v| v.is_finite()), "{} has NaN", name);
    }
    assert!(sfxgen::generate("no_such_sound", 1).is_none());
}

#[test]
fn sfx_generation_is_deterministic_per_seed() {
    let a = sfxgen::generate("splash", 7).unwrap();
    let b = sfxgen::generate("splash", 7).unwrap();
    let c = sfxgen::generate("splash", 8).unwrap();
    assert_eq!(a.samples, b.samples);
    assert_ne!(a.samples, c.samples);
}

#[test]
fn music_loops_are_the_right_length_and_seamless() {
    for mood in Mood::ALL {
        let spec = MusicSpec { bars: 4, ..MusicSpec::new(mood, 3) };
        let a = musicgen::generate(&spec);
        let expect = 4.0 * 4.0 * 60.0 / spec.bpm as f64;
        assert!((a.duration() - expect).abs() < 0.01, "{:?}: {} vs {}", mood, a.duration(), expect);
        assert!(dsp::peak(&a.samples) > 0.5, "{:?} is silent", mood);
        // Seamless: the jump from the last sample to the first is no bigger than normal sample-to-sample motion.
        let jump = (a.samples[0] - a.samples[a.samples.len() - 1]).abs();
        let max_step = a.samples.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
        assert!(jump <= max_step * 2.0 + 0.05, "{:?}: loop click {} (max step {})", mood, jump, max_step);
    }
    let x = musicgen::generate(&MusicSpec::new(Mood::Cheerful, 1));
    let y = musicgen::generate(&MusicSpec::new(Mood::Cheerful, 2));
    assert_ne!(x.samples, y.samples, "different seeds should give different tunes");
}

#[test]
fn normalizing_hits_the_requested_levels() {
    let mut a = tone(440.0, 1.0, 0.05);
    dsp::normalize_peak(&mut a, -6.0);
    assert!((dsp::gain_to_db(dsp::peak(&a.samples)) + 6.0).abs() < 0.1);

    let mut b = tone(300.0, 1.0, 0.02);
    dsp::normalize_loudness(&mut b, -20.0);
    assert!((dsp::loudness_db(&b) + 20.0).abs() < 0.6, "got {}", dsp::loudness_db(&b));
}

#[test]
fn trim_silence_keeps_the_sound_and_a_little_padding() {
    let mut s = vec![0.0; SR as usize];
    s.extend(tone(500.0, 0.5, 0.5).samples);
    s.extend(vec![0.0; SR as usize]);
    let a = Audio { sr: SR, samples: s };
    let t = dsp::trim_silence(&a, -40.0, 0.05);
    assert!((t.duration() - 0.6).abs() < 0.03, "got {}", t.duration());
    assert!(dsp::trim_silence(&Audio { sr: SR, samples: vec![0.0; 100] }, -40.0, 0.0).samples.is_empty());
}

#[test]
fn filters_attenuate_the_right_band() {
    let mut low = tone(100.0, 0.5, 0.5);
    dsp::highpass(&mut low, 1000.0);
    assert!(rms(&low.samples[SR as usize / 10..]) < 0.05);
    let mut high = tone(8000.0, 0.5, 0.5);
    dsp::lowpass(&mut high, 500.0);
    assert!(rms(&high.samples[SR as usize / 10..]) < 0.05);
    let mut mid = tone(440.0, 0.5, 0.5);
    dsp::lowpass(&mut mid, 4000.0);
    assert!(rms(&mid.samples[SR as usize / 10..]) > 0.3);
}

#[test]
fn speed_changes_duration_and_loop_to_fills() {
    let a = tone(440.0, 1.0, 0.5);
    assert!((dsp::speed(&a, 2.0).duration() - 0.5).abs() < 0.01);
    assert!((dsp::loop_to(&a, 2.5).duration() - 2.5).abs() < 0.001);
    assert_eq!(dsp::waveform(&a, 50).len(), 50);
}

fn ev(kind: Kind, start: f64, a: Audio, gain: f32) -> Event {
    Event::new(kind, start, a, gain)
}

#[test]
fn music_is_ducked_under_speech_and_recovers() {
    let music = ev(Kind::Music, 0.0, tone(220.0, 6.0, 0.5), 1.0);
    let voice = ev(Kind::Voice, 2.0, tone(300.0, 1.0, 0.5), 0.0001); // inaudible: only checks music without speech
    let _ = voice;
    let speech = ev(Kind::Voice, 2.0, tone(300.0, 1.0, 0.0), 1.0);
    let _ = speech;

    // A voice at 440 Hz, music at 220 Hz: measure the music component by correlation-free RMS windows.
    let opts = MixOptions { duck_db: -12.0, ..Default::default() };
    let (no_voice, _) = mix::mix_with(&[music.clone()], 6.0, &opts);
    let (with_voice, _) = mix::mix_with(&[music.clone(), ev(Kind::Voice, 2.0, tone(440.0, 1.0, 0.0001), 1.0)], 6.0, &opts);
    // Voice too quiet to trigger ducking (threshold 0.02): music untouched.
    let w = |v: &[f32], a: f64, b: f64| rms(&v[(a * SR as f64) as usize..(b * SR as f64) as usize]);
    assert!((w(&no_voice, 2.2, 2.8) - w(&with_voice, 2.2, 2.8)).abs() < 0.01);

    let loud = ev(Kind::Voice, 2.0, tone(440.0, 1.0, 0.3), 1.0);
    let (ducked, _) = mix::mix_with(&[music.clone(), loud], 6.0, &opts);
    let before = w(&no_voice, 2.2, 2.8);
    // During speech the music component is ~12 dB lower; the voice itself adds ~0.21 rms, so compare via a music-only reference.
    let (voice_only, _) = mix::mix_with(&[ev(Kind::Voice, 2.0, tone(440.0, 1.0, 0.3), 1.0)], 6.0, &opts);
    let music_during: Vec<f32> = ducked.iter().zip(&voice_only).map(|(m, v)| m - v).collect();
    let during = w(&music_during, 2.3, 2.8);
    assert!(during < before * 0.35, "music should duck: {} vs {}", during, before);
    let after = w(&music_during, 4.5, 5.5);
    assert!(after > before * 0.9, "music should recover: {} vs {}", after, before);
}

#[test]
fn fades_loops_and_stop_times_are_honoured() {
    let short = tone(200.0, 0.5, 0.5);
    let mut m = ev(Kind::Music, 1.0, short, 1.0);
    m.looped = true;
    m.stop = Some(3.0);
    m.fade_out = 0.5;
    m.fade_in = 0.25;
    let (o, _) = mix::mix_with(&[m], 5.0, &MixOptions { duck_db: 0.0, ..Default::default() });
    let w = |a: f64, b: f64| rms(&o[(a * SR as f64) as usize..(b * SR as f64) as usize]);
    assert!(w(0.0, 0.9) < 1e-6, "silent before the start");
    assert!(w(1.0, 1.1) < w(1.6, 2.0), "fade in");
    assert!(w(1.6, 2.4) > 0.25, "loops past its natural length");
    assert!(w(2.9, 3.0) < w(2.0, 2.2) * 0.5, "fade out before the stop");
    assert!(w(3.1, 5.0) < 1e-6, "silent after the stop");
}

#[test]
fn the_master_never_clips() {
    let evs: Vec<Event> = (0..6).map(|i| ev(Kind::Sfx, 0.0, tone(200.0 + i as f32 * 70.0, 1.0, 0.9), 1.0)).collect();
    let (o, report) = mix::mix_with(&evs, 1.0, &MixOptions::default());
    assert!(o.iter().all(|v| v.abs() <= 1.0));
    assert!(report.limited && report.peak_db > 0.0);
}

#[test]
fn import_converts_to_project_format_and_cleans_voices() {
    let dir = std::env::temp_dir().join(format!("oneiro-studio-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // 22.05 kHz, quiet voice with silence around it and a DC offset.
    let mut s = vec![0.1f32; 11025];
    s.extend((0..22050).map(|i| 0.1 + (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 22050.0).sin() * 0.03));
    s.extend(vec![0.1f32; 11025]);
    let src = dir.join("raw.wav");
    write_wav(&src, &s, 22050).unwrap();
    let out = import::import_into(&dir, &src, "audio/clean", ImportKind::Voice).unwrap();
    assert!(out.ends_with("audio/clean.wav") || out.ends_with("audio\\clean.wav"));
    let a = oneiro_audio::read_wav(&out).unwrap();
    assert_eq!(a.sr, SR);
    assert!(a.duration() > 0.9 && a.duration() < 1.3, "trimmed to ~1 s, got {}", a.duration());
    assert!((dsp::loudness_db(&a) + 20.0).abs() < 1.5, "levelled, got {}", dsp::loudness_db(&a));
    let mean = a.samples.iter().sum::<f32>() / a.samples.len() as f32;
    assert!(mean.abs() < 0.01, "DC removed, got {}", mean);
    assert!(import::import_into(&dir, &dir.join("missing.mp3"), "x", ImportKind::Raw).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn ffmpeg_decodes_other_formats_when_available() {
    if !import::ffmpeg_available() {
        eprintln!("ffmpeg not installed: skipped");
        return;
    }
    let dir = std::env::temp_dir().join(format!("oneiro-ffmpeg-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let wav = dir.join("t.wav");
    write_wav(&wav, &tone(440.0, 1.0, 0.5).samples, SR).unwrap();
    let flac = dir.join("t.flac");
    let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-i"]).arg(&wav).arg(&flac).status().unwrap().success();
    assert!(ok);
    let a = import::read_any(&flac).unwrap();
    assert!((a.duration() - 1.0).abs() < 0.05);
    assert!(dsp::peak(&a.samples) > 0.4);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn pcm16_bytes_decode() {
    let bytes: Vec<u8> = [0i16, 16384, -16384, 32767].iter().flat_map(|v| v.to_le_bytes()).collect();
    let a = import::from_pcm16(&bytes, 24000);
    assert_eq!(a.samples.len(), 4);
    assert!((a.samples[1] - 0.5).abs() < 1e-3 && (a.samples[2] + 0.5).abs() < 1e-3);
}
