use std::f64::consts::PI;

pub type V2 = [f64; 2];
pub type V3 = [f64; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub fn from_hex(h: &str) -> Option<Rgb> {
        let h = h.trim_start_matches('#');
        let p = |s: &str| u8::from_str_radix(s, 16).ok();
        match h.len() {
            6 => Some(Rgb(p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?)),
            3 => {
                let d = |i: usize| p(&format!("{0}{0}", &h[i..i + 1]));
                Some(Rgb(d(0)?, d(1)?, d(2)?))
            }
            _ => None,
        }
    }
    pub fn lerp(self, o: Rgb, t: f64) -> Rgb {
        let f = |a: u8, b: u8| (a as f64 + (b as f64 - a as f64) * t).round().clamp(0.0, 255.0) as u8;
        Rgb(f(self.0, o.0), f(self.1, o.1), f(self.2, o.2))
    }
    pub fn scale(self, k: f64) -> Rgb {
        let f = |a: u8| (a as f64 * k).round().clamp(0.0, 255.0) as u8;
        Rgb(f(self.0), f(self.1), f(self.2))
    }
    pub fn hex(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.0, self.1, self.2)
    }
}

pub fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

pub fn smooth(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Shortest signed angle from `b` to `a`.
pub fn angle_diff(a: f64, b: f64) -> f64 {
    let mut d = (a - b) % (2.0 * PI);
    if d > PI {
        d -= 2.0 * PI;
    }
    if d < -PI {
        d += 2.0 * PI;
    }
    d
}

pub fn dist2(a: V2, b: V2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Yaw convention: 0 faces north (+z), positive turns toward east (+x).
pub fn yaw_to(from: V2, to: V2) -> f64 {
    (to[0] - from[0]).atan2(to[1] - from[1])
}

pub fn fwd(yaw: f64) -> V2 {
    [yaw.sin(), yaw.cos()]
}

pub fn norm2(v: V2) -> V2 {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt();
    if l < 1e-9 {
        [0.0, 1.0]
    } else {
        [v[0] / l, v[1] / l]
    }
}

pub fn add3(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub3(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn mul3(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}
pub fn dot3(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
/// Cross product (works for the left-handed x-right, y-up, z-forward world).
pub fn cross3(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn len3(a: V3) -> f64 {
    dot3(a, a).sqrt()
}
pub fn norm3(a: V3) -> V3 {
    let l = len3(a);
    if l < 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        mul3(a, 1.0 / l)
    }
}

/// Stable string hash (FNV-1a), used for deterministic seeds.
pub fn hash_str(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
