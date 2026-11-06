//! Set props as real 3D meshes (`look: toon_3d`): counter, stool, table and pie case built from boxes and
//! lathed shapes, in the same world coordinates as the 2D set so the camera matches exactly.

use crate::mesh::*;
use oneiro_core::defs::{PropInst, SetDef};
use oneiro_core::Rgb;

fn col(c: Rgb) -> V3 {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}


/// Props that have a 3D version.
pub fn supported(pr: &PropInst) -> bool {
    pr.wall.is_none() && (matches!(pr.kind.as_str(), "counter" | "stool" | "table" | "pie_case") || (pr.kind == "model" && pr.mesh.is_some()))
}

/// World-space mesh of one piece of a prop.
pub fn prop_mesh(set: &SetDef, pr: &PropInst, piece: usize) -> Mesh {
    let mut m = Mesh::default();
    let yaw = pr.yaw as f32;
    let (sy, cy) = ((pr.yaw).sin() as f32, (pr.yaw).cos() as f32);
    let at = [pr.at[0] as f32, pr.at[1] as f32];
    let steel = col(Rgb(190, 190, 198));
    match pr.kind.as_str() {
        "counter" => {
            let _ = piece; // drawn whole: slicing it would leave seams and outlines
            let (pw, d, h) = (pr.w.unwrap_or(5.0) as f32, pr.d.unwrap_or(0.8) as f32, pr.h.unwrap_or(1.0) as f32);
            let c = at;
            let body = col(pr.color.unwrap_or(set.trim));
            m.cuboid(c, 0.0, [pw, h - 0.05, d], yaw, body);
            m.cuboid(c, h - 0.05, [pw + 0.004, 0.07, d + 0.14], yaw, col(Rgb(232, 232, 236)));
            m.cuboid(c, 0.06, [pw + 0.004, 0.06, d + 0.01], yaw, steel);
        }
        "stool" => {
            let seat_y = pr.h.unwrap_or(0.6) as f32;
            m.cylinder([at[0], 0.0, at[1]], 0.035, 0.2, col(Rgb(170, 170, 178)));
            m.cylinder([at[0], 0.03, at[1]], seat_y - 0.1, 0.035, steel);
            m.cylinder([at[0], seat_y - 0.07, at[1]], 0.08, 0.2, col(pr.color.unwrap_or(Rgb(196, 44, 56))));
        }
        "table" => {
            let (w, d, h) = (pr.w.unwrap_or(1.2) as f32, pr.d.unwrap_or(0.8) as f32, pr.h.unwrap_or(0.74) as f32);
            for (i, k) in [(-1.0f32, -1.0f32), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let (a, b) = (i * (w / 2.0 - 0.08), k * (d / 2.0 - 0.08));
                m.cuboid([at[0] + cy * a + sy * b, at[1] - sy * a + cy * b], 0.0, [0.06, h - 0.04, 0.06], yaw, steel);
            }
            m.cuboid(at, h - 0.05, [w, 0.05, d], yaw, col(pr.color.unwrap_or(Rgb(236, 230, 214))));
        }
        "pie_case" => {
            let (w, d) = (pr.w.unwrap_or(0.9) as f32, pr.d.unwrap_or(0.5) as f32);
            let y0 = if pr.y > 0.0 { pr.y as f32 } else { 1.05 };
            m.cuboid(at, y0, [w, 0.08, d], yaw, col(Rgb(210, 210, 216)));
            for (k, c) in [Rgb(214, 160, 90), Rgb(196, 56, 70), Rgb(240, 222, 150)].into_iter().enumerate() {
                let o = (k as f32 - 1.0) * w * 0.3;
                m.cylinder([at[0] + cy * o, y0 + 0.08, at[1] - sy * o], 0.06, 0.12, col(c));
            }
        }
        "model" => {
            if let Some(obj) = &pr.mesh {
                add_model(&mut m, pr, obj);
            }
        }
        _ => {}
    }
    m
}

/// An OBJ model: scaled to the prop's `height` (default 1 m), centered on `at`, standing on the floor
/// (plus `height_at`), turned by `yaw`. Normals are smooth across edges flatter than 50 degrees.
fn add_model(m: &mut Mesh, pr: &PropInst, obj: &oneiro_core::obj::ObjModel) {
    let (lo, hi) = obj.bounds();
    let hgt = (hi[1] - lo[1]).max(1e-6);
    let k = pr.h.unwrap_or(1.0) as f32 / hgt;
    let (sy, cy) = ((pr.yaw).sin() as f32, (pr.yaw).cos() as f32);
    let ctr = [(lo[0] + hi[0]) / 2.0, (lo[2] + hi[2]) / 2.0];
    let place = |v: [f32; 3]| -> V3 {
        let (lx, lz) = ((v[0] - ctr[0]) * k, (v[2] - ctr[1]) * k);
        [pr.at[0] as f32 + cy * lx + sy * lz, (v[1] - lo[1]) * k + pr.y as f32, pr.at[1] as f32 - sy * lx + cy * lz]
    };
    let pos: Vec<V3> = obj.verts.iter().map(|v| place(*v)).collect();
    // Orientation: models are assumed counter-clockwise; flip them all if most faces point inward.
    let centroid = {
        let n = pos.len() as f32;
        let s = pos.iter().fold([0.0f32; 3], |a, p| add(a, *p));
        mul(s, 1.0 / n)
    };
    let face_n = |t: &[u32; 3]| cross(sub(pos[t[1] as usize], pos[t[0] as usize]), sub(pos[t[2] as usize], pos[t[0] as usize]));
    let outward: f32 = obj.tris.iter().map(|t| dot(face_n(t), sub(pos[t[0] as usize], centroid))).sum();
    let flip = outward < 0.0;
    let faces: Vec<V3> = obj.tris.iter().map(|t| norm(face_n(t))).collect();
    let mut at_vertex: Vec<Vec<usize>> = vec![vec![]; pos.len()];
    for (fi, t) in obj.tris.iter().enumerate() {
        for &v in t {
            at_vertex[v as usize].push(fi);
        }
    }
    let color = pr.color.map(|c| [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]).unwrap_or([0.78, 0.74, 0.68]);
    let crease = 50.0f32.to_radians().cos();
    for (fi, t) in obj.tris.iter().enumerate() {
        let base = m.v.len() as u32;
        for &v in t {
            let mut n = [0.0f32; 3];
            for &g in &at_vertex[v as usize] {
                if dot(faces[g], faces[fi]) >= crease {
                    n = add(n, faces[g]);
                }
            }
            let n = norm(n);
            m.v.push(Vtx { p: pos[v as usize], n: if flip { mul(n, -1.0) } else { n }, c: color });
        }
        if flip {
            m.i.extend_from_slice(&[base, base + 2, base + 1]);
        } else {
            m.i.extend_from_slice(&[base, base + 1, base + 2]);
        }
    }
}
