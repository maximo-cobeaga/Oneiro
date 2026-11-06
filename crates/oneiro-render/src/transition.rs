//! Blending two rendered shots across a camera cut: dissolve, fade through black or white, and wipes.

use tiny_skia::Pixmap;

fn lerp(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t.clamp(0.0, 1.0)).round() as u8
}

/// Which of the two shots a transition at mix `m` (0 = all outgoing, 1 = all incoming) needs.
/// Fades only ever show one shot at a time, so the other need not be rendered.
pub fn needs(kind: &str, m: f64) -> (bool, bool) {
    match kind {
        "fade_black" | "fade_white" => (m < 0.5, m >= 0.5),
        _ => (true, true),
    }
}

/// Blends `out` (the shot that is leaving) and `inc` (the one arriving). Either may be `None` when
/// `needs` said it is not needed.
pub fn blend(kind: &str, out: Option<&Pixmap>, inc: Option<&Pixmap>, m: f64) -> Pixmap {
    let reference = out.or(inc).expect("at least one shot");
    let (w, h) = (reference.width() as usize, reference.height() as usize);
    let mut res = Pixmap::new(w as u32, h as u32).expect("pixmap");
    let m = m.clamp(0.0, 1.0) as f32;
    let d = res.data_mut();
    match kind {
        "fade_black" | "fade_white" => {
            let level = if kind == "fade_white" { 255u8 } else { 0 };
            let (src, k) = if m < 0.5 { (out.or(inc).unwrap(), m * 2.0) } else { (inc.or(out).unwrap(), (1.0 - m) * 2.0) };
            for (o, s) in d.chunks_mut(4).zip(src.data().chunks(4)) {
                for c in 0..3 {
                    o[c] = lerp(s[c], level, k);
                }
                o[3] = 255;
            }
        }
        "wipe_left" | "wipe_right" => {
            let (a, b) = (out.unwrap_or_else(|| inc.unwrap()), inc.unwrap_or_else(|| out.unwrap()));
            let edge = (w as f32 * 0.03).max(2.0);
            // wipe_left: the new shot slides in from the right; wipe_right: from the left.
            let boundary = if kind == "wipe_left" { w as f32 * (1.0 - m) } else { w as f32 * m };
            for y in 0..h {
                for x in 0..w {
                    let xf = x as f32 + 0.5;
                    let show_new = if kind == "wipe_left" { ((xf - boundary) / edge + 0.5).clamp(0.0, 1.0) } else { ((boundary - xf) / edge + 0.5).clamp(0.0, 1.0) };
                    let i = (y * w + x) * 4;
                    for c in 0..3 {
                        d[i + c] = lerp(a.data()[i + c], b.data()[i + c], show_new);
                    }
                    d[i + 3] = 255;
                }
            }
        }
        _ => {
            // dissolve
            let (a, b) = (out.unwrap_or_else(|| inc.unwrap()), inc.unwrap_or_else(|| out.unwrap()));
            for ((o, x), y) in d.chunks_mut(4).zip(a.data().chunks(4)).zip(b.data().chunks(4)) {
                for c in 0..3 {
                    o[c] = lerp(x[c], y[c], m);
                }
                o[3] = 255;
            }
        }
    }
    res
}
