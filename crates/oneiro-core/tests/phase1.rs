//! Phase 1 acceptance tests: the example compiles cleanly, broken variants give the expected
//! diagnostics, cameras respect the 180-degree rule and speech drives the mouth.

use oneiro_core::lock::Lock;
use oneiro_core::math::*;
use oneiro_core::{compile, Episode, Project};
use oneiro_lang::Diagnostic;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap()
}

fn compile_text(text: &str) -> (Episode, Vec<Diagnostic>) {
    let root = root();
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let tmp = std::env::temp_dir().join(format!("oneiro_test_{}_{}.onr", std::process::id(), n));
    std::fs::write(&tmp, text).unwrap();
    let (ep, mut d2) = compile(&project, &tmp, &Lock::default());
    let _ = std::fs::remove_file(&tmp);
    d.append(&mut d2);
    (ep, d)
}

fn example_text() -> String {
    std::fs::read_to_string(root().join("script.onr")).unwrap()
}

fn errors(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter().filter(|x| x.is_error()).collect()
}

/// Replaces one statement of the example and returns the diagnostics.
fn with_replacement(from: &str, to: &str) -> Vec<Diagnostic> {
    let text = example_text();
    assert!(text.contains(from), "fixture line not found: {}", from);
    compile_text(&text.replacen(from, to, 1)).1
}

fn has(d: &[Diagnostic], code: &str) -> bool {
    d.iter().any(|x| x.code == code)
}

