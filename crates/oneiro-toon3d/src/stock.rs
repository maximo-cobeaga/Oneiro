//! Stock 3D props (trees, houses, furniture, a car...) and `solids` shapes, built from simple primitives so that
//! any camera side sees real geometry with the toon look (cel shading and ink outlines).

use crate::mesh::*;
use oneiro_core::defs::PropInst;
use oneiro_core::Rgb;

/// Stock props: (kind, default width, height, depth, people walk around it).
pub const STOCK: &[(&str, f64, f64, f64, bool)] = &[
    ("tree", 2.4, 3.6, 2.4, true),
    ("pine", 2.0, 4.0, 2.0, true),
    ("bush", 1.2, 0.8, 1.2, false),
    ("rock", 1.2, 0.7, 0.9, true),
    ("house", 5.0, 3.6, 4.0, true),
    ("fence", 3.0, 1.0, 0.12, false),
    ("lamp_post", 0.4, 3.2, 0.4, true),
    ("bench", 1.6, 0.9, 0.55, true),
    ("sofa", 2.0, 0.9, 0.9, true),
    ("bed", 1.5, 0.6, 2.0, true),
    ("bookshelf", 1.2, 2.0, 0.35, true),
    ("tv", 1.1, 0.9, 0.4, false),
    ("car", 1.8, 1.4, 4.2, true),
    ("rug", 2.0, 0.02, 1.4, false),
];

pub fn stock(kind: &str) -> Option<&'static (&'static str, f64, f64, f64, bool)> {
    STOCK.iter().find(|s| s.0 == kind)
}

fn col(c: Rgb) -> V3 {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}

fn rgbf(r: u8, g: u8, b: u8) -> V3 {
    [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0]
}

fn shade_of(c: V3, k: f32) -> V3 {
    [(c[0] * k).min(1.0), (c[1] * k).min(1.0), (c[2] * k).min(1.0)]
}

/// Builds shapes in a local frame (x right, y up, z forward; one unit = the prop's default size) that is scaled to
/// the requested size, turned by the yaw and placed at its position.
struct Local<'a> {
    m: &'a mut Mesh,
    at: [f32; 2],
    y0: f32,
    yaw: f32,
    s: [f32; 3],
}

impl Local<'_> {
    fn xz(&self, lx: f32, lz: f32) -> [f32; 2] {
        let (sx, sz) = (lx * self.s[0], lz * self.s[2]);
        let (c, s) = (self.yaw.cos(), self.yaw.sin());
        [self.at[0] + c * sx + s * sz, self.at[1] - s * sx + c * sz]
    }
    #[allow(clippy::too_many_arguments)]
    fn bx(&mut self, lx: f32, ly: f32, lz: f32, w: f32, h: f32, d: f32, color: V3) {
        let c = self.xz(lx, lz);
        self.m.cuboid(c, self.y0 + ly * self.s[1], [w * self.s[0], h * self.s[1], d * self.s[2]], self.yaw, color);
    }
    fn cyl(&mut self, lx: f32, ly: f32, lz: f32, h: f32, r: f32, color: V3) {
        let c = self.xz(lx, lz);
        let rr = r * (self.s[0] + self.s[2]) / 2.0;
        self.m.cylinder([c[0], self.y0 + ly * self.s[1], c[1]], h * self.s[1], rr, color);
    }
    fn cone(&mut self, lx: f32, ly: f32, lz: f32, h: f32, r: f32, color: V3) {
        let c = self.xz(lx, lz);
        let (hh, rr) = (h * self.s[1], r * (self.s[0] + self.s[2]) / 2.0);
        let a = [c[0], self.y0 + ly * self.s[1], c[1]];
        self.m.revolve(a, [a[0], a[1] + hh, a[2]], &[(0.0, 0.0), (0.0, rr), (hh, 0.0)], color);
    }
    #[allow(clippy::too_many_arguments)]
    fn sph(&mut self, lx: f32, ly: f32, lz: f32, rx: f32, ry: f32, rz: f32, color: V3) {
        let c = self.xz(lx, lz);
        self.m.ellipsoid(&Xf::scaled([c[0], self.y0 + ly * self.s[1], c[1]], [rx * self.s[0], ry * self.s[1], rz * self.s[2]]), 1.0, 1.0, color);
    }
    /// A wheel: a flat cylinder lying along the local x axis.
    fn wheel(&mut self, lx: f32, ly: f32, lz: f32, r: f32, thick: f32, color: V3) {
        let c = self.xz(lx, lz);
        let axis = [self.yaw.cos(), 0.0, -self.yaw.sin()];
        let (t, rr) = (thick * self.s[0], r * self.s[1]);
        let mid = [c[0], self.y0 + ly * self.s[1], c[1]];
        let a = sub(mid, mul(axis, t / 2.0));
        self.m.revolve(a, add(a, mul(axis, t)), &[(0.0, 0.0), (0.0, rr), (t, rr), (t, 0.0)], color);
    }
}

