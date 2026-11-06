//! Phase 2 support in the core: floor-point targets (used by motion recording), compiling from
//! memory, character serialization (character builder) and statement spans (timeline links).

use oneiro_core::lock::Lock;
use oneiro_core::math::*;
use oneiro_core::{compile, compile_source, CharacterDef, Project};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap()
}

fn project() -> Project {
    let mut d = vec![];
    Project::load(&root(), &mut d)
}

fn example_text() -> String {
    std::fs::read_to_string(root().join("script.onr")).unwrap()
}

#[test]
fn floor_point_targets_move_the_character_there() {
    let text = example_text().replacen("Blob walks_to diner.counter", "Blob walks_to (1.5, 0.5)\n  Blob runs_to (-1, -1.25)", 1);
    let (ep, d) = compile_source(&project(), "t.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d);
    let blob = ep.scenes[0].actors.iter().find(|a| a.name == "Blob").unwrap();
    assert_eq!(blob.moves.len(), 2);
    assert!(dist2(blob.moves[0].to, [1.5, 0.5]) < 1e-9, "{:?}", blob.moves[0].to);
    assert!(dist2(blob.moves[1].to, [-1.0, -1.25]) < 1e-9);
    assert!(blob.moves[1].run && !blob.moves[0].run);
}

#[test]
fn malformed_points_are_reported() {
    let text = example_text().replacen("Blob walks_to diner.counter", "Blob walks_to (1.5)", 1);
    let (_, d) = compile_source(&project(), "t.onr", &text, &Lock::default());
    assert!(d.iter().any(|x| x.code == "E0101"), "{:?}", d);
}

#[test]
fn compile_source_matches_compile_from_disk() {
    let p = project();
    let (a, da) = compile(&p, &root().join("script.onr"), &Lock::default());
    let (b, db) = compile_source(&p, "script.onr", &example_text(), &Lock::default());
    assert!(da.is_empty() && db.is_empty());
    assert!((a.duration - b.duration).abs() < 1e-12);
    assert_eq!(a.scenes[0].cameras.len(), b.scenes[0].cameras.len());
}

#[test]
fn characters_round_trip_through_source() {
    let p = project();
    assert!(p.characters.len() >= 2);
    for (name, def) in &p.characters {
        let src = def.to_source();
        let (file, d) = oneiro_lang::parse(&src, "rt");
        assert!(d.is_empty(), "{}: {:?}", name, d);
        let mut diags = vec![];
        let back = CharacterDef::from_block(&file.blocks[0], "rt", &mut diags);
        assert!(diags.is_empty(), "{}: {:?}", name, diags);
        assert_eq!(back.to_source(), src, "{} is not stable", name);
        assert_eq!(back.wardrobe, def.wardrobe);
        assert!((back.height() - def.height()).abs() < 1e-9);
        assert_eq!(back.head.kind, def.head.kind);
        assert_eq!(back.skin(), def.skin());
    }
}

#[test]
fn statement_spans_are_contiguous_and_linked_to_lines() {
    let (ep, _) = compile(&project(), &root().join("script.onr"), &Lock::default());
    let sc = &ep.scenes[0];
    assert!(sc.spans.len() > 20);
    for w in sc.spans.windows(2) {
        assert!((w[0].t1 - w[1].t0).abs() < 1e-9, "gap between {:?} and {:?}", w[0], w[1]);
        assert!(w[0].line < w[1].line);
    }
    assert!(sc.spans.last().unwrap().t1 <= sc.dur);
    // the walk statement is on the line that says so
    let text = example_text();
    let walk = sc.spans.iter().find(|s| s.label.contains("Blob walks_to")).unwrap();
    assert!(text.lines().nth(walk.line as usize - 1).unwrap().contains("Blob walks_to"));
    assert!(walk.t1 - walk.t0 > 2.0);
}

#[test]
fn camera_lookup_never_panics_without_cameras() {
    let (mut ep, _) = compile(&project(), &root().join("script.onr"), &Lock::default());
    ep.scenes[0].cameras.clear();
    let pose = ep.scenes[0].camera_at(3.0);
    assert!(pose.fov > 10.0);
}

#[test]
fn placing_at_a_solid_anchor_stands_in_front_of_it_facing_it() {
    let text = example_text().replacen("place Blob  at diner.door", "place Blob  at diner.counter", 1);
    let (ep, d) = compile_source(&project(), "t.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d);
    let blob = ep.scenes[0].actors.iter().find(|a| a.name == "Blob").unwrap();
    let counter = [0.0, 2.35];
    let dist = dist2(blob.init_pos, counter);
    assert!((dist - 0.75).abs() < 1e-6, "stands {} m from the counter", dist);
    assert!(blob.init_pos[1] < counter[1], "in front of the counter, not behind it");
    // facing the counter (north)
    assert!(angle_diff(blob.init_yaw, 0.0).abs() < 0.2, "yaw {}", blob.init_yaw);
}

#[test]
fn a_lone_subject_is_framed_from_the_front_not_from_behind() {
    // Blob faces the counter (north); alone in the scene, a close-up must come from the north side.
    let text = "oneiro 0.1\nuse character Blob\nuse set diner\nscene 1 \"t\" in diner {\n  place Blob at diner.counter\n  cut to close on Blob\n  wait 2s\n}\n";
    let (ep, d) = compile_source(&project(), "t.onr", text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d);
    let sc = &ep.scenes[0];
    let blob = sc.actors.iter().find(|a| a.name == "Blob").unwrap();
    let cam = sc.cameras.iter().find(|c| c.index == 1).unwrap().base;
    let to_cam = [cam.pos[0] - blob.init_pos[0], cam.pos[2] - blob.init_pos[1]];
    let facing = fwd(blob.init_yaw);
    let cos = (to_cam[0] * facing[0] + to_cam[1] * facing[1]) / (to_cam[0].hypot(to_cam[1]) * 1.0);
    assert!(cos > 0.8, "camera should be in front of Blob's face (cos {:.2})", cos);
}
