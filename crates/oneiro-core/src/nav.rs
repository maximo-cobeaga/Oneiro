//! Walking around furniture. Props that people cannot walk through (the counter, tables, stools) have a
//! footprint; characters keep their own radius away from it, go around it instead of through it, and
//! stop in front of a solid anchor at a distance that depends on how wide they are.

use crate::defs::{CharacterDef, PropInst, SetDef};
use crate::math::*;

/// An oriented rectangle on the floor: `hw` along the prop's right axis, `hd` along its forward axis.
#[derive(Clone, Copy, Debug)]
pub struct Obb {
    pub c: V2,
    pub hw: f64,
    pub hd: f64,
    pub yaw: f64,
}

impl Obb {
    fn axes(&self) -> (V2, V2) {
        // Same convention as the renderer: right = (cos, -sin), forward = (sin, cos) in (x, z).
        ([self.yaw.cos(), -self.yaw.sin()], [self.yaw.sin(), self.yaw.cos()])
    }

    /// Point in the rectangle's own coordinates (along right, along forward).
    pub fn local(&self, p: V2) -> V2 {
        let (r, f) = self.axes();
        let d = [p[0] - self.c[0], p[1] - self.c[1]];
        [d[0] * r[0] + d[1] * r[1], d[0] * f[0] + d[1] * f[1]]
    }

    pub fn world(&self, l: V2) -> V2 {
        let (r, f) = self.axes();
        [self.c[0] + r[0] * l[0] + f[0] * l[1], self.c[1] + r[1] * l[0] + f[1] * l[1]]
    }

    pub fn inflate(&self, by: f64) -> Obb {
        Obb { hw: self.hw + by, hd: self.hd + by, ..*self }
    }

    pub fn contains(&self, p: V2) -> bool {
        let l = self.local(p);
        l[0].abs() < self.hw && l[1].abs() < self.hd
    }

    /// True when the segment `a -> b` crosses the interior of the rectangle.
    pub fn blocks(&self, a: V2, b: V2) -> bool {
        let (la, lb) = (self.local(a), self.local(b));
        let (hw, hd) = (self.hw - 1e-3, self.hd - 1e-3);
        let d = [lb[0] - la[0], lb[1] - la[1]];
        let (mut t0, mut t1) = (0.0f64, 1.0f64);
        for (p, q, lim) in [(la[0], d[0], hw), (la[1], d[1], hd)] {
            if q.abs() < 1e-12 {
                if p.abs() >= lim {
                    return false;
                }
            } else {
                let (mut ta, mut tb) = ((-lim - p) / q, (lim - p) / q);
                if ta > tb {
                    std::mem::swap(&mut ta, &mut tb);
                }
                t0 = t0.max(ta);
                t1 = t1.min(tb);
                if t0 >= t1 {
                    return false;
                }
            }
        }
        t0 < t1
    }

    /// Corners pushed a little outward (waypoints for going around).
    fn corners(&self, extra: f64) -> [V2; 4] {
        let (w, d) = (self.hw + extra, self.hd + extra);
        [self.world([-w, -d]), self.world([w, -d]), self.world([w, d]), self.world([-w, d])]
    }
}

/// Footprints of the props people cannot walk through.
pub fn footprints(set: &SetDef) -> Vec<(String, Obb)> {
    set.props.iter().filter_map(|p| footprint(p).map(|o| (p.name.clone(), o))).collect()
}

pub fn footprint_of(p: &PropInst) -> Option<Obb> {
    footprint(p)
}

fn footprint(p: &PropInst) -> Option<Obb> {
    if p.wall.is_some() {
        return None;
    }
    if p.solid == Some(false) {
        return None;
    }
    let (w, d) = match p.kind.as_str() {
        // The counter top overhangs the front by 7 cm on each side (see the renderer).
        "counter" => (p.w.unwrap_or(5.0), p.d.unwrap_or(0.8) + 0.14),
        "table" => (p.w.unwrap_or(1.2), p.d.unwrap_or(0.8)),
        "stool" => (p.w.unwrap_or(0.56) * 0.9, p.w.unwrap_or(0.56) * 0.9),
        // Anything else only blocks when asked to (`solid: true`), with its own width and depth.
        _ if p.solid == Some(true) => (p.w.unwrap_or(0.8), p.d.unwrap_or(0.6)),
        _ => return None,
    };
    Some(Obb { c: p.at, hw: w / 2.0, hd: d / 2.0, yaw: p.yaw })
}

