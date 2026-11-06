//! The 3D toon engine: meshes, the software rasterizer and the shading.

use oneiro_toon3d::mesh::*;
use oneiro_toon3d::raster::{render, shade, Camera, Style};

fn cam_at(z: f64, w: u32, h: u32) -> Camera {
    Camera { pos: [0.0, 0.0, -z], right: [1.0, 0.0, 0.0], up: [0.0, 1.0, 0.0], fwd: [0.0, 0.0, 1.0], f: h as f64, cx: w as f64 / 2.0, cy: h as f64 / 2.0 }
}

fn px(pm: &tiny_skia::Pixmap, x: u32, y: u32) -> [u8; 4] {
    let d = &pm.data()[((y * pm.width() + x) * 4) as usize..][..4];
    [d[0], d[1], d[2], d[3]]
}

const STYLE: Style = Style { tint: [1.0, 1.0, 1.0], outline_px: 0.0 };

#[test]
fn every_primitive_has_outward_unit_normals_and_finite_vertices() {
    let c = [0.3, 1.0, -0.2];
    let mut shapes: Vec<(&str, Mesh, V3)> = Vec::new();
    let mut m = Mesh::default();
    m.sphere(c, 0.5, [1.0, 0.0, 0.0]);
    shapes.push(("sphere", m, c));
    let mut m = Mesh::default();
    m.ellipsoid(&Xf::scaled(c, [0.4, 0.8, 0.3]), 0.55, 0.6, [1.0, 0.0, 0.0]);
    shapes.push(("rounded box", m, c));
    let mut m = Mesh::default();
    m.capsule([0.0, 0.0, 0.0], [0.0, 1.0, 0.0], 0.1, 0.07, [0.0; 3]);
    shapes.push(("capsule", m, [0.0, 0.5, 0.0]));
    let mut m = Mesh::default();
    m.cylinder([0.0, 0.0, 0.0], 0.4, 0.3, [0.0; 3]);
    shapes.push(("cylinder", m, [0.0, 0.2, 0.0]));
    let mut m = Mesh::default();
    m.ellipsoid_part(&Xf::scaled([0.0; 3], [1.0, 1.0, 1.0]), 1.0, 1.0, 20.0, 90.0, [0.0; 3], true);
    shapes.push(("cap", m, [0.0; 3]));
    for (name, m, center) in &shapes {
        assert!(m.triangles() > 40, "{} has {} triangles", name, m.triangles());
        assert!(m.i.iter().all(|i| (*i as usize) < m.v.len()), "{} has a bad index", name);
        let mut outward = 0;
        for v in &m.v {
            assert!(v.p.iter().chain(v.n.iter()).all(|x| x.is_finite()), "{} has NaN", name);
            assert!((len(v.n) - 1.0).abs() < 1e-3, "{}: normal length {}", name, len(v.n));
            if dot(v.n, sub(v.p, *center)) > 0.0 {
                outward += 1;
            }
        }
        assert!(outward as f32 > 0.95 * m.v.len() as f32, "{}: only {}/{} normals point outward", name, outward, m.v.len());
        // Triangle winding agrees with the normals (so back-face culling is right).
        let mut agree = 0;
        for t in m.i.chunks_exact(3) {
            let (a, b, c2) = (m.v[t[0] as usize], m.v[t[1] as usize], m.v[t[2] as usize]);
            if dot(cross(sub(b.p, a.p), sub(c2.p, a.p)), add(add(a.n, b.n), c2.n)) > 0.0 {
                agree += 1;
            }
        }
        assert!(agree as f32 > 0.97 * m.triangles() as f32, "{}: winding disagrees with normals ({}/{})", name, agree, m.triangles());
    }
}

#[test]
fn a_capsule_spans_its_two_ends_and_a_sheet_is_double_sided() {
    let mut m = Mesh::default();
    m.capsule([1.0, 0.0, 0.0], [1.0, 2.0, 0.0], 0.2, 0.1, [0.0; 3]);
    let ys: Vec<f32> = m.v.iter().map(|v| v.p[1]).collect();
    let (lo, hi) = (ys.iter().cloned().fold(f32::MAX, f32::min), ys.iter().cloned().fold(f32::MIN, f32::max));
    assert!((lo + 0.2).abs() < 1e-3 && (hi - 2.1).abs() < 1e-3, "{} .. {}", lo, hi);

    let pts: Vec<V3> = (0..3).flat_map(|r| (0..3).map(move |c| [c as f32 * 0.1, -(r as f32) * 0.1, 0.0])).collect();
    let mut s = Mesh::default();
    s.sheet(&pts, 3, 3, 0.01, [1.0, 0.0, 0.0]);
    assert_eq!(s.triangles(), 2 * 4 * 2, "two faces of 8 triangles");
    // Both sides present: normals pointing +z and -z.
    assert!(s.v.iter().any(|v| v.n[2] > 0.9) && s.v.iter().any(|v| v.n[2] < -0.9));
}