/// The 3D mesh of a stock prop.
pub fn stock_mesh(pr: &PropInst) -> Option<Mesh> {
    let &(kind, dw, dh, dd, _) = stock(&pr.kind)?;
    let mut m = Mesh::default();
    let s = [(pr.w.unwrap_or(dw) / dw) as f32, (pr.h.unwrap_or(dh) / dh) as f32, (pr.d.unwrap_or(dd) / dd) as f32];
    let user = pr.color.map(col);
    let mut l = Local { m: &mut m, at: [pr.at[0] as f32, pr.at[1] as f32], y0: pr.y as f32, yaw: pr.yaw as f32, s };
    let (trunk, leaf2) = (rgbf(122, 82, 48), rgbf(44, 140, 74));
    let metal = rgbf(170, 174, 184);
    match kind {
        "tree" => {
            let leaf = user.unwrap_or(rgbf(58, 168, 88));
            l.cyl(0.0, 0.0, 0.0, 1.6, 0.14, trunk);
            l.sph(0.0, 2.3, 0.0, 1.1, 1.0, 1.1, leaf);
            l.sph(0.45, 1.9, 0.3, 0.7, 0.6, 0.7, leaf2);
            l.sph(-0.4, 2.0, -0.3, 0.7, 0.65, 0.7, leaf);
        }
        "pine" => {
            l.cyl(0.0, 0.0, 0.0, 0.8, 0.12, trunk);
            let c = user.unwrap_or(rgbf(40, 128, 84));
            l.cone(0.0, 0.7, 0.0, 1.6, 0.95, c);
            l.cone(0.0, 1.7, 0.0, 1.4, 0.75, shade_of(c, 1.12));
            l.cone(0.0, 2.6, 0.0, 1.3, 0.5, shade_of(c, 1.25));
        }
        "bush" => {
            let c = user.unwrap_or(rgbf(70, 176, 96));
            l.sph(0.0, 0.4, 0.0, 0.55, 0.4, 0.5, c);
            l.sph(0.4, 0.32, 0.1, 0.38, 0.3, 0.38, shade_of(c, 0.9));
            l.sph(-0.38, 0.3, -0.05, 0.36, 0.3, 0.36, shade_of(c, 1.08));
        }
        "rock" => {
            let c = user.unwrap_or(rgbf(150, 152, 160));
            l.sph(0.0, 0.32, 0.0, 0.58, 0.34, 0.44, c);
            l.sph(0.32, 0.18, 0.12, 0.3, 0.2, 0.26, shade_of(c, 0.88));
        }
        "house" => {
            let wall = user.unwrap_or(rgbf(244, 226, 190));
            l.bx(0.0, 0.0, 0.0, 5.0, 2.5, 4.0, wall);
            let roof_at = l.xz(0.0, 0.0);
            let (ry, rs, yaw) = (l.y0 + 2.5 * l.s[1], [5.5 * l.s[0], 1.3 * l.s[1], 4.5 * l.s[2]], l.yaw);
            l.m.gable_roof(roof_at, ry, rs, yaw, rgbf(196, 64, 56));
            l.bx(0.0, 0.0, 2.0, 0.9, 1.9, 0.12, rgbf(120, 76, 44));
            for x in [-1.6f32, 1.6] {
                l.bx(x, 1.2, 2.0, 0.9, 0.9, 0.1, rgbf(150, 214, 240));
            }
            l.bx(1.7, 2.5, -0.9, 0.6, 1.4, 0.6, rgbf(190, 100, 80));
        }
        "fence" => {
            let wood = user.unwrap_or(rgbf(214, 176, 120));
            for k in 0..=6 {
                l.bx(-1.5 + k as f32 * 0.5, 0.0, 0.0, 0.1, 1.0, 0.1, wood);
            }
            l.bx(0.0, 0.25, 0.0, 3.0, 0.1, 0.06, shade_of(wood, 0.9));
            l.bx(0.0, 0.65, 0.0, 3.0, 0.1, 0.06, shade_of(wood, 0.9));
        }
        "lamp_post" => {
            l.cyl(0.0, 0.0, 0.0, 0.2, 0.18, metal);
            l.cyl(0.0, 0.0, 0.0, 2.9, 0.05, rgbf(70, 74, 86));
            l.sph(0.0, 2.95, 0.0, 0.22, 0.22, 0.22, user.unwrap_or(rgbf(255, 226, 120)));
        }
        "bench" => {
            let wood = user.unwrap_or(rgbf(170, 112, 66));
            l.bx(0.0, 0.42, 0.0, 1.6, 0.08, 0.5, wood);
            l.bx(0.0, 0.62, -0.22, 1.6, 0.4, 0.07, wood);
            for x in [-0.7f32, 0.7] {
                l.bx(x, 0.0, 0.0, 0.08, 0.42, 0.45, metal);
            }
        }
        "sofa" => {
            let c = user.unwrap_or(rgbf(86, 120, 196));
            l.bx(0.0, 0.0, 0.0, 2.0, 0.4, 0.9, shade_of(c, 0.85));
            l.bx(0.0, 0.4, 0.0, 1.5, 0.18, 0.75, c);
            l.bx(0.0, 0.4, -0.38, 1.9, 0.5, 0.16, shade_of(c, 0.92));
            for x in [-0.9f32, 0.9] {
                l.bx(x, 0.4, 0.0, 0.2, 0.32, 0.9, shade_of(c, 0.8));
            }
        }
        "bed" => {
            let c = user.unwrap_or(rgbf(220, 120, 140));
            l.bx(0.0, 0.0, 0.0, 1.5, 0.3, 2.0, rgbf(150, 100, 64));
            l.bx(0.0, 0.3, 0.1, 1.4, 0.2, 1.8, c);
            l.bx(0.0, 0.3, -0.75, 1.0, 0.12, 0.35, rgbf(250, 246, 240));
            l.bx(0.0, 0.0, -1.0, 1.5, 0.6, 0.08, rgbf(120, 80, 50));
        }
        "bookshelf" => {
            let wood = user.unwrap_or(rgbf(150, 100, 60));
            l.bx(0.0, 0.0, 0.0, 1.2, 2.0, 0.35, wood);
            let cols = [rgbf(214, 72, 72), rgbf(80, 140, 214), rgbf(240, 200, 80), rgbf(90, 180, 110)];
            for (row, y) in [0.12f32, 0.7, 1.28].iter().enumerate() {
                l.bx(0.0, *y + 0.5, 0.0, 1.14, 0.04, 0.34, shade_of(wood, 0.8));
                for k in 0..5 {
                    l.bx(-0.45 + k as f32 * 0.2, *y, 0.02, 0.14, 0.4 + 0.05 * ((k + row) % 3) as f32, 0.24, cols[(k + row) % 4]);
                }
            }
        }
        "tv" => {
            l.bx(0.0, 0.0, 0.0, 1.1, 0.4, 0.4, rgbf(120, 84, 56));
            l.bx(0.0, 0.4, 0.0, 1.0, 0.55, 0.08, rgbf(30, 32, 40));
            l.bx(0.0, 0.45, 0.045, 0.9, 0.45, 0.02, user.unwrap_or(rgbf(90, 160, 230)));
        }
        "car" => {
            let c = user.unwrap_or(rgbf(228, 70, 70));
            l.bx(0.0, 0.3, 0.0, 1.8, 0.6, 4.2, c);
            l.bx(0.0, 0.9, -0.2, 1.6, 0.55, 2.2, shade_of(c, 0.92));
            l.bx(0.0, 0.95, -0.2, 1.64, 0.4, 1.9, rgbf(150, 210, 240));
            for (x, z) in [(-0.85f32, 1.3f32), (0.85, 1.3), (-0.85, -1.3), (0.85, -1.3)] {
                l.wheel(x, 0.32, z, 0.34, 0.2, rgbf(40, 40, 46));
            }
        }
        "rug" => {
            let c = user.unwrap_or(rgbf(190, 70, 90));
            l.bx(0.0, 0.0, 0.0, 2.0, 0.02, 1.4, c);
            l.bx(0.0, 0.02, 0.0, 1.5, 0.01, 0.9, shade_of(c, 1.25));
        }
        _ => return None,
    }
    Some(m)
}

/// The 3D mesh of a `solids` entry.
pub fn solid_mesh(sd: &oneiro_core::scenery::Solid) -> Mesh {
    let mut m = Mesh::default();
    let c = [sd.at[0] as f32, sd.at[1] as f32];
    let (y0, yaw) = (sd.y as f32, sd.yaw as f32);
    let [w, h, d] = [sd.size[0] as f32, sd.size[1] as f32, sd.size[2] as f32];
    let color = col(sd.color);
    match sd.shape.as_str() {
        "box" => m.cuboid(c, y0, [w, h, d], yaw, color),
        "cylinder" => m.cylinder([c[0], y0, c[1]], h, w / 2.0, color),
        "cone" => m.revolve([c[0], y0, c[1]], [c[0], y0 + h, c[1]], &[(0.0, 0.0), (0.0, w / 2.0), (h, 0.0)], color),
        "sphere" => m.ellipsoid(&Xf::scaled([c[0], y0 + h / 2.0, c[1]], [w / 2.0, h / 2.0, d / 2.0]), 1.0, 1.0, color),
        "roof" => m.gable_roof(c, y0, [w, h, d], yaw, color),
        "pyramid" => m.pyramid(c, y0, [w, h, d], yaw, color),
        _ => {}
    }
    m
}
