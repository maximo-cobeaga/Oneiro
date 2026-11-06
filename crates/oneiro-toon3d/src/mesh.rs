//! Procedural triangle meshes for the 3D toon style: superellipsoids (heads, torsos, shoes), capsules
//! (limbs, tail), caps and cylinders (hats, hair) and thin cloth sheets (the cape). Everything is
//! built directly in world space with smooth vertex normals and outward-facing triangles.

use std::f32::consts::{PI, TAU};

pub type V3 = [f32; 3];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn mul(a: V3, k: f32) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
pub fn len(a: V3) -> f32 {
    dot(a, a).sqrt()
}
pub fn norm(a: V3) -> V3 {
    let l = len(a);
    if l < 1e-9 {
        [0.0, 1.0, 0.0]
    } else {
        mul(a, 1.0 / l)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Vtx {
    pub p: V3,
    pub n: V3,
    pub c: V3,
}

#[derive(Clone, Debug, Default)]
pub struct Mesh {
    pub v: Vec<Vtx>,
    pub i: Vec<u32>,
}

/// A scaled frame: `world = o + x * ax + y * ay + z * az` (the axes carry the scale).
#[derive(Clone, Copy, Debug)]
pub struct Xf {
    pub o: V3,
    pub ax: V3,
    pub ay: V3,
    pub az: V3,
}

impl Xf {
    pub fn new(o: V3, ax: V3, ay: V3, az: V3) -> Xf {
        Xf { o, ax, ay, az }
    }
    /// Axis-aligned frame scaled by `s` (x, y, z).
    pub fn scaled(o: V3, s: V3) -> Xf {
        Xf { o, ax: [s[0], 0.0, 0.0], ay: [0.0, s[1], 0.0], az: [0.0, 0.0, s[2]] }
    }
    /// Unit axes `ax, ay, az` scaled by `s`.
    pub fn basis(o: V3, ax: V3, ay: V3, az: V3, s: V3) -> Xf {
        Xf { o, ax: mul(norm(ax), s[0]), ay: mul(norm(ay), s[1]), az: mul(norm(az), s[2]) }
    }
    pub fn apply(&self, p: V3) -> V3 {
        add(self.o, add(add(mul(self.ax, p[0]), mul(self.ay, p[1])), mul(self.az, p[2])))
    }
}

fn spow(x: f32, e: f32) -> f32 {
    x.signum() * x.abs().powf(e)
}

impl Mesh {
    pub fn is_empty(&self) -> bool {
        self.i.is_empty()
    }

    pub fn triangles(&self) -> usize {
        self.i.len() / 3
    }

    /// Adds a parametric surface. `rows` is a list of rows, each a ring of points (`closed` rings wrap
    /// around). A row with a single point is a pole. Normals are smooth (area-weighted), shared across the
    /// ring seam and the poles. If `center` is given, triangles are oriented to face away from it.
    pub fn ring_surface(&mut self, rows: &[Vec<V3>], closed: bool, color: V3, center: Option<V3>) {
        if rows.len() < 2 {
            return;
        }
        let base = self.v.len() as u32;
        let mut ids: Vec<Vec<u32>> = Vec::new();
        let mut positions: Vec<V3> = Vec::new();
        for row in rows {
            let mut r = Vec::new();
            for p in row {
                r.push(base + positions.len() as u32);
                positions.push(*p);
            }
            ids.push(r);
        }
        let mut tris: Vec<[u32; 3]> = Vec::new();
        for j in 0..rows.len() - 1 {
            let (a, b) = (&ids[j], &ids[j + 1]);
            let (na, nb) = (a.len(), b.len());
            let n = na.max(nb);
            let segs = if closed { n } else { n - 1 };
            for i in 0..segs {
                let pa = |k: usize| if na == 1 { a[0] } else { a[(i + k) % na] };
                let pb = |k: usize| if nb == 1 { b[0] } else { b[(i + k) % nb] };
                let (a0, a1, b0, b1) = (pa(0), pa(1), pb(0), pb(1));
                if a0 != a1 && a0 != b0 && a1 != b0 {
                    tris.push([a0, a1, b0]);
                }
                if a1 != b1 && a1 != b0 && b0 != b1 {
                    tris.push([a1, b1, b0]);
                }
            }
        }
        // Normals.
        let mut normals = vec![[0.0f32; 3]; positions.len()];
        let local = |g: u32| (g - base) as usize;
        for t in &tris {
            let (p0, p1, p2) = (positions[local(t[0])], positions[local(t[1])], positions[local(t[2])]);
            let n = cross(sub(p1, p0), sub(p2, p0));
            for g in t {
                let k = local(*g);
                normals[k] = add(normals[k], n);
            }
        }
        let mut flip = false;
        if let Some(c) = center {
            let s: f32 = (0..positions.len()).map(|k| dot(normals[k], sub(positions[k], c))).sum();
            flip = s < 0.0;
        }
        for (k, p) in positions.iter().enumerate() {
            let n = norm(normals[k]);
            self.v.push(Vtx { p: *p, n: if flip { mul(n, -1.0) } else { n }, c: color });
        }
        for t in tris {
            if flip {
                self.i.extend_from_slice(&[t[0], t[2], t[1]]);
            } else {
                self.i.extend_from_slice(&t);
            }
        }
    }

    /// Superellipsoid scaled and placed by `xf` (`e1`/`e2` = 1 is an ellipsoid, small values round boxes).
    pub fn ellipsoid(&mut self, xf: &Xf, e1: f32, e2: f32, color: V3) {
        self.ellipsoid_part(xf, e1, e2, -90.0, 90.0, color, true);
    }

    /// A latitude band of an ellipsoid (degrees, -90 = bottom pole, 90 = top pole): hats, hair.
    #[allow(clippy::too_many_arguments)]
    pub fn ellipsoid_part(&mut self, xf: &Xf, e1: f32, e2: f32, lat0: f32, lat1: f32, color: V3, orient: bool) {
        let (nu, nv) = (22usize, 14usize);
        let mut rows = Vec::new();
        for j in 0..=nv {
            let phi = (lat0 + (lat1 - lat0) * j as f32 / nv as f32).to_radians();
            let (c, s) = (spow(phi.cos(), e1), spow(phi.sin(), e1));
            if (phi.cos().abs() < 1e-4) && orient_pole(lat0, lat1, j, nv) {
                rows.push(vec![xf.apply([0.0, s, 0.0])]);
                continue;
            }
            let ring: Vec<V3> = (0..nu)
                .map(|i| {
                    let th = TAU * i as f32 / nu as f32;
                    xf.apply([c * spow(th.cos(), e2), s, c * spow(th.sin(), e2)])
                })
                .collect();
            rows.push(ring);
        }
        let center = if orient { Some(xf.o) } else { Some(add(xf.o, mul(xf.ay, -2.0))) };
        self.ring_surface(&rows, true, color, center);
    }

    pub fn sphere(&mut self, c: V3, r: f32, color: V3) {
        self.ellipsoid(&Xf::scaled(c, [r, r, r]), 1.0, 1.0, color);
    }

    /// Surface of revolution around the axis `a -> b`: `profile` is (distance along the axis from `a`, radius).
    /// First and last points should have radius 0 (poles).
    pub fn revolve(&mut self, a: V3, b: V3, profile: &[(f32, f32)], color: V3) {
        let axis = norm(sub(b, a));
        let helper = if axis[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let u = norm(cross(axis, helper));
        let w = cross(axis, u);
        let nu = 14usize;
        let mut rows = Vec::new();
        for (t, r) in profile {
            let c = add(a, mul(axis, *t));
            if *r < 1e-6 {
                rows.push(vec![c]);
            } else {
                rows.push((0..nu).map(|i| {
                    let th = TAU * i as f32 / nu as f32;
                    add(c, add(mul(u, r * th.cos()), mul(w, r * th.sin())))
                }).collect());
            }
        }
        let mid = add(a, mul(axis, profile.iter().map(|p| p.0).sum::<f32>() / profile.len().max(1) as f32));
        self.ring_surface(&rows, true, color, Some(mid));
    }

    /// A capsule from `a` to `b` with end radii `ra`, `rb`.
    pub fn capsule(&mut self, a: V3, b: V3, ra: f32, rb: f32, color: V3) {
        let l = len(sub(b, a));
        let mut prof: Vec<(f32, f32)> = vec![(-ra, 0.0)];
        for k in 1..=5 {
            let ang = k as f32 / 5.0 * PI / 2.0;
            prof.push((-ra * ang.cos(), ra * ang.sin()));
        }
        prof.push((l, rb));
        for k in 1..5 {
            let ang = k as f32 / 5.0 * PI / 2.0;
            prof.push((l + rb * ang.sin(), rb * ang.cos()));
        }
        prof.push((l + rb, 0.0));
        self.revolve(a, b, &prof, color);
    }

    /// A short cylinder with rounded edge, standing on `base` and going up `h` (hats, rims).
    pub fn cylinder(&mut self, base: V3, h: f32, r: f32, color: V3) {
        let top = add(base, [0.0, h, 0.0]);
        let prof = vec![(0.0, 0.0), (0.0, r * 0.92), (h * 0.04, r), (h * 0.96, r), (h, r * 0.92), (h, 0.0)];
        self.revolve(base, top, &prof, color);
    }

    /// A thin double-sided sheet through a `rows x cols` grid of points (cloth). `thickness` pushes the two
    /// faces apart so that the outline pass has a hull to grow.
    pub fn sheet(&mut self, pts: &[V3], rows: usize, cols: usize, thickness: f32, color: V3) {
        if pts.len() != rows * cols || rows < 2 || cols < 2 {
            return;
        }
        let at = |r: usize, c: usize| pts[r * cols + c];
        // Per-vertex normal of the sheet.
        let mut nrm = vec![[0.0f32; 3]; pts.len()];
        for r in 0..rows - 1 {
            for c in 0..cols - 1 {
                let n = cross(sub(at(r, c + 1), at(r, c)), sub(at(r + 1, c), at(r, c)));
                for (rr, cc) in [(r, c), (r, c + 1), (r + 1, c), (r + 1, c + 1)] {
                    nrm[rr * cols + cc] = add(nrm[rr * cols + cc], n);
                }
            }
        }
        for side in [1.0f32, -1.0] {
            let rows_pts: Vec<Vec<V3>> = (0..rows).map(|r| (0..cols).map(|c| {
                let n = norm(nrm[r * cols + c]);
                add(at(r, c), mul(n, side * thickness * 0.5))
            }).collect()).collect();
            let base = self.v.len() as u32;
            for r in 0..rows {
                for c in 0..cols {
                    let n = mul(norm(nrm[r * cols + c]), side);
                    self.v.push(Vtx { p: rows_pts[r][c], n, c: color });
                }
            }
            for r in 0..rows - 1 {
                for c in 0..cols - 1 {
                    let i = |rr: usize, cc: usize| base + (rr * cols + cc) as u32;
                    let (a, b, d, e) = (i(r, c), i(r, c + 1), i(r + 1, c), i(r + 1, c + 1));
                    if side > 0.0 {
                        self.i.extend_from_slice(&[a, b, d, b, e, d]);
                    } else {
                        self.i.extend_from_slice(&[a, d, b, b, d, e]);
                    }
                }
            }
        }
    }
}

fn orient_pole(lat0: f32, lat1: f32, j: usize, nv: usize) -> bool {
    (j == 0 && lat0.abs() >= 89.9) || (j == nv && lat1.abs() >= 89.9)
}
