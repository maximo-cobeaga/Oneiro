//! The picture an AI (or a person) looks at: the illustration with the boxes drawn over it and a
//! numbered marker on each (1 head, 2 body, 3 arm, 4 leg). The mirrored arm and leg are drawn faint.

use crate::layout::{Layout, RectN};
use crate::Image;
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Rect, Stroke, Transform};

pub const COLORS: [(u8, u8, u8); 4] = [(230, 70, 70), (60, 180, 90), (70, 120, 235), (240, 150, 30)];

// 3x5 digits, one string per row.
const DIGITS: [[&str; 5]; 4] = [["010", "110", "010", "010", "111"], ["111", "001", "111", "100", "111"], ["111", "001", "111", "001", "111"], ["101", "101", "111", "001", "001"]];

fn paint(c: (u8, u8, u8), a: u8) -> Paint<'static> {
    let mut p = Paint::default();
    p.anti_alias = true;
    p.set_color(Color::from_rgba8(c.0, c.1, c.2, a));
    p
}

fn rect_path(r: RectN, w: f32, h: f32) -> Option<tiny_skia::Path> {
    let rr = Rect::from_ltrb(r.x0 * w, r.y0 * h, (r.x1 * w).max(r.x0 * w + 1.0), (r.y1 * h).max(r.y0 * h + 1.0))?;
    Some(PathBuilder::from_rect(rr))
}

/// Draws the layout over a copy of the image.
pub fn draw(img: &Image, layout: &Layout, cx: Option<f32>) -> Pixmap {
    let mut pm = img.pm.clone();
    let (w, h) = (pm.width() as f32, pm.height() as f32);
    let t = (w.max(h) / 320.0).max(1.5);
    let stroke = |width: f32| Stroke { width, line_cap: LineCap::Butt, line_join: LineJoin::Miter, ..Default::default() };
    for (i, (_, r)) in layout.rects().iter().enumerate() {
        let col = COLORS[i];
        if let Some(path) = rect_path(*r, w, h) {
            pm.fill_path(&path, &paint(col, 28), FillRule::Winding, Transform::identity(), None);
            pm.stroke_path(&path, &paint(col, 255), &stroke(t), Transform::identity(), None);
        }
        // The mirrored side of arm and leg.
        if let (Some(cx), true) = (cx, i >= 2) {
            if let Some(path) = rect_path(r.mirrored(cx), w, h) {
                pm.stroke_path(&path, &paint(col, 110), &stroke(t * 0.7), Transform::identity(), None);
            }
        }
        // Numbered badge at the top-left corner.
        let cell = (t * 2.2).max(3.0);
        let (bw, bh) = (cell * 5.0, cell * 7.0);
        let (bx, by) = ((r.x0 * w).min(w - bw), (r.y0 * h).min(h - bh));
        if let Some(bg) = Rect::from_xywh(bx, by, bw, bh) {
            pm.fill_rect(bg, &paint(col, 255), Transform::identity(), None);
        }
        for (ry, row) in DIGITS[i].iter().enumerate() {
            for (rx, ch) in row.chars().enumerate() {
                if ch == '1' {
                    if let Some(cell_r) = Rect::from_xywh(bx + cell * (1.0 + rx as f32), by + cell * (1.0 + ry as f32), cell, cell) {
                        pm.fill_rect(cell_r, &paint((255, 255, 255), 255), Transform::identity(), None);
                    }
                }
            }
        }
    }
    pm
}
