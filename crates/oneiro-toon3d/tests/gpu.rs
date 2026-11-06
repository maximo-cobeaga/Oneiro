//! The GPU backend must draw what the CPU backend draws. Skipped (with a message) when the machine has no
//! graphics adapter.

#![cfg(feature = "gpu")]

use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, Project};
use oneiro_toon3d::mesh::*;
use oneiro_toon3d::raster::{self, Camera, Style};
use oneiro_toon3d::{character_mesh, gpu, Backend};
use std::path::{Path, PathBuf};
use tiny_skia::Pixmap;

fn cam(z: f64, w: u32, h: u32) -> Camera {
    Camera { pos: [0.0, 0.0, -z], right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], fwd: [0.0, 0.0, 1.0], f: h as f64, cx: w as f64 / 2.0, cy: h as f64 / 2.0 }
}

/// (coverage agreement, mean absolute color difference over pixels both cover).
fn compare(a: &Pixmap, b: &Pixmap) -> (f32, f32) {
    let (mut both, mut either, mut diff, mut n) = (0usize, 0usize, 0u64, 0u64);
    for (x, y) in a.data().chunks(4).zip(b.data().chunks(4)) {
        let (ca, cb) = (x[3] > 127, y[3] > 127);
        if ca || cb {
            either += 1;
        }
        if ca && cb {
            both += 1;
            if x[3] == 255 && y[3] == 255 {
                for k in 0..3 {
                    diff += (x[k] as i32 - y[k] as i32).unsigned_abs() as u64;
                    n += 1;
                }
            }
        }
    }
    (both as f32 / either.max(1) as f32, diff as f32 / n.max(1) as f32)
}

fn gallery() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/toon3d_demo").canonicalize().unwrap()
}

#[test]
fn the_gpu_draws_what_the_cpu_draws() {
    let Some(g) = gpu::gpu() else {
        eprintln!("no graphics adapter: skipped");
        return;
    };
    eprintln!("GPU adapter: {}", g.adapter_name);
    let style = Style { tint: [1.0, 1.0, 1.0], outline_px: 3.0 };

    // Simple shapes first: a sphere and a capsule, with outlines.
    let mut m = Mesh::default();
    m.sphere([0.0, 0.2, 0.0], 0.5, [0.9, 0.3, 0.2]);
    m.capsule([-0.6, -0.8, 0.2], [0.6, -0.5, 0.0], 0.12, 0.09, [0.2, 0.5, 0.9]);
    let (w, h) = (256u32, 256u32);
    let (c, gp) = (raster::render(&m, &cam(3.0, w, h), w, h, &style), g.render(&m, &cam(3.0, w, h), w, h, &style).expect("the GPU render"));
    let (iou, mean) = compare(&c, &gp);
    assert!(iou > 0.97, "silhouettes agree: {}", iou);
    assert!(mean < 9.0, "colors agree: mean abs diff {}", mean);
    // Ink outline and depth order survive on the GPU too.
    assert!(gp.data().chunks(4).filter(|p| p[3] > 200 && p[0] < 40 && p[1] < 40 && p[2] < 45).count() > 300, "the GPU draws the ink outline");
    assert_eq!(gp.data()[3], 0, "transparent corner");

    // A real character with a cape, at an off-center camera.
    let root = gallery();
    let mut d = vec![];
    let project = Project::load(&root, &mut d);
    let script = "oneiro 0.1\nproject \"t\"\nuse character Hero\nuse set diner\nscene 1 \"s\" in diner {\n  place Hero at diner.door\n  camera wide\n  Hero runs_to (1.0, 0.0)\n  wait 1s\n}\n";
    let (ep, diags) = compile_source(&project, "script.onr", script, &Lock::default());
    assert!(diags.iter().all(|x| !x.is_error()));
    let sc = &ep.scenes[0];
    let fr = sc.frames_at(1.0).remove(0);
    let hero = &sc.actors[0];
    let mesh = character_mesh(&hero.def, &fr, hero.physics.as_deref()).mesh;
    assert!(mesh.triangles() > 5000, "{} triangles", mesh.triangles());
    let kin = &fr.kin;
    let camera = Camera { pos: [kin.pos[0] + 0.5, 1.0, kin.pos[1] - 3.0], right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], fwd: [0.0, 0.0, 1.0], f: 700.0, cx: 300.0, cy: 380.0 };
    let (w, h) = (600u32, 760u32);
    let (c, gp) = (raster::render(&mesh, &camera, w, h, &style), g.render(&mesh, &camera, w, h, &style).expect("the GPU render"));
    let (iou, mean) = compare(&c, &gp);
    assert!(iou > 0.96, "character silhouettes agree: {}", iou);
    assert!(mean < 12.0, "character colors agree: mean abs diff {}", mean);
}

#[test]
fn backend_choice_and_fallback() {
    // Asking for the CPU never touches the GPU and is always exact.
    let mut m = Mesh::default();
    m.sphere([0.0, 0.0, 0.0], 0.5, [0.5, 0.5, 0.5]);
    let style = Style { tint: [1.0; 3], outline_px: 0.0 };
    let a = oneiro_toon3d::render_mesh(&m, &cam(3.0, 64, 64), 64, 64, &style, Backend::Cpu);
    let b = raster::render(&m, &cam(3.0, 64, 64), 64, 64, &style);
    if std::env::var("ONEIRO_3D").is_err() {
        assert_eq!(a.data(), b.data(), "Backend::Cpu is the software rasterizer");
    }
    // Whatever the backend, the same frame twice is identical on this machine.
    let x = oneiro_toon3d::render_mesh(&m, &cam(3.0, 64, 64), 64, 64, &style, Backend::Auto);
    let y = oneiro_toon3d::render_mesh(&m, &cam(3.0, 64, 64), 64, 64, &style, Backend::Auto);
    assert_eq!(x.data(), y.data());
    // An empty mesh and odd sizes are handled.
    let e = oneiro_toon3d::render_mesh(&Mesh::default(), &cam(3.0, 33, 17), 33, 17, &style, Backend::Auto);
    assert!(e.data().chunks(4).all(|p| p[3] == 0));
}
