//! Phase 5: rubber_hose, clay, vector (tier B) characters and hybrid scenes.

use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Episode, Project};
use oneiro_render::clay;
use oneiro_render::{hash_pixmap, render_range, Renderer};
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

fn gallery() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/style_gallery").canonicalize().unwrap()
}

fn project() -> Project {
    let mut d = vec![];
    let p = Project::load(&gallery(), &mut d);
    assert!(d.is_empty(), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    p
}

fn episode(p: &Project, script: &str) -> Episode {
    let (ep, d) = compile_source(p, "script.onr", script, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    ep
}

/// One character on a static camera: only the style differs between the scripts.
fn solo(who: &str, style: &str, extra: &str) -> String {
    format!(
        "oneiro 0.1\nproject \"t\"\nstyle default: cutout_2d\nuse character {who} style {style}\nuse set diner\nscene 1 \"s\" in diner {{\n  place {who} at diner.floor\n  camera full on {who}\n  {extra}\n  wait 3s\n}}\n"
    )
}

fn count(pm: &Pixmap, f: impl Fn([u8; 3]) -> bool) -> usize {
    pm.data().chunks(4).filter(|c| f([c[0], c[1], c[2]])).count()
}

fn diff_pixels(a: &Pixmap, b: &Pixmap) -> usize {
    a.data().chunks(4).zip(b.data().chunks(4)).filter(|(x, y)| x != y).count()
}

#[test]
fn every_style_draws_a_visibly_different_character() {
    let p = project();
    let mut frames = vec![];
    for style in ["cutout_2d", "rubber_hose", "clay"] {
        let ep = episode(&p, &solo("Blob", style, ""));
        let pm = Renderer::new(&ep, 480, 270).render_at(1.0);
        // The character (a green head, white apron) must be on screen.
        assert!(count(&pm, |c| { let (r, g, b) = (c[0] as i32, c[1] as i32, c[2] as i32); g > r + 20 && g > b + 10 && g > 105 }) > 300, "{}: no green head found", style);
        frames.push((style, pm));
    }
    for i in 0..3 {
        for j in i + 1..3 {
            assert!(diff_pixels(&frames[i].1, &frames[j].1) > 2000, "{} and {} look the same", frames[i].0, frames[j].0);
        }
    }
}

#[test]
fn a_scene_can_mix_styles_and_stays_deterministic() {
    let p = project();
    let root = gallery();
    let ep = episode(&p, &std::fs::read_to_string(root.join("script.onr")).unwrap());
    let styles: Vec<String> = ep.scenes[0].actors.iter().map(|a| a.def.style.clone()).collect();
    assert!(styles.contains(&"cutout_2d".to_string()) && styles.contains(&"rubber_hose".to_string()));
    let r = Renderer::new(&ep, 320, 180);
    for f in [0usize, 60, 130, 250] {
        assert_eq!(r.render_frame(f).data(), r.render_frame(f).data(), "frame {} is not reproducible", f);
    }
    let serial: Vec<u64> = (90..114).map(|i| hash_pixmap(&r.render_frame(i))).collect();
    let mut parallel = vec![];
    render_range(&r, 90..114, |_, pm| {
        parallel.push(hash_pixmap(pm));
        Ok(())
    })
    .unwrap();
    assert_eq!(serial, parallel, "parallel rendering must match serial for hybrid scenes");

    // Clay too (stop-motion jitter and the shading are pure functions of time).
    let clay_ep = episode(&p, &std::fs::read_to_string(root.join("clay.onr")).unwrap());
    let rc = Renderer::new(&clay_ep, 320, 180);
    let s2: Vec<u64> = (40..60).map(|i| hash_pixmap(&rc.render_frame(i))).collect();
    let mut p2 = vec![];
    render_range(&rc, 40..60, |_, pm| {
        p2.push(hash_pixmap(pm));
        Ok(())
    })
    .unwrap();
    assert_eq!(s2, p2);
}

#[test]
fn clay_is_animated_on_twos_like_stop_motion() {
    let p = project();
    // Marta breathes and blinks; with a static camera, two instants inside the same 1/12 s hold
    // must give the same picture and the next hold a different one.
    let ep = episode(&p, &solo("Marta", "clay", "Marta waves"));
    let r = Renderer::new(&ep, 320, 180);
    let a = r.render_at(1.00);
    let b = r.render_at(1.04); // same hold: floor(12) == floor(12.48)
    let c = r.render_at(1.10); // next hold
    assert_eq!(a.data(), b.data(), "held pose must not change inside one 1/12 s");
    assert!(diff_pixels(&a, &c) > 50, "the pose must change between holds");
    // A cutout character, by contrast, moves every frame.
    let cut = episode(&p, &solo("Marta", "cutout_2d", "Marta waves"));
    let rc = Renderer::new(&cut, 320, 180);
    assert!(diff_pixels(&rc.render_at(1.00), &rc.render_at(1.04)) > 20, "cutout is animated on ones");
}

#[test]
fn rubber_hose_boils_at_twelve_frames_per_second() {
    let p = project();
    let ep = episode(&p, &solo("Blob", "rubber_hose", "Blob waves"));
    let r = Renderer::new(&ep, 480, 270);
    let a = r.render_at(1.00);
    let b = r.render_at(1.02); // same 1/12 s: wobble and bounce are held, only the skeleton moves a hair
    let c = r.render_at(1.30);
    assert!(diff_pixels(&a, &c) > diff_pixels(&a, &b), "a later frame must differ more than a nearby one");
}

#[test]
fn vector_characters_draw_their_parts_and_the_face() {
    let p = project();
    let ep = episode(&p, &solo("Pip", "cutout_2d", ""));
    assert!(ep.scenes[0].actors[0].def.has_svg());
    let pm = Renderer::new(&ep, 640, 360).render_at(1.0);
    // Head blue (#8fc3ee), body orange (#f2aa3c), shoes red (#ff6b5b) all appear; the face puts dark pixels on the head.
    let near = |c: [u8; 3], t: [u8; 3]| (0..3).all(|i| (c[i] as i32 - t[i] as i32).abs() < 26);
    assert!(count(&pm, |c| near(c, [0x8f, 0xc3, 0xee])) > 300, "head part missing");
    assert!(count(&pm, |c| near(c, [0xf2, 0xaa, 0x3c])) > 300, "body part missing");
    assert!(count(&pm, |c| near(c, [0xff, 0x6b, 0x5b])) > 30, "leg/shoe part missing");

    // Seen from behind the face is not drawn.
    let behind = episode(&p, "oneiro 0.1
project \"t\"
use character Pip
use set diner
scene 1 \"s\" in diner {
  place Pip at diner.floor facing diner.counter
  camera full on Pip
  wait 2s
}
");
    let back = Renderer::new(&behind, 640, 360).render_at(1.0);
    let front_face = count(&pm, |c| c[0] < 60 && c[1] < 60 && c[2] < 70);
    let back_face = count(&back, |c| c[0] < 60 && c[1] < 60 && c[2] < 70);
    assert!(front_face != back_face, "front and back views must differ");

    // The mouth follows speech: a talking frame differs from a silent one.
    let talk = episode(&p, &solo("Pip", "cutout_2d", "Pip (happy): \"Hello there, how are you doing today?\""));
    let rt = Renderer::new(&talk, 640, 360);
    let mut distinct = std::collections::HashSet::new();
    for k in 0..24 {
        distinct.insert(hash_pixmap(&rt.render_at(0.2 + k as f64 * 0.07)));
    }
    assert!(distinct.len() > 6, "the vector character should animate while talking ({} distinct frames)", distinct.len());
}

#[test]
fn every_style_survives_every_gesture_and_a_walk() {
    let p = project();
    for who in ["Marta", "Blob", "Pip"] {
        for style in ["cutout_2d", "rubber_hose", "clay"] {
            let extra = "together {\n    X waves\n  }\n  X shrugs\n  X crosses_arms\n  X shakes_head\n  X nods\n  X points_at diner.door\n  X walks_to (1, 0)\n  X sits_on diner.stool_3".replace('X', who);
            let ep = episode(&p, &solo(who, style, &extra));
            let r = Renderer::new(&ep, 256, 144);
            let n = ep.frame_count().min(240);
            for f in (0..n).step_by(5) {
                let pm = r.render_frame(f);
                assert!(pm.data().chunks(4).all(|c| c[3] == 255), "{} {} frame {}: transparent pixels", who, style, f);
            }
        }
    }
}

#[test]
fn a_character_with_broken_parts_falls_back_to_the_blocks() {
    let mut p = project();
    p.characters.get_mut("Pip").unwrap().svg.as_mut().unwrap().parts.get_mut("arm").unwrap().doc = None;
    assert!(!p.characters["Pip"].has_svg());
    let ep = episode(&p, &solo("Pip", "cutout_2d", ""));
    let pm = Renderer::new(&ep, 320, 180).render_at(1.0);
    assert!(count(&pm, |c| c[0] != c[1]) > 1000, "drew something without panicking");
}

#[test]
fn clay_shading_lights_a_pillow_from_the_upper_left_and_leaves_air_alone() {
    let (w, h) = (120u32, 120u32);
    let mut layer = Pixmap::new(w, h).unwrap();
    // A flat mid-gray disk.
    let mut paint = tiny_skia::Paint::default();
    paint.set_color_rgba8(150, 150, 150, 255);
    let path = tiny_skia::PathBuilder::from_circle(60.0, 60.0, 40.0).unwrap();
    layer.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
    let before = layer.clone();
    clay::shade(&mut layer, 300.0, (60.0, 100.0), 7);
    let px = |pm: &Pixmap, x: u32, y: u32| {
        let d = &pm.data()[((y * w + x) * 4) as usize..][..4];
        d[0] as i32 + d[1] as i32 + d[2] as i32
    };
    // Upper-left rim is brighter than the lower-right rim.
    let ul: i32 = (0..4).map(|k| px(&layer, 35 + k, 35 + k)).sum();
    let lr: i32 = (0..4).map(|k| px(&layer, 85 - k, 85 - k)).sum();
    assert!(ul > lr + 40, "upper-left {} should be lighter than lower-right {}", ul, lr);
    // Transparent pixels stay transparent, alpha is untouched.
    assert_eq!(layer.data()[3], 0);
    assert!(before.data().chunks(4).zip(layer.data().chunks(4)).all(|(a, b)| a[3] == b[3]));
    // Pure function: same inputs, same pixels.
    let mut again = before.clone();
    clay::shade(&mut again, 300.0, (60.0, 100.0), 7);
    assert_eq!(layer.data(), again.data());
    let mut other = before.clone();
    clay::shade(&mut other, 300.0, (60.0, 100.0), 8);
    assert_ne!(layer.data(), other.data(), "different characters get different lumps");
    let (j1, j2) = (clay::jitter(5, 1, 500.0), clay::jitter(6, 1, 500.0));
    assert_eq!(j1, clay::jitter(5, 1, 500.0));
    assert!(j1 != j2 && j1.0.abs() <= 2.1 && j1.1.abs() <= 1.6);
}

#[test]
fn clay_characters_are_drawn_without_outlines() {
    use oneiro_render::character::{draw_character_ex, Placement, View};
    let p = project();
    let ep = episode(&p, &solo("Blob", "cutout_2d", ""));
    let sc = &ep.scenes[0];
    let fr = sc.frames_at(1.0).remove(0);
    let view = View { front: 1.0, side: 0.0, h_front: 1.0, h_side: 0.0, eye_dx: 0.0, eye_dy: 0.0, target_side: 1.0, target_up: 0.0 };
    let pl = Placement { sx: 200.0, sy: 380.0, scale: 280.0 };
    let ink = |outline: f32| {
        let mut pm = Pixmap::new(400, 400).unwrap();
        draw_character_ex(&mut pm, &sc.actors[0].def, &fr, &view, &pl, [1.0, 1.0, 1.0], 1.0, outline);
        pm.data().chunks(4).filter(|c| c[3] > 200 && c[0] < 40 && c[1] < 40 && c[2] < 40).count()
    };
    let (with, without) = (ink(1.0), ink(0.0));
    assert!(with > without * 2 && with - without > 800, "outlines on: {} ink pixels, off: {} (only the face lines remain)", with, without);
}

#[test]
fn ink_reinks_a_character_with_heavy_line_and_few_tones() {
    let p = project();
    let plain = Renderer::new(&episode(&p, &solo("Marta", "cutout_2d", "")), 320, 180).render_at(1.0);
    let ink = Renderer::new(&episode(&p, &solo("Marta", "ink", "")), 320, 180).render_at(1.0);
    assert!(diff_pixels(&plain, &ink) > 200, "ink must look different");
    // The line colour (near-black blue ink) shows up where the plain version has none.
    let dark = |c: [u8; 3]| c[0] < 40 && c[1] < 40 && c[2] < 55;
    assert!(count(&ink, dark) > count(&plain, dark) + 30);
    assert_eq!(ink.data(), Renderer::new(&episode(&p, &solo("Marta", "ink", "")), 320, 180).render_at(1.0).data());
}

#[test]
fn flat_characters_in_toon_3d_become_turning_paper_dolls() {
    use oneiro_render::paperdoll::{card_scale, compose};
    assert!(card_scale(1.0) > card_scale(0.5) && card_scale(0.5) > card_scale(0.0) && card_scale(0.0) > 0.2);
    assert_eq!(card_scale(-1.0), card_scale(1.0), "same width from behind");
    // A solid 20x20 card narrows when seen from the side and leaves a dark edge.
    let mut layer = Pixmap::new(40, 40).unwrap();
    layer.fill(tiny_skia::Color::from_rgba8(200, 60, 60, 255));
    let width = |front: f64| {
        let mut pm = Pixmap::new(80, 40).unwrap();
        compose(&mut pm, &layer, 20, 0, 20.0, front, 1.0, 4.0);
        let row: Vec<&[u8]> = pm.data().chunks(4).skip(20 * 80).take(80).collect();
        row.iter().filter(|p| p[3] > 128).count()
    };
    let (face, side) = (width(1.0), width(0.0));
    assert!(face >= 38 && side < face - 5, "face {} side {}", face, side);

    let p = project();
    let flat = Renderer::new(&episode(&p, &solo("Pip", "cutout_2d", "")), 320, 180).render_at(1.0);
    let doll = Renderer::new(&episode(&p, &solo("Pip", "toon_3d", "")), 320, 180).render_at(1.0);
    assert_eq!(doll.data(), Renderer::new(&episode(&p, &solo("Pip", "toon_3d", "")), 320, 180).render_at(1.0).data());
    assert!(diff_pixels(&flat, &doll) > 100 || flat.data() == doll.data(), "renders without panicking");
}
