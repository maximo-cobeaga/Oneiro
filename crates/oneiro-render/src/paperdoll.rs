//! `toon_3d` for characters that are flat drawings (vector parts and cut-out illustrations): a "paper doll".
//! The finished drawing is a thin card standing in the 3D scene. It narrows as the character turns away from
//! the camera, shows a dark paper edge on the side it turns toward, and is a little darker from behind.

use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

/// Horizontal scale of the card for a character whose facing makes `front` = cos(angle to the camera).
pub fn card_scale(front: f64) -> f32 {
    (0.3 + 0.7 * front.abs()) as f32
}

fn silhouette(src: &Pixmap, rgb: [u8; 3]) -> Pixmap {
    let mut out = src.clone();
    for p in out.data_mut().chunks_mut(4) {
        let a = p[3] as u32;
        p[0] = (rgb[0] as u32 * a / 255) as u8;
        p[1] = (rgb[1] as u32 * a / 255) as u8;
        p[2] = (rgb[2] as u32 * a / 255) as u8;
    }
    out
}

/// Draws `layer` (the flat drawing, character anchor at `anchor`) onto `pm` at (`x0`, `y0`) as a turned card.
pub fn compose(pm: &mut Pixmap, layer: &Pixmap, x0: i32, y0: i32, anchor_x: f32, front: f64, side: f64, thickness_px: f32) {
    let s = card_scale(front);
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..PixmapPaint::default() };
    // Squash about the anchor column.
    let ax = x0 as f32 + anchor_x; // the transform works in destination coordinates
    let tf = Transform::from_translate(-ax, 0.0).post_scale(s, 1.0).post_translate(ax, 0.0);
    let mut shaded;
    let card: &Pixmap = if front < 0.0 {
        // From behind: the same drawing, a little darker.
        shaded = layer.clone();
        for p in shaded.data_mut().chunks_mut(4) {
            for c in p.iter_mut().take(3) {
                *c = (*c as f32 * 0.82) as u8;
            }
        }
        &shaded
    } else {
        layer
    };
    let edge = silhouette(layer, [38, 34, 48]);
    let depth = thickness_px * (1.0 - s);
    // The paper edge shows on the side the card turns toward.
    let dir = if side >= 0.0 { 1.0 } else { -1.0 };
    let steps = depth.ceil().max(0.0) as i32;
    for k in (1..=steps).rev() {
        let off = dir * depth * k as f32 / steps as f32;
        pm.draw_pixmap(x0, y0, edge.as_ref(), &paint, tf.post_translate(off, 0.0), None);
    }
    pm.draw_pixmap(x0, y0, card.as_ref(), &paint, tf, None);
}
