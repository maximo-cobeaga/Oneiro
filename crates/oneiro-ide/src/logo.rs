//! The Oneiro logo ("oneiro" is the Greek word for dream): a dream badge with a crescent moon cradling a play
//! button, sparkles around it, on a violet-to-magenta-to-orange sky. Drawn in code so the program needs no image
//! file; `docs/img/logo.svg` is the same drawing as a vector file.

use tiny_skia::{BlendMode, Color, FillRule, GradientStop, LinearGradient, Paint, Path, PathBuilder, Pixmap, PixmapPaint, Point, RadialGradient, Rect, SpreadMode, Transform};

fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgba8(r, g, b, 255)
}

fn rounded_square(size: f32, radius: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let (s, r) = (size, radius);
    pb.move_to(r, 0.0);
    pb.line_to(s - r, 0.0);
    pb.quad_to(s, 0.0, s, r);
    pb.line_to(s, s - r);
    pb.quad_to(s, s, s - r, s);
    pb.line_to(r, s);
    pb.quad_to(0.0, s, 0.0, s - r);
    pb.line_to(0.0, r);
    pb.quad_to(0.0, 0.0, r, 0.0);
    pb.close();
    pb.finish()
}

fn sparkle(cx: f32, cy: f32, r: f32) -> Option<Path> {
    let mut pb = PathBuilder::new();
    let k = r * 0.22;
    pb.move_to(cx, cy - r);
    pb.quad_to(cx + k * 0.3, cy - k * 0.3, cx + r, cy);
    pb.quad_to(cx + k * 0.3, cy + k * 0.3, cx, cy + r);
    pb.quad_to(cx - k * 0.3, cy + k * 0.3, cx - r, cy);
    pb.quad_to(cx - k * 0.3, cy - k * 0.3, cx, cy - r);
    pb.close();
    pb.finish()
}

fn fill(pm: &mut Pixmap, path: &Path, c: Color) {
    let mut p = Paint::default();
    p.anti_alias = true;
    p.set_color(c);
    pm.fill_path(path, &p, FillRule::Winding, Transform::identity(), None);
}

/// The logo as a `size` x `size` image.
pub fn render(size: u32) -> Pixmap {
    let n = size.max(16) as f32;
    let mut art = Pixmap::new(size.max(16), size.max(16)).unwrap();
    let k = n / 256.0; // the drawing is designed on a 256 grid

    // Sky: violet on top, magenta in the middle, orange at the bottom.
    let mut p = Paint::default();
    p.anti_alias = true;
    if let Some(sh) = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(n * 0.35, n),
        vec![GradientStop::new(0.0, rgb(52, 24, 150)), GradientStop::new(0.5, rgb(214, 40, 150)), GradientStop::new(1.0, rgb(255, 150, 40))],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        p.shader = sh;
        if let Some(r) = Rect::from_xywh(0.0, 0.0, n, n) {
            art.fill_rect(r, &p, Transform::identity(), None);
        }
    }
    // A warm glow behind the moon.
    let mut glow = Paint::default();
    glow.anti_alias = true;
    if let Some(sh) = RadialGradient::new(
        Point::from_xy(128.0 * k, 122.0 * k),
        Point::from_xy(128.0 * k, 122.0 * k),
        118.0 * k,
        vec![GradientStop::new(0.0, Color::from_rgba8(255, 230, 120, 150)), GradientStop::new(1.0, Color::from_rgba8(255, 230, 120, 0))],
        SpreadMode::Pad,
        Transform::identity(),
    ) {
        glow.shader = sh;
        if let Some(r) = Rect::from_xywh(0.0, 0.0, n, n) {
            art.fill_rect(r, &glow, Transform::identity(), None);
        }
    }
    // Crescent moon, on its own layer so the bite can be cut out.
    let mut moon = Pixmap::new(size.max(16), size.max(16)).unwrap();
    if let Some(c) = PathBuilder::from_circle(122.0 * k, 130.0 * k, 78.0 * k) {
        fill(&mut moon, &c, rgb(255, 232, 130));
    }
    if let Some(c) = PathBuilder::from_circle(160.0 * k, 106.0 * k, 66.0 * k) {
        let mut cut = Paint::default();
        cut.anti_alias = true;
        cut.blend_mode = BlendMode::Clear;
        cut.set_color(rgb(0, 0, 0));
        moon.fill_path(&c, &cut, FillRule::Winding, Transform::identity(), None);
    }
    art.draw_pixmap(0, 0, moon.as_ref(), &PixmapPaint::default(), Transform::identity(), None);
    // Play button (animation) resting in the crescent's mouth.
    let mut pb = PathBuilder::new();
    pb.move_to(142.0 * k, 96.0 * k);
    pb.line_to(142.0 * k, 160.0 * k);
    pb.line_to(196.0 * k, 128.0 * k);
    pb.close();
    if let Some(tri) = pb.finish() {
        fill(&mut art, &tri, rgb(40, 214, 200));
    }
    // Sparkles
    for (x, y, r, c) in [(60.0, 52.0, 20.0, rgb(255, 255, 255)), (206.0, 48.0, 13.0, rgb(120, 240, 255)), (44.0, 176.0, 11.0, rgb(255, 240, 150)), (204.0, 206.0, 17.0, rgb(255, 255, 255))] {
        if let Some(s) = sparkle(x * k, y * k, r * k) {
            fill(&mut art, &s, c);
        }
    }
    // Round the badge corners by keeping only the rounded square.
    let mut out = Pixmap::new(size.max(16), size.max(16)).unwrap();
    if let Some(sq) = rounded_square(n, 56.0 * k) {
        let mut mask = Paint::default();
        mask.anti_alias = true;
        mask.shader = tiny_skia::Pattern::new(art.as_ref(), SpreadMode::Pad, tiny_skia::FilterQuality::Nearest, 1.0, Transform::identity());
        out.fill_path(&sq, &mask, FillRule::Winding, Transform::identity(), None);
    }
    out
}

/// RGBA (straight alpha) bytes for a window icon.
pub fn icon_rgba(size: u32) -> Vec<u8> {
    let pm = render(size);
    pm.data()
        .chunks(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            if a == 0 {
                [0, 0, 0, 0]
            } else {
                [(p[0] as u32 * 255 / a).min(255) as u8, (p[1] as u32 * 255 / a).min(255) as u8, (p[2] as u32 * 255 / a).min(255) as u8, p[3]]
            }
        })
        .collect()
}
