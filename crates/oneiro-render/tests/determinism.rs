use oneiro_core::lock::Lock;
use oneiro_core::{compile, Project};
use oneiro_render::{hash_pixmap, render_range, Renderer};
use std::path::Path;

fn episode() -> oneiro_core::Episode {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/first_short").canonicalize().unwrap();
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    let (ep, d2) = compile(&project, &root.join("script.onr"), &Lock::default());
    assert!(d2.iter().all(|x| !x.is_error()), "{:?}", d2);
    ep
}

#[test]
fn same_frame_is_byte_identical() {
    let ep = episode();
    let r = Renderer::new(&ep, 480, 270);
    for f in [0usize, 150, 300, 500, 650] {
        let a = r.render_frame(f);
        let b = r.render_frame(f);
        assert_eq!(a.data(), b.data(), "frame {} differs", f);
    }
}

#[test]
fn parallel_render_matches_serial() {
    let ep = episode();
    let r = Renderer::new(&ep, 320, 180);
    let serial: Vec<u64> = (100..132).map(|i| hash_pixmap(&r.render_frame(i))).collect();
    let mut parallel = Vec::new();
    render_range(&r, 100..132, |_, pm| {
        parallel.push(hash_pixmap(pm));
        Ok(())
    })
    .unwrap();
    assert_eq!(serial, parallel);
}

#[test]
fn frames_are_not_blank_and_change_over_time() {
    let ep = episode();
    let r = Renderer::new(&ep, 320, 180);
    let a = r.render_frame(10);
    let b = r.render_frame(400);
    assert_ne!(hash_pixmap(&a), hash_pixmap(&b));
    let px = a.data();
    let distinct: std::collections::HashSet<[u8; 4]> = px.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect();
    assert!(distinct.len() > 20);
    assert!(px.chunks(4).all(|c| c[3] == 255));
}
