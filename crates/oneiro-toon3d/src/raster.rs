//! The CPU backend of the 3D toon renderer: a small software rasterizer (z-buffer, perspective-correct
//! interpolation, cel-shaded lighting, inverted-hull ink outlines, 2x2 supersampling). It needs no GPU
//! and gives the same pixels on every machine; the GPU backend produces the same look faster.

use crate::mesh::*;
use tiny_skia::Pixmap;

/// A pinhole camera in world space; `cx`, `cy` and `f` are in pixels of the target.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub pos: [f64; 3],
    pub right: [f64; 3],
    pub up: [f64; 3],
    pub fwd: [f64; 3],
    pub f: f64,
    pub cx: f64,
    pub cy: f64,
}

impl Camera {
    pub fn to_cam(&self, p: V3) -> [f32; 3] {
        let v = [p[0] as f64 - self.pos[0], p[1] as f64 - self.pos[1], p[2] as f64 - self.pos[2]];
        let d = |a: [f64; 3]| (v[0] * a[0] + v[1] * a[1] + v[2] * a[2]) as f32;
        [d(self.right), d(self.up), d(self.fwd)]
    }
    pub fn pos_f32(&self) -> V3 {
        [self.pos[0] as f32, self.pos[1] as f32, self.pos[2] as f32]
    }
    /// The light comes from the upper left, toward the viewer, whatever the camera does.
    pub fn light(&self) -> V3 {
        let f = |a: [f64; 3]| [a[0] as f32, a[1] as f32, a[2] as f32];
        norm(add(add(mul(f(self.right), -0.4), mul(f(self.up), 0.7)), mul(f(self.fwd), -0.6)))
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Scene tint (night is bluish).
    pub tint: [f32; 3],
    /// Ink outline width in pixels of the final image.
    pub outline_px: f32,
}

pub const INK: V3 = [0.07, 0.06, 0.08];

fn smoothstep(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Cel shading: three flat bands with soft edges, a toon highlight and a rim. Shared with the GPU shader
/// (the WGSL version mirrors this function).
pub fn shade(base: V3, n: V3, view: V3, light: V3, tint: [f32; 3]) -> V3 {
    let ndl = dot(n, light);
    let band = 0.58 + 0.22 * smoothstep(-0.12, -0.02, ndl) + 0.20 * smoothstep(0.22, 0.30, ndl);
    let h = norm(add(light, view));
    let spec = smoothstep(0.965, 0.975, dot(n, h)) * 0.22;
    let rim = smoothstep(0.62, 0.70, 1.0 - dot(n, view).max(0.0)) * 0.10 * smoothstep(0.0, 0.3, ndl);
    [
        (base[0] * band * tint[0] + spec + rim).clamp(0.0, 1.0),
        (base[1] * band * tint[1] + spec + rim).clamp(0.0, 1.0),
        (base[2] * band * tint[2] + spec + rim).clamp(0.0, 1.0),
    ]
}

struct Target {
    w: usize,
    h: usize,
    color: Vec<[u8; 4]>,
    depth: Vec<f32>,
}

#[derive(Clone, Copy)]
struct SVert {
    x: f32,
    y: f32,
    z: f32,
    p: V3,
    n: V3,
    c: V3,
}

/// Rasterizes one triangle; `fragment` returns the color for a pixel (None to skip it).
fn tri(t: &mut Target, a: SVert, b: SVert, c: SVert, depth_bias: f32, mut fragment: impl FnMut(f32, f32, f32, [f32; 3]) -> Option<V3>) {
    let area = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
    if area.abs() < 1e-8 {
        return;
    }
    let (minx, maxx) = (a.x.min(b.x).min(c.x).floor().max(0.0) as usize, (a.x.max(b.x).max(c.x).ceil() as i64).min(t.w as i64 - 1).max(-1));
    let (miny, maxy) = (a.y.min(b.y).min(c.y).floor().max(0.0) as usize, (a.y.max(b.y).max(c.y).ceil() as i64).min(t.h as i64 - 1).max(-1));
    if maxx < 0 || maxy < 0 {
        return;
    }
    let inv = 1.0 / area;
    let (iz0, iz1, iz2) = (1.0 / a.z, 1.0 / b.z, 1.0 / c.z);
    for y in miny..=maxy as usize {
        for x in minx..=maxx as usize {
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            let w0 = ((b.x - px) * (c.y - py) - (b.y - py) * (c.x - px)) * inv;
            let w1 = ((c.x - px) * (a.y - py) - (c.y - py) * (a.x - px)) * inv;
            let w2 = 1.0 - w0 - w1;
            if w0 < -1e-4 || w1 < -1e-4 || w2 < -1e-4 {
                continue;
            }
            let denom = w0 * iz0 + w1 * iz1 + w2 * iz2;
            let z = 1.0 / denom;
            let idx = y * t.w + x;
            if z - depth_bias >= t.depth[idx] {
                continue;
            }
            // Perspective-correct barycentrics.
            let q = [w0 * iz0 * z, w1 * iz1 * z, w2 * iz2 * z];
            if let Some(col) = fragment(px, py, z, q) {
                t.depth[idx] = z;
                t.color[idx] = [(col[0] * 255.0 + 0.5) as u8, (col[1] * 255.0 + 0.5) as u8, (col[2] * 255.0 + 0.5) as u8, 255];
            }
        }
    }
}

/// Renders `mesh` into a transparent RGBA (premultiplied) image of `w x h` pixels.
pub fn render(mesh: &Mesh, cam: &Camera, w: u32, h: u32, style: &Style) -> Pixmap {
    let ss = 2usize;
    let (sw, sh) = (w as usize * ss, h as usize * ss);
    let mut t = Target { w: sw, h: sh, color: vec![[0, 0, 0, 0]; sw * sh], depth: vec![f32::INFINITY; sw * sh] };
    let cam_pos = cam.pos_f32();
    let light = cam.light();
    let near = 0.05f32;
    let (f, cx, cy) = ((cam.f * ss as f64) as f32, (cam.cx * ss as f64) as f32, (cam.cy * ss as f64) as f32);
    let project = |p: V3, n: V3, c: V3| -> SVert {
        let q = cam.to_cam(p);
        SVert { x: cx + f * q[0] / q[2].max(1e-6), y: cy - f * q[1] / q[2].max(1e-6), z: q[2], p, n, c }
    };

    // Pass 1: the ink outline, back faces pushed out along their normals by a constant width in pixels.
    if style.outline_px > 0.0 {
        let grow = |v: &Vtx| -> V3 {
            let z = cam.to_cam(v.p)[2].max(near);
            add(v.p, mul(v.n, style.outline_px * z / (f / ss as f32)))
        };
        for tr in mesh.i.chunks_exact(3) {
            let vs = [&mesh.v[tr[0] as usize], &mesh.v[tr[1] as usize], &mesh.v[tr[2] as usize]];
            let fnrm = cross(sub(vs[1].p, vs[0].p), sub(vs[2].p, vs[0].p));
            if dot(fnrm, sub(cam_pos, vs[0].p)) >= 0.0 {
                continue; // front face
            }
            let s: Vec<SVert> = vs.iter().map(|v| project(grow(v), v.n, INK)).collect();
            if s.iter().any(|v| v.z < near) {
                continue;
            }
            tri(&mut t, s[0], s[1], s[2], 0.0, |_, _, _, _| Some(INK));
        }
    }

    // Pass 2: front faces, cel shaded.
    for tr in mesh.i.chunks_exact(3) {
        let vs = [&mesh.v[tr[0] as usize], &mesh.v[tr[1] as usize], &mesh.v[tr[2] as usize]];
        let fnrm = cross(sub(vs[1].p, vs[0].p), sub(vs[2].p, vs[0].p));
        if dot(fnrm, sub(cam_pos, vs[0].p)) <= 0.0 {
            continue;
        }
        let s: Vec<SVert> = vs.iter().map(|v| project(v.p, v.n, v.c)).collect();
        if s.iter().any(|v| v.z < near) {
            continue;
        }
        let (a, b, c) = (s[0], s[1], s[2]);
        tri(&mut t, a, b, c, 1e-5, |_, _, _, q| {
            let n = norm([a.n[0] * q[0] + b.n[0] * q[1] + c.n[0] * q[2], a.n[1] * q[0] + b.n[1] * q[1] + c.n[1] * q[2], a.n[2] * q[0] + b.n[2] * q[1] + c.n[2] * q[2]]);
            let col = [a.c[0] * q[0] + b.c[0] * q[1] + c.c[0] * q[2], a.c[1] * q[0] + b.c[1] * q[1] + c.c[1] * q[2], a.c[2] * q[0] + b.c[2] * q[1] + c.c[2] * q[2]];
            let p = [a.p[0] * q[0] + b.p[0] * q[1] + c.p[0] * q[2], a.p[1] * q[0] + b.p[1] * q[1] + c.p[1] * q[2], a.p[2] * q[0] + b.p[2] * q[1] + c.p[2] * q[2]];
            let view = norm(sub(cam_pos, p));
            Some(shade(col, n, view, light, style.tint))
        });
    }

    // Downsample (premultiplied average).
    let mut out = Pixmap::new(w, h).expect("layer");
    let data = out.data_mut();
    for y in 0..h as usize {
        for x in 0..w as usize {
            let mut acc = [0u32; 4];
            for dy in 0..ss {
                for dx in 0..ss {
                    let c = t.color[(y * ss + dy) * sw + x * ss + dx];
                    // Stored as straight color with alpha 255 where covered, 0 elsewhere.
                    for k in 0..4 {
                        acc[k] += c[k] as u32;
                    }
                }
            }
            let n = (ss * ss) as u32;
            let o = (y * w as usize + x) * 4;
            let a = acc[3] / n;
            // acc[0..3] already sums covered colors only (uncovered are 0), i.e. premultiplied by coverage.
            data[o] = (acc[0] / n) as u8;
            data[o + 1] = (acc[1] / n) as u8;
            data[o + 2] = (acc[2] / n) as u8;
            data[o + 3] = a as u8;
        }
    }
    out
}