#[test]
fn example_compiles_without_diagnostics() {
    let (ep, d) = compile_text(&example_text());
    assert!(d.is_empty(), "{:#?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    assert_eq!(ep.scenes.len(), 1);
    assert!(ep.duration > 28.0 && ep.duration < 33.0, "duration {}", ep.duration);
    assert!(ep.scenes[0].cameras.len() >= 6, "cameras: {}", ep.scenes[0].cameras.len());
    assert_eq!(ep.scenes[0].actors.len(), 2);
}

#[test]
fn unknown_verb_suggests_the_right_one() {
    let d = with_replacement("Blob walks_to diner.counter", "Blob walk_to diner.counter");
    let e = d.iter().find(|x| x.code == "E1042").expect("E1042");
    assert!(e.suggestion.as_deref().unwrap_or("").contains("walks_to"), "{:?}", e);
}

#[test]
fn planned_verb_is_reported_as_unavailable() {
    let d = with_replacement("Blob waves", "Blob picks_up diner.clock");
    assert!(has(&d, "E2010"), "{:?}", d);
}

#[test]
fn bad_references_produce_specific_codes() {
    assert!(has(&with_replacement("Blob walks_to diner.counter", "Blob walks_to diner.countr"), "E1044"));
    assert!(has(&with_replacement("place Blob  at diner.door", ""), "E1041"));
    assert!(has(&with_replacement("(annoyed): \"Where's", "(anoyed): \"Where's"), "E1046"));
    assert!(has(&with_replacement("cut to close on Blob", "cut to closeup on Blob"), "E1045"));
    assert!(has(&with_replacement("style default: cutout_2d", "style default: toon_3d"), "E2301"));
    assert!(has(&with_replacement("audio/marta_01.wav", "audio/does_not_exist.wav"), "E2401"));
    assert!(has(&with_replacement("use character Blob", "use character Blobb"), "E1051"));
    assert!(has(&with_replacement("use set diner", "use set dinner"), "E1050"));
}

#[test]
fn diagnostics_serialize_to_json() {
    let d = with_replacement("Blob walks_to diner.counter", "Blob walk_to diner.counter");
    let json = oneiro_lang::to_json_array(&d);
    assert!(json.starts_with('[') && json.contains("\"code\":\"E1042\""));
}

#[test]
fn shot_reverse_shot_stays_on_one_side_of_the_line() {
    let (ep, d) = compile_text(&example_text());
    assert!(errors(&d).is_empty());
    let sc = &ep.scenes[0];
    let ots: Vec<_> = sc.cameras.iter().filter(|c| c.source.contains("over_shoulder")).collect();
    assert_eq!(ots.len(), 2);
    let pos = |name: &str, t: f64| sc.actors.iter().find(|a| a.name == name).unwrap().kin(t).pos;
    let side = |c: &oneiro_core::CameraSeg| {
        let (a, b) = (pos("Marta", c.t0), pos("Blob", c.t0));
        let cam = [c.base.pos[0], c.base.pos[2]];
        (b[0] - a[0]) * (cam[1] - a[1]) - (b[1] - a[1]) * (cam[0] - a[0])
    };
    assert!(side(ots[0]) * side(ots[1]) > 0.0, "cameras on opposite sides of the line of action");
}

#[test]
fn characters_walk_to_anchors_and_sit() {
    let (ep, _) = compile_text(&example_text());
    let sc = &ep.scenes[0];
    let blob = sc.actors.iter().find(|a| a.name == "Blob").unwrap();
    let marta = sc.actors.iter().find(|a| a.name == "Marta").unwrap();
    // Blob starts at the door and ends the walk near the counter (stopping short of it).
    assert!(dist2(blob.kin(0.0).pos, [-3.4, -1.0]) < 0.01);
    let at_counter = blob.kin(7.0).pos;
    assert!(dist2(at_counter, [0.0, 2.35]) > 0.5 && dist2(at_counter, [0.0, 2.35]) < 1.2, "{:?}", at_counter);
    // Marta starts seated, and is standing and gone toward the door at the end.
    assert!(marta.kin(1.0).sit > 0.99);
    assert!(marta.kin(ep.duration - 0.1).sit < 0.01);
    assert!(dist2(marta.kin(ep.duration - 0.1).pos, [-3.4, -1.0]) < 0.2);
}

#[test]
fn speech_drives_the_mouth_from_the_audio() {
    let (ep, d) = compile_text(&example_text());
    assert!(errors(&d).is_empty());
    let sc = &ep.scenes[0];
    let marta = sc.actors.iter().position(|a| a.name == "Marta").unwrap();
    let seg = &sc.actors[marta].speech[0];
    let mut max_open = 0.0f32;
    let mut t = seg.t0;
    while t < seg.t1 {
        let f = &sc.frames_at(t)[marta];
        assert!(f.speaking);
        max_open = max_open.max(f.viseme.open);
        t += 1.0 / 24.0;
    }
    assert!(max_open > 0.3, "mouth never opened: {}", max_open);
    // Not speaking before her first line.
    assert!(!sc.frames_at(0.5)[marta].speaking);
}

#[test]
fn compiling_twice_gives_identical_timelines() {
    let a = compile_text(&example_text()).0;
    let b = compile_text(&example_text()).0;
    for t in [1.0, 5.0, 9.3, 14.1, 20.0, 26.0] {
        let fa = a.scenes[0].frames_at(t);
        let fb = b.scenes[0].frames_at(t);
        for (x, y) in fa.iter().zip(fb.iter()) {
            assert_eq!(x.kin.pos, y.kin.pos);
            assert_eq!(x.kin.yaw, y.kin.yaw);
        }
        let (ca, cb) = (a.scenes[0].camera_at(t), b.scenes[0].camera_at(t));
        assert_eq!(ca.pos, cb.pos);
    }
}

#[test]
fn pinned_lock_values_override_the_solver() {
    let text = example_text();
    let (ep, _) = compile_text(&text);
    // Generate a lock, pin shot 2 with a hand-edited position, and reload it.
    let rendered = oneiro_core::lock::render(&ep.scenes, &Lock::default(), "script.onr");
    let edited = rendered
        .lines()
        .map(|l| {
            if l.trim_start().starts_with("shot 2 {") {
                let start = l.find("pos: (").unwrap();
                let end = l[start..].find(')').unwrap() + start + 1;
                let l2 = format!("{}pos: (1.000, 1.500, -2.000){}", &l[..start], &l[end..]);
                l2.replace("pinned: false", "pinned: true")
            } else {
                l.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    let lock_path = std::env::temp_dir().join(format!("oneiro_lock_{}.onlock", std::process::id()));
    std::fs::write(&lock_path, edited).unwrap();
    let lock = Lock::load(&lock_path);
    let _ = std::fs::remove_file(&lock_path);
    assert!(lock.shots.get(&(1, 2)).map_or(false, |s| s.pinned));

    let mut d = vec![];
    let project = Project::load(&root(), &mut d);
    let tmp = std::env::temp_dir().join(format!("oneiro_pin_{}.onr", std::process::id()));
    std::fs::write(&tmp, &text).unwrap();
    let (ep2, d2) = compile(&project, &tmp, &lock);
    let _ = std::fs::remove_file(&tmp);
    assert!(errors(&d2).is_empty());
    let seg = ep2.scenes[0].cameras.iter().find(|c| c.index == 2).unwrap();
    assert!(seg.pinned);
    assert_eq!(seg.base.pos, [1.0, 1.5, -2.0]);
    // Unpinned shots are still solved normally.
    let seg3 = ep2.scenes[0].cameras.iter().find(|c| c.index == 3).unwrap();
    assert!(!seg3.pinned);
}
