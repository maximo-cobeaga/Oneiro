//! Phase 4: music and sound statements compile into mixer events with the right timing.

use oneiro_audio::musicgen::{self, Mood, MusicSpec};
use oneiro_audio::{dsp, write_wav, SR};
use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, AudioKind, Episode, Project};
use oneiro_lang::Diagnostic;
use std::path::{Path, PathBuf};

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap().flatten() {
        let (p, q) = (e.path(), to.join(e.file_name()));
        if p.is_dir() {
            copy_dir(&p, &q);
        } else {
            std::fs::copy(&p, &q).unwrap();
        }
    }
}

/// A scratch copy of the example project with two music tracks added.
fn project(tag: &str) -> (PathBuf, Project) {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short");
    let root = std::env::temp_dir().join(format!("oneiro_audio_test_{}_{}", std::process::id(), tag));
    let _ = std::fs::remove_dir_all(&root);
    copy_dir(&src, &root);
    for (name, mood) in [("bgm", Mood::Cheerful), ("tense", Mood::Tense)] {
        let a = musicgen::generate(&MusicSpec { bars: 2, ..MusicSpec::new(mood, 1) });
        write_wav(&root.join("library/music").join(format!("{}.wav", name)), &a.samples, SR).unwrap();
    }
    let mut d = vec![];
    let p = Project::load(&root, &mut d);
    (root, p)
}

fn compile_with(p: &Project, script: &str) -> (Episode, Vec<Diagnostic>) {
    compile_source(p, "script.onr", script, &Lock::default())
}

fn example(root: &Path) -> String {
    std::fs::read_to_string(root.join("script.onr")).unwrap()
}

fn music(ep: &Episode) -> Vec<&oneiro_core::AudioEvent> {
    ep.audio.iter().filter(|e| e.kind == AudioKind::Music).collect()
}

#[test]
fn events_are_classified_and_music_loops_by_default() {
    let (root, p) = project("kinds");
    let text = example(&root).replacen("  sfx door_bell at diner.door", "  music bgm volume 0.5 fade_in 2s\n  sfx door_bell at diner.door", 1);
    let (ep, d) = compile_with(&p, &text);
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert_eq!(ep.audio.iter().filter(|e| e.kind == AudioKind::Voice).count(), 5);
    assert_eq!(ep.audio.iter().filter(|e| e.kind == AudioKind::Sfx).count(), 1);
    let m = music(&ep);
    assert_eq!(m.len(), 1);
    assert!(m[0].looped && m[0].stop.is_none());
    assert!((m[0].gain - 0.5).abs() < 1e-6 && (m[0].fade_in - 2.0).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn music_stop_fades_out_from_that_moment() {
    let (root, p) = project("stop");
    let text = example(&root)
        .replacen("  sfx door_bell at diner.door", "  music bgm\n  sfx door_bell at diner.door", 1)
        .replacen("  camera wide from east", "  music stop fade_out 2s\n  camera wide from east", 1);
    let (ep, d) = compile_with(&p, &text);
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let m = music(&ep);
    let stop = m[0].stop.expect("music stop sets a stop time");
    assert!((m[0].fade_out - 2.0).abs() < 1e-9);
    // The stop statement sits late in the 30 s script; the fade ends two seconds after it.
    assert!(stop > 15.0 && stop < ep.duration + 2.0, "stop at {}", stop);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_new_track_replaces_the_old_one_with_a_crossfade() {
    let (root, p) = project("cross");
    let text = example(&root)
        .replacen("  sfx door_bell at diner.door", "  music bgm\n  sfx door_bell at diner.door", 1)
        .replacen("  camera over_shoulder Blob -> Marta", "  music tense fade_in 1.5s once\n  camera over_shoulder Blob -> Marta", 1);
    let (ep, d) = compile_with(&p, &text);
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let m = music(&ep);
    assert_eq!(m.len(), 2);
    let t2 = m[1].start;
    assert!(m[0].looped && !m[1].looped, "`once` disables looping");
    assert!((m[0].stop.unwrap() - (t2 + 1.5)).abs() < 1e-6, "old track ends one fade after the new one starts");
    assert!((m[0].fade_out - 1.5).abs() < 1e-9);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn render_audio_has_music_ducked_under_dialogue() {
    let (root, p) = project("render");
    let text = example(&root).replacen("  sfx door_bell at diner.door", "  music bgm volume 0.6\n  sfx door_bell at diner.door", 1);
    let (ep, _) = compile_with(&p, &text);
    let (loud, report) = ep.render_audio();
    assert_eq!(loud.len(), (ep.duration * SR as f64).ceil() as usize);
    assert!(report.peak_db <= 0.5 && loud.iter().all(|v| v.abs() <= 1.0));

    let mut quiet_ep = Episode { audio: ep.audio.clone(), ..compile_with(&p, &text).0 };
    quiet_ep.duck_db = 0.0;
    let (flat, _) = quiet_ep.render_audio();
    // Compare the energy during the first spoken line: ducking must make the mix quieter there.
    let v = ep.audio.iter().find(|e| e.kind == AudioKind::Voice).unwrap();
    let (a, b) = ((v.start * SR as f64) as usize, ((v.start + 1.0) * SR as f64) as usize);
    let rms = |s: &[f32]| (s.iter().map(|x| x * x).sum::<f32>() / s.len() as f32).sqrt();
    assert!(rms(&loud[a..b]) < rms(&flat[a..b]), "ducked {} vs flat {}", rms(&loud[a..b]), rms(&flat[a..b]));
    let _ = dsp::peak(&loud);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn unknown_music_is_reported_with_a_suggestion() {
    let (root, p) = project("missing");
    let text = example(&root).replacen("  sfx door_bell at diner.door", "  music nope\n  sfx door_bell at diner.door", 1);
    let (_, d) = compile_with(&p, &text);
    assert!(d.iter().any(|x| x.code == "E2401"), "{:?}", d);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn voice_settings_round_trip_through_onchar() {
    let src = "character Zed {\n  rig: library.rigs.biped_cutout\n  voice { provider: elevenlabs, pitch: 150, voice: \"abc123\", speed: 1.15 }\n}\n";
    let (file, mut d) = oneiro_lang::parse(src, "Zed.onchar");
    let c = oneiro_core::CharacterDef::from_block(&file.blocks[0], "Zed.onchar", &mut d);
    assert_eq!((c.voice_provider.as_str(), c.voice_id.as_str()), ("elevenlabs", "abc123"));
    assert!((c.voice_speed - 1.15).abs() < 1e-9);
    let again = c.to_source();
    assert!(again.contains("voice: \"abc123\"") && again.contains("speed: 1.15"), "{}", again);
}
