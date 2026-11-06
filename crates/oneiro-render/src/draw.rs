//! Low-level drawing helpers: camera projection and a small vector painter on top of tiny-skia.

use oneiro_core::math::*;
use oneiro_core::{CameraPose, Rgb};
use tiny_skia::{Color, FillRule, LineCap, LineJoin, Paint, Path, PathBuilder, Pixmap, Rect, Stroke, Transform};

#[derive(Clone, Debug)]
pub struct Cam {
    pub pos: V3,
    pub fwd: V3,
    pub right: V3,
    pub up: V3,
    /// Focal length in pixels.
    pub f: f64,
    pub cx: f64,
    pub cy: f64,
}

impl Cam {
    pub fn new(p: &CameraPose, w: u32, h: u32) -> Cam {
        let fwd = norm3(sub3(p.target, p.pos));
        let mut right = cross3([0.0, 1.0, 0.0], fwd);
        if len3(right) < 1e-6 {
            right = [1.0, 0.0, 0.0];
        }
        let right = norm3(right);
        let up = cross3(fwd, right);
        let r = p.roll.to_radians();
        let (c, s) = (r.cos(), r.sin());
        let right2 = add3(mul3(right, c), mul3(up, s));
        let up2 = sub3(mul3(up, c), mul3(right, s));
        Cam {
            pos: p.pos,
            fwd,
            right: right2,
            up: up2,
            f: (h as f64 / 2.0) / (p.fov.to_radians() / 2.0).tan(),
            cx: w as f64 / 2.0,
            cy: h as f64 / 2.0,
        }
    }

    pub fn to_cam(&self, p: V3) -> V3 {
        let v = sub3(p, self.pos);
        [dot3(v, self.right), dot3(v, self.up), dot3(v, self.fwd)]
    }

    pub fn project_cam(&self, c: V3) -> (f32, f32) {
        ((self.cx + self.f * c[0] / c[2]) as f32, (self.cy - self.f * c[1] / c[2]) as f32)
    }

    /// Projects a world point; returns screen x, y and depth (None if behind the camera).
    pub fn project(&self, p: V3) -> Option<(f32, f32, f64)> {
        let c = self.to_cam(p);
        if c[2] < 0.05 {
            return None;
        }
        let (x, y) = self.project_cam(c);
        Some((x, y, c[2]))
    }
}

fn clip_near(poly: &[V3], near: f64) -> Vec<V3> {
    let mut out = Vec::with_capacity(poly.len() + 2);
    for i in 0..poly.len() {
        let a = poly[i];
        let b = poly[(i + 1) % poly.len()];
        let ain = a[2] >= near;
        let bin = b[2] >= near;
        if ain {
            out.push(a);
        }
        if ain != bin {
            let t = (near - a[2]) / (b[2] - a[2]);
            out.push([a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, near]);
        }
    }
    out
}

pub struct Painter<'a> {
    pub pm: &'a mut Pixmap,
    pub tf: Transform,
    /// Outline width in the current coordinate space.
    pub ow: f32,
    pub tint: [f64; 3],
}

impl<'a> Painter<'a> {
    pub fn color(&self, c: Rgb) -> Color {
        let f = |v: u8, k: f64| (v as f64 * k).clamp(0.0, 255.0) as u8;
        Color::from_rgba8(f(c.0, self.tint[0]), f(c.1, self.tint[1]), f(c.2, self.tint[2]), 255)
    }