#[test]
fn a_sphere_renders_as_a_lit_disc_with_soft_edges() {
    let mut m = Mesh::default();
    m.sphere([0.0, 0.0, 0.0], 0.5, [0.8, 0.2, 0.2]);
    let (w, h) = (200u32, 200u32);
    let pm = render(&m, &cam_at(3.0, w, h), w, h, &STYLE);
    // The disc has radius f*r/z = 200*0.5/3 = 33 px.
    assert_eq!(px(&pm, 5, 5)[3], 0, "corners stay transparent");
    assert_eq!(px(&pm, 100, 100)[3], 255, "the center is covered");
    assert_eq!(px(&pm, 100, 100 + 30)[3], 255);
    assert_eq!(px(&pm, 100 + 60, 100)[3], 0);
    let covered = pm.data().chunks(4).filter(|p| p[3] > 0).count() as f32;
    let expect = std::f32::consts::PI * 33.3 * 33.3;
    assert!((covered / expect - 1.0).abs() < 0.08, "area {} vs {}", covered, expect);
    // Anti-aliased edge: some pixels are partially covered.
    assert!(pm.data().chunks(4).any(|p| p[3] > 20 && p[3] < 235), "supersampling gives soft edges");
    // Light from the upper left: that side is brighter than the lower right (cel bands).
    let lit = px(&pm, 100 - 14, 100 - 14);
    let dark = px(&pm, 100 + 14, 100 + 14);
    assert!(lit[0] as i32 > dark[0] as i32 + 20, "lit {:?} dark {:?}", lit, dark);
    // Cel shading: only a few distinct brightness levels across the sphere (flat bands), not a smooth ramp.
    let mut levels: std::collections::BTreeSet<u8> = std::collections::BTreeSet::new();
    for y in 80..120 {
        for x in 80..120 {
            let p = px(&pm, x, y);
            if p[3] == 255 {
                levels.insert(p[0] / 32);
            }
        }
    }
    assert!(levels.len() >= 2 && levels.len() <= 5, "toon bands, got {} distinct levels", levels.len());
}

#[test]
fn ink_outlines_surround_the_silhouette_and_nearer_objects_hide_farther_ones() {
    let mut m = Mesh::default();
    m.sphere([0.0, 0.0, 0.0], 0.5, [0.9, 0.9, 0.2]);
    let (w, h) = (200u32, 200u32);
    let with = render(&m, &cam_at(3.0, w, h), w, h, &Style { tint: [1.0; 3], outline_px: 3.0 });
    let without = render(&m, &cam_at(3.0, w, h), w, h, &STYLE);
    let ink = |pm: &tiny_skia::Pixmap| pm.data().chunks(4).filter(|p| p[3] > 200 && p[0] < 40 && p[1] < 40 && p[2] < 40).count();
    assert!(ink(&with) > 300 && ink(&without) == 0, "outline pixels: {} vs {}", ink(&with), ink(&without));
    // The outline sits outside the disc: a ring wider than the bare disc.
    assert!(with.data().chunks(4).filter(|p| p[3] > 0).count() > without.data().chunks(4).filter(|p| p[3] > 0).count());
    // Width control.
    let thin = render(&m, &cam_at(3.0, w, h), w, h, &Style { tint: [1.0; 3], outline_px: 1.0 });
    assert!(ink(&thin) < ink(&with));

    // Depth: a red sphere in front of a blue one hides it where they overlap.
    let mut m = Mesh::default();
    m.sphere([0.12, 0.0, 0.8], 0.5, [0.1, 0.1, 0.9]);
    m.sphere([-0.12, 0.0, 0.0], 0.5, [0.9, 0.1, 0.1]);
    let pm = render(&m, &cam_at(3.0, w, h), w, h, &STYLE);
    let c = px(&pm, 100, 100);
    assert!(c[0] > c[2], "the near (red) sphere wins the overlap: {:?}", c);
    let right = px(&pm, 100 + 27, 100);
    assert!(right[2] > right[0], "the far (blue) sphere shows where the near one does not reach: {:?}", right);
}

#[test]
fn shading_bands_tint_and_gloss() {
    let n_up = [0.0, 1.0, 0.0];
    let light = [0.0, 1.0, 0.0];
    let view = norm([0.0, 0.5, -1.0]);
    let base = [0.5, 0.5, 0.5];
    let lit = shade(base, n_up, view, light, [1.0; 3]);
    let shadow = shade(base, [0.0, -1.0, 0.0], view, light, [1.0; 3]);
    assert!(lit[0] > shadow[0] * 1.4, "lit {:?} shadow {:?}", lit, shadow);
    // Night tint darkens and blues.
    let night = shade(base, n_up, view, light, [0.5, 0.55, 0.8]);
    assert!(night[0] < lit[0] && night[2] > night[0]);
    // A mirror-like normal gets the toon highlight (brighter than the plain lit band).
    let h = norm(add(light, view));
    let gloss = shade(base, h, view, light, [1.0; 3]);
    assert!(gloss[0] > lit[0] + 0.05, "gloss {:?} lit {:?}", gloss, lit);
    assert!(lit.iter().chain(shadow.iter()).all(|c| (0.0..=1.0).contains(c)));
}

#[test]
fn geometry_behind_the_camera_or_empty_meshes_do_not_crash() {
    let pm = render(&Mesh::default(), &cam_at(3.0, 64, 64), 64, 64, &STYLE);
    assert!(pm.data().chunks(4).all(|p| p[3] == 0));
    let mut m = Mesh::default();
    m.sphere([0.0, 0.0, -10.0], 0.5, [1.0, 0.0, 0.0]); // behind the camera at z = -3
    let pm = render(&m, &cam_at(3.0, 64, 64), 64, 64, &Style { tint: [1.0; 3], outline_px: 2.0 });
    assert!(pm.data().chunks(4).all(|p| p[3] == 0));
    // A sphere around the camera (near plane clipping) must not panic.
    let mut m = Mesh::default();
    m.sphere([0.0, 0.0, -3.0], 0.5, [1.0, 0.0, 0.0]);
    let _ = render(&m, &cam_at(3.0, 64, 64), 64, 64, &Style { tint: [1.0; 3], outline_px: 2.0 });
}
