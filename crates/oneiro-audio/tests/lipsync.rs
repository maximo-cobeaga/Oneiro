use oneiro_audio::{lipsync, synth, Audio, Shape};

#[test]
fn speech_opens_the_mouth_and_silence_closes_it() {
    let sr = 48_000;
    let mut samples = vec![0.0f32; sr as usize / 2]; // 0.5s of silence
    samples.extend(synth::synth_speech("hello there, how are you", 160.0, sr, 3));
    let a = Audio { sr, samples };
    let track = lipsync::from_audio(&a);

    // Silence at the start.
    assert_eq!(track.at(0.2).shape, Shape::Rest);
    assert_eq!(track.at(0.2).open, 0.0);

    // Speech: the mouth is open for a good part of it and uses several shapes.
    let dur = a.duration();
    let mut open_frames = 0;
    let mut shapes = std::collections::HashSet::new();
    let mut n = 0;
    let mut t = 0.5;
    while t < dur {
        let v = track.at(t);
        if v.open > 0.2 {
            open_frames += 1;
        }
        shapes.insert(v.shape as usize);
        n += 1;
        t += 1.0 / 24.0;
    }
    assert!(open_frames as f64 / n as f64 > 0.3, "mouth open only {}/{}", open_frames, n);
    assert!(shapes.len() >= 3, "only {} distinct shapes", shapes.len());
}

#[test]
fn text_fallback_covers_the_duration() {
    let t = lipsync::from_text("Where's my pie?", 1.5);
    assert!((t.duration() - 1.5).abs() < 0.02);
    assert!(t.frames.iter().any(|f| f.open > 0.5));
}
