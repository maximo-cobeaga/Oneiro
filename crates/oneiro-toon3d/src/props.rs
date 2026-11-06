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
    pr.wall.is_none() && matches!(pr.kind.as_str(), "counter" | "stool" | "table" | "pie_case")
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
        _ => {}
    }
    m
}