    fn paint(&self, c: Rgb) -> Paint<'static> {
        let mut p = Paint::default();
        p.anti_alias = true;
        p.set_color(self.color(c));
        p
    }

    fn black(&self) -> Paint<'static> {
        let mut p = Paint::default();
        p.anti_alias = true;
        p.set_color(Color::from_rgba8(12, 10, 14, 255));
        p
    }

    pub fn fill(&mut self, path: &Path, c: Rgb) {
        let p = self.paint(c);
        self.pm.fill_path(path, &p, FillRule::Winding, self.tf, None);
    }

    pub fn fill_alpha(&mut self, path: &Path, c: Rgb, alpha: u8) {
        let mut p = Paint::default();
        p.anti_alias = true;
        let col = self.color(c);
        p.set_color_rgba8((col.red() * 255.0) as u8, (col.green() * 255.0) as u8, (col.blue() * 255.0) as u8, alpha);
        self.pm.fill_path(path, &p, FillRule::Winding, self.tf, None);
    }

    pub fn stroke(&mut self, path: &Path, c: Rgb, width: f32) {
        let p = self.paint(c);
        let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        self.pm.stroke_path(path, &p, &s, self.tf, None);
    }

    pub fn stroke_black(&mut self, path: &Path, width: f32) {
        let p = self.black();
        let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        self.pm.stroke_path(path, &p, &s, self.tf, None);
    }

    /// Fill plus black outline.
    pub fn shape(&mut self, path: &Path, c: Rgb) {
        self.fill(path, c);
        let ow = self.ow;
        if ow > 0.0 {
            self.stroke_black(path, ow);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        if let Some(p) = PathBuilder::from_circle(cx, cy, r) {
            self.shape(&p, c);
        }
    }

    pub fn ellipse_path(cx: f32, cy: f32, rx: f32, ry: f32) -> Option<Path> {
        let r = Rect::from_xywh(cx - rx, cy - ry, rx * 2.0, ry * 2.0)?;
        PathBuilder::from_oval(r)
    }

    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb) {
        if let Some(p) = Self::ellipse_path(cx, cy, rx, ry) {
            self.shape(&p, c);
        }
    }

    pub fn ellipse_nooutline(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, c: Rgb) {
        if let Some(p) = Self::ellipse_path(cx, cy, rx, ry) {
            self.fill(&p, c);
        }
    }

    pub fn capsule(&mut self, a: (f32, f32), b: (f32, f32), r: f32, c: Rgb) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            let ow = self.ow;
            if ow > 0.0 {
                self.stroke_black(&p, r * 2.0 + ow * 1.6);
            }
            self.stroke(&p, c, r * 2.0);
        }
    }

    pub fn line(&mut self, a: (f32, f32), b: (f32, f32), w: f32, c: Rgb) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            self.stroke(&p, c, w);
        }
    }

    pub fn line_black(&mut self, a: (f32, f32), b: (f32, f32), w: f32) {
        let mut pb = PathBuilder::new();
        pb.move_to(a.0, a.1);
        pb.line_to(b.0, b.1);
        if let Some(p) = pb.finish() {
            self.stroke_black(&p, w);
        }
    }

    pub fn poly_path(pts: &[(f32, f32)]) -> Option<Path> {
        let mut pb = PathBuilder::new();
        for (i, p) in pts.iter().enumerate() {
            if i == 0 {
                pb.move_to(p.0, p.1);
            } else {
                pb.line_to(p.0, p.1);
            }
        }
        pb.close();
        pb.finish()
    }

    /// Filled polygon without outline.
    pub fn fill_poly(&mut self, pts: &[(f32, f32)], c: Rgb) {
        if let Some(p) = Self::poly_path(pts) {
            self.fill(&p, c);
        }
    }

    /// Fill with a straight-alpha color (used for vector parts); the scene tint still applies.
    pub fn fill_rgba(&mut self, path: &Path, c: [u8; 4]) {
        let mut p = Paint::default();
        p.anti_alias = true;
        let col = self.color(Rgb(c[0], c[1], c[2]));
        p.set_color_rgba8((col.red() * 255.0).round() as u8, (col.green() * 255.0).round() as u8, (col.blue() * 255.0).round() as u8, c[3]);
        self.pm.fill_path(path, &p, FillRule::Winding, self.tf, None);
    }

    pub fn stroke_rgba(&mut self, path: &Path, c: [u8; 4], width: f32) {
        let mut p = Paint::default();
        p.anti_alias = true;
        let col = self.color(Rgb(c[0], c[1], c[2]));
        p.set_color_rgba8((col.red() * 255.0).round() as u8, (col.green() * 255.0).round() as u8, (col.blue() * 255.0).round() as u8, c[3]);
        let s = Stroke { width, line_cap: LineCap::Round, line_join: LineJoin::Round, ..Default::default() };
        self.pm.stroke_path(path, &p, &s, self.tf, None);
    }

    pub fn poly(&mut self, pts: &[(f32, f32)], c: Rgb) {
        if let Some(p) = Self::poly_path(pts) {
            self.shape(&p, c);
        }
    }

    /// Closed polygon with rounded corners (control points at the corners).
    pub fn smooth_path(pts: &[(f32, f32)]) -> Option<Path> {
        let n = pts.len();
        if n < 3 {
            return None;
        }
        let mid = |a: (f32, f32), b: (f32, f32)| ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
        let mut pb = PathBuilder::new();
        let start = mid(pts[n - 1], pts[0]);
        pb.move_to(start.0, start.1);
        for i in 0..n {
            let c = pts[i];
            let e = mid(pts[i], pts[(i + 1) % n]);
            pb.quad_to(c.0, c.1, e.0, e.1);
        }
        pb.close();
        pb.finish()
    }

    pub fn smooth(&mut self, pts: &[(f32, f32)], c: Rgb) {
        if let Some(p) = Self::smooth_path(pts) {
            self.shape(&p, c);
        }
    }

    /// Fills a world-space polygon (clipped against the near plane) in screen space. The painter
    /// must have an identity transform.
    pub fn poly3(&mut self, cam: &Cam, pts: &[V3], c: Rgb, outline: bool) {
        let cs: Vec<V3> = pts.iter().map(|p| cam.to_cam(*p)).collect();
        let clipped = clip_near(&cs, 0.08);
        if clipped.len() < 3 {
            return;
        }
        let sp: Vec<(f32, f32)> = clipped.iter().map(|p| cam.project_cam(*p)).collect();
        if let Some(path) = Self::poly_path(&sp) {
            self.fill(&path, c);
            if outline {
                let ow = self.ow;
                self.stroke_black(&path, ow);
            }
        }
    }

    pub fn poly3_alpha(&mut self, cam: &Cam, pts: &[V3], c: Rgb, alpha: u8) {
        let cs: Vec<V3> = pts.iter().map(|p| cam.to_cam(*p)).collect();
        let clipped = clip_near(&cs, 0.08);
        if clipped.len() < 3 {
            return;
        }
        let sp: Vec<(f32, f32)> = clipped.iter().map(|p| cam.project_cam(*p)).collect();
        if let Some(path) = Self::poly_path(&sp) {
            self.fill_alpha(&path, c, alpha);
        }
    }

    pub fn line3(&mut self, cam: &Cam, a: V3, b: V3, w: f32, c: Rgb) {
        let cs = [cam.to_cam(a), cam.to_cam(b)];
        let (mut p, mut q) = (cs[0], cs[1]);
        let near = 0.08;
        if p[2] < near && q[2] < near {
            return;
        }
        if p[2] < near {
            let t = (near - p[2]) / (q[2] - p[2]);
            p = [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t, near];
        }
        if q[2] < near {
            let t = (near - q[2]) / (p[2] - q[2]);
            q = [q[0] + (p[0] - q[0]) * t, q[1] + (p[1] - q[1]) * t, near];
        }
        self.line(cam.project_cam(p), cam.project_cam(q), w, c);
    }
}