/// How far from furniture the body of this character must stay.
pub fn radius(def: &CharacterDef) -> f64 {
    (def.dims().torso_w * 0.5 + 0.12).max(0.3)
}

/// The obstacle whose footprint contains (or is closest to, within `slack`) the point.
pub fn obstacle_at(obs: &[(String, Obb)], p: V2, slack: f64) -> Option<usize> {
    obs.iter().position(|(_, o)| o.inflate(slack).contains(p))
}

/// Where to stand in front of the solid prop at `anchor` when coming from `from`: just outside the
/// footprint inflated by the character's radius, on the line from the prop toward the character.
pub fn standoff(o: &Obb, anchor: V2, from: V2, r: f64) -> V2 {
    let to = [from[0] - anchor[0], from[1] - anchor[1]];
    let l = (to[0] * to[0] + to[1] * to[1]).sqrt();
    let u = if l < 1e-6 { [0.0, -1.0] } else { [to[0] / l, to[1] / l] };
    let big = o.inflate(r + 0.06);
    let mut t = 0.0;
    while t < 20.0 {
        let p = [anchor[0] + u[0] * t, anchor[1] + u[1] * t];
        if !big.contains(p) {
            return p;
        }
        t += 0.02;
    }
    anchor
}

/// Nearest point outside the (inflated) rectangle.
pub fn push_out(o: &Obb, p: V2) -> V2 {
    let l = o.local(p);
    if l[0].abs() >= o.hw || l[1].abs() >= o.hd {
        return p;
    }
    let (dx, dz) = (o.hw - l[0].abs(), o.hd - l[1].abs());
    let eps = 0.03;
    let out = if dx < dz { [(o.hw + eps) * l[0].signum(), l[1]] } else { [l[0], (o.hd + eps) * l[1].signum()] };
    o.world(out)
}

fn clear(a: V2, b: V2, obs: &[Obb]) -> bool {
    !obs.iter().any(|o| o.blocks(a, b))
}

fn len(a: V2, b: V2) -> f64 {
    dist2(a, b)
}

/// Waypoints from `from` to `to` (the last one is `to`), going around the obstacles with the shortest
/// detour through their corners. Obstacles that already contain the start or the end are ignored (a
/// character leaving a stool, or sitting on one).
pub fn plan(from: V2, to: V2, obstacles: &[Obb], bounds: (f64, f64)) -> Vec<V2> {
    let obs: Vec<Obb> = obstacles.iter().copied().filter(|o| !o.contains(from) && !o.contains(to)).collect();
    if clear(from, to, &obs) {
        return vec![to];
    }
    let clampp = |p: V2| [p[0].clamp(-bounds.0, bounds.0), p[1].clamp(-bounds.1, bounds.1)];
    let pts: Vec<V2> = obs.iter().flat_map(|o| o.corners(0.06)).map(clampp).collect();
    let mut best: Option<(f64, Vec<V2>)> = None;
    fn offer(best: &mut Option<(f64, Vec<V2>)>, total: f64, path: Vec<V2>) {
        if best.as_ref().map_or(true, |(b, _)| total < *b) {
            *best = Some((total, path));
        }
    }
    for &c in &pts {
        if clear(from, c, &obs) && clear(c, to, &obs) {
            offer(&mut best, len(from, c) + len(c, to), vec![c, to]);
        }
    }
    if best.is_none() {
        for &c1 in &pts {
            if !clear(from, c1, &obs) {
                continue;
            }
            for &c2 in &pts {
                if c1 != c2 && clear(c1, c2, &obs) && clear(c2, to, &obs) {
                    offer(&mut best, len(from, c1) + len(c1, c2) + len(c2, to), vec![c1, c2, to]);
                }
            }
        }
    }
    best.map(|b| b.1).unwrap_or_else(|| vec![to])
}
