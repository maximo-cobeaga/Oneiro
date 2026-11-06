use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Project};
use oneiro_render::Renderer;
use std::path::Path;

fn ep(kind: &str) -> oneiro_core::Episode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap();
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    let text = format!("oneiro 0.1\nproject \"t\"\nuse character Marta\nuse set diner\nscene 1 \"s\" in diner {{\n  place Marta at diner.door\n  camera wide\n  wait 2s\n  transition {} for 1s\n  camera close on Marta\n  wait 2s\n}}\n", kind);
    let (e, d) = compile_source(&project, "t.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    e
}

fn mean(p: &tiny_skia::Pixmap) -> f64 {
    let d = p.data();
    d.chunks(4).map(|c| c[0] as f64 + c[1] as f64 + c[2] as f64).sum::<f64>() / (d.len() / 4) as f64
}

#[test]
fn fade_black_goes_dark_in_the_middle_and_returns() {
    let e = ep("fade_black");
    let r = Renderer::new(&e, 160, 90);
    let (before, mid, after) = (mean(&r.render_at(1.0)), mean(&r.render_at(2.0)), mean(&r.render_at(3.2)));
    assert!(mid < 1.0, "black at the cut: {}", mid);
    assert!(before > 30.0 && after > 30.0, "{} {}", before, after);
    assert_eq!(r.render_at(2.3).data(), r.render_at(2.3).data());
}

#[test]
fn fade_white_and_dissolve_and_wipes_render() {
    let e = ep("fade_white");
    assert!(mean(&Renderer::new(&e, 160, 90).render_at(2.0)) > 760.0);
    for k in ["dissolve", "wipe_left", "wipe_right"] {
        let e = ep(k);
        let r = Renderer::new(&e, 160, 90);
        let m = r.render_at(2.0);
        assert_ne!(m.data(), r.render_at(1.0).data(), "{}", k);
        assert_ne!(m.data(), r.render_at(3.0).data(), "{}", k);
    }
}

#[test]
fn shadows_follow_the_time_of_day() {
    use oneiro_render::light_dir;
    let (d, n, k) = (light_dir("day"), light_dir("night"), light_dir("dusk"));
    let l = |v: [f64; 2]| (v[0] * v[0] + v[1] * v[1]).sqrt();
    assert!(l(k) > l(d) && l(d) > l(n), "dusk shadows are longest, night shortest");
    assert!(d[0] > 0.0 && k[0] < 0.0, "the sun moves from one side to the other");
}

fn diner_with(look: &str) -> (Project, String) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap();
    let mut d = vec![];
    let mut p = Project::load(&root, &mut d);
    let set = p.sets.get_mut("diner").unwrap();
    set.look = look.to_string();
    (p, "oneiro 0.1\nproject \"t\"\nuse character Marta\nuse set diner\nscene 1 \"s\" in diner {\n  place Marta at diner.stool_2\n  camera wide\n  wait 1s\n}\n".into())
}

#[test]
fn three_d_props_draw_differently_and_reproducibly() {
    let draw = |look: &str| {
        let (p, text) = diner_with(look);
        let (e, d) = compile_source(&p, "t.onr", &text, &Lock::default());
        assert!(d.iter().all(|x| !x.is_error()));
        Renderer::new(&e, 320, 180).render_at(0.5)
    };
    let (flat, solid) = (draw("flat"), draw("toon_3d"));
    assert_ne!(flat.data(), solid.data());
    assert_eq!(solid.data(), draw("toon_3d").data());
    let diff = flat.data().chunks(4).zip(solid.data().chunks(4)).filter(|(a, b)| a != b).count();
    assert!(diff > 500, "the 3D props must visibly differ: {} px", diff);
}

#[test]
fn a_bad_look_is_reported() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap();
    let set = std::fs::read_to_string(root.join("library/sets/diner.onset")).unwrap().replace("height: 3m", "height: 3m\n  look: plastic");
    let (file, mut d) = oneiro_lang::parse(&set, "diner.onset");
    let _ = oneiro_core::SetDef::from_block(&file.blocks[0], "diner.onset", &mut d);
    assert!(d.iter().any(|x| x.code == "E1120"), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
}

#[test]
fn obj_models_show_up_in_the_render() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/models_demo").canonicalize().unwrap();
    let mut d = vec![];
    let p = Project::load(&root, &mut d);
    let text = std::fs::read_to_string(root.join("script.onr")).unwrap();
    let (e, d) = compile_source(&p, "script.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()));
    let pm = Renderer::new(&e, 320, 180).render_at(0.2);
    // The crate is #B5833F: plenty of brownish-orange pixels.
    let n = pm.data().chunks(4).filter(|c| c[0] > 150 && c[1] > 100 && c[1] < 160 && c[2] < 100).count();
    assert!(n > 300, "crate pixels: {}", n);
}

#[test]
fn open_sets_draw_sky_ground_backdrops_and_animated_sprites() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/beach").canonicalize().unwrap();
    let mut d = vec![];
    let p = Project::load(&root, &mut d);
    let text = std::fs::read_to_string(root.join("script.onr")).unwrap();
    let (e, d) = compile_source(&p, "script.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()), "{:?}", d.iter().map(|x| x.render()).collect::<Vec<_>>());
    let r = Renderer::new(&e, 320, 180);
    let a = r.render_at(1.0);
    let px = |x: usize, y: usize| {
        let i = (y * 320 + x) * 4;
        [a.data()[i], a.data()[i + 1], a.data()[i + 2]]
    };
    assert!(px(160, 5)[2] > 200 && px(160, 5)[0] < 140, "sky blue at the top: {:?}", px(160, 5));
    assert!(px(160, 175)[0] > 200 && px(160, 175)[2] < 170, "sand at the bottom: {:?}", px(160, 175));
    assert!(px(160, 70)[2] > px(160, 70)[0], "water from the north backdrop: {:?}", px(160, 70));
    // Sprites move with the time, and the render is reproducible.
    assert_eq!(a.data(), r.render_at(1.0).data());
    assert_ne!(a.data(), r.render_at(1.4).data());
    // The other sides are different pictures.
    let east = r.render_at(8.0);
    assert_ne!(east.data(), a.data());
}

#[test]
fn open_3d_sets_have_real_geometry_seen_from_every_side() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/village").canonicalize().unwrap();
    let mut d = vec![];
    let p = Project::load(&root, &mut d);
    let text = std::fs::read_to_string(root.join("script.onr")).unwrap();
    let (e, d) = compile_source(&p, "script.onr", &text, &Lock::default());
    assert!(d.iter().all(|x| !x.is_error()));
    let r = Renderer::new(&e, 320, 180);
    let south = r.render_at(1.0);
    let north = r.render_at(8.5);
    assert_ne!(south.data(), north.data(), "the other side shows different things");
    assert_eq!(south.data(), r.render_at(1.0).data());
    // Red roofs (house) and green hills exist in the south view.
    let count = |pm: &tiny_skia::Pixmap, f: &dyn Fn([u8; 3]) -> bool| pm.data().chunks(4).filter(|c| f([c[0], c[1], c[2]])).count();
    assert!(count(&south, &|c| c[0] > 170 && c[1] < 100 && c[2] < 90) > 150, "red roofs");
    assert!(count(&south, &|c| c[1] as i32 > c[0] as i32 + 40 && c[1] as i32 > c[2] as i32 + 20) > 3000, "green hills and grass");
}
