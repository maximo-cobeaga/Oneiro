//! Real audio devices. Skipped (with a message) on machines without a speaker or microphone.
#![cfg(feature = "device")]

use oneiro_audio::device::{self, Player, Recorder};
use oneiro_audio::{Audio, SR};
use std::time::Duration;

fn quiet_tone(secs: f32) -> Audio {
    let n = (SR as f32 * secs) as usize;
    Audio { sr: SR, samples: (0..n).map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / SR as f32).sin() * 0.03).collect() }
}

#[test]
fn the_speaker_plays_and_reports_its_position() {
    let p = match Player::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("no output device ({}): skipped", e);
            return;
        }
    };
    eprintln!("output: {} at {} Hz; devices: {:?}", p.device_name, p.device_sr, device::output_devices());
    p.play_clip(&quiet_tone(0.6));
    assert!(p.is_playing());
    std::thread::sleep(Duration::from_millis(300));
    let pos = p.position();
    assert!(pos > 0.15 && pos < 0.6, "position after 300 ms: {}", pos);
    // Seeking moves the playhead.
    p.play_from(0.5);
    std::thread::sleep(Duration::from_millis(50));
    assert!(p.position() >= 0.5);
    // It stops by itself at the end.
    std::thread::sleep(Duration::from_millis(500));
    assert!(!p.is_playing());
    p.pause();
}

#[test]
fn the_microphone_records_samples() {
    let r = match Recorder::start() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no microphone ({}): skipped", e);
            return;
        }
    };
    eprintln!("microphone: {} at {} Hz; devices: {:?}", r.device_name, r.sr, device::input_devices());
    std::thread::sleep(Duration::from_millis(700));
    let secs = r.seconds();
    let a = r.stop();
    eprintln!("recorded {:.2}s ({} samples), peak {:.4}", a.duration(), a.samples.len(), oneiro_audio::dsp::peak(&a.samples));
    assert!(secs > 0.3, "only {} s recorded in 0.7 s", secs);
    assert!(a.samples.iter().all(|v| v.is_finite()));
}
