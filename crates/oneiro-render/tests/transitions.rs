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
