//! Drawing an SVG document into a pixmap (used for open-set scenery, which is rasterized once at load time).

use crate::svg::{Seg, SvgDoc};
use tiny_skia::{FillRule, LineCap, LineJoin, Paint, Pixmap, Stroke, Transform};

pub fn seg_path(segs: &[Seg]) -> Option<tiny_skia::Path> {
    let mut pb = tiny_skia::PathBuilder::new();
    for s in segs {
        match *s {
            Seg::Move(x, y) => pb.move_to(x, y),
            Seg::Line(x, y) => pb.line_to(x, y),
            Seg::Quad(a, b, x, y) => pb.quad_to(a, b, x, y),
            Seg::Cubic(a, b, c, d, x, y) => pb.cubic_to(a, b, c, d, x, y),
            Seg::Close => pb.close(),
        }
    }
    pb.finish()
}

impl SvgDoc {
    /// Draws the document into a transparent pixmap `height_px` tall (width follows the aspect ratio).
    pub fn rasterize(&self, height_px: u32) -> Option<Pixmap> {
        let [vx, vy, vw, vh] = self.vb;
        if vw <= 0.0 || vh <= 0.0 {
            return None;
        }
        let h = height_px.clamp(8, 4096);
        let w = ((h as f32 * vw / vh).round() as u32).clamp(8, 8192);
        let mut pm = Pixmap::new(w, h)?;
        let tf = Transform::from_scale(w as f32 / vw, h as f32 / vh).pre_translate(-vx, -vy);
        for sh in &self.shapes {
            let Some(path) = seg_path(&sh.segs) else { continue };
            if let Some(c) = sh.fill {
                let mut p = Paint::default();
                p.anti_alias = true;
                p.set_color_rgba8(c[0], c[1], c[2], c[3]);
                pm.fill_path(&path, &p, FillRule::Winding, tf, None);
            }
            if let Some((c, wd)) = sh.stroke {
                let mut p = Paint::default();
                p.anti_alias = true;
                p.set_color_rgba8(c[0], c[1], c[2], c[3]);
                let s = Stroke { width: wd, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
                pm.stroke_path(&path, &p, &s, tf, None);
            }
        }
        Some(pm)
    }
}
