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
