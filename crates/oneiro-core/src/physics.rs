//! Secondary motion for `toon_3d` characters: a cloth cape and a springy tail.
//!
//! Physics has history (the cape swings because of how the character moved a moment ago), but frames
//! must stay a pure function of time so they can be rendered in parallel and reproduced exactly. So the
//! simulation runs once, in order, when a scene is compiled, and stores the result; rendering only
//! reads (and interpolates) it. Everything is deterministic: fixed time step, no randomness.

use crate::compile::Scene;
use crate::defs::CharacterDef;
use crate::math::*;
use crate::timeline::ActorTimeline;
use std::sync::Arc;

pub const SIM_HZ: f64 = 120.0;
/// Every second step is stored.
pub const STORE_HZ: f64 = 60.0;
pub const CAPE_COLS: usize = 9;
pub const CAPE_ROWS: usize = 8;
pub const TAIL_POINTS: usize = 7;
const MAX_SECONDS: f64 = 180.0;
const GRAVITY: f64 = 9.8;

/// Simulated points of one actor, one entry per stored frame (`STORE_HZ`), in world space.
#[derive(Clone, Debug, Default)]
pub struct Physics {
    /// `CAPE_COLS * CAPE_ROWS` points, row-major (row 0 is the top edge on the shoulders).
    pub cape: Vec<Vec<V3>>,
    /// `TAIL_POINTS` points from the root to the tip.
    pub tail: Vec<Vec<V3>>,
}

fn sample(frames: &[Vec<V3>], t: f64) -> Option<Vec<V3>> {
    if frames.is_empty() {
        return None;
    }
    let x = (t * STORE_HZ).max(0.0);
    let i = (x.floor() as usize).min(frames.len() - 1);
    let j = (i + 1).min(frames.len() - 1);
    let u = (x - i as f64).clamp(0.0, 1.0);
    Some(frames[i].iter().zip(&frames[j]).map(|(a, b)| [lerp(a[0], b[0], u), lerp(a[1], b[1], u), lerp(a[2], b[2], u)]).collect())
}

impl Physics {
    pub fn cape_at(&self, t: f64) -> Option<Vec<V3>> {
        sample(&self.cape, t)
    }

    pub fn tail_at(&self, t: f64) -> Option<Vec<V3>> {
        sample(&self.tail, t)
    }
}

/// The character's frame of reference: `x` to its left (screen-right when it faces the camera), `y` up,
/// `z` forward.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub pos: V2,
    pub fwd: V2,
    pub left: V2,
}

impl Frame {
    pub fn new(pos: V2, yaw: f64) -> Frame {
        let f = fwd(yaw);
        Frame { pos, fwd: f, left: [-f[1], f[0]] }
    }

    pub fn world(&self, x: f64, y: f64, z: f64) -> V3 {
        [self.pos[0] + x * self.left[0] + z * self.fwd[0], y, self.pos[1] + x * self.left[1] + z * self.fwd[1]]
    }

    pub fn local(&self, p: V3) -> (f64, f64, f64) {
        let (dx, dz) = (p[0] - self.pos[0], p[2] - self.pos[1]);
        (dx * self.left[0] + dz * self.left[1], p[1], dx * self.fwd[0] + dz * self.fwd[1])
    }
}

/// Body measurements the simulation needs (shared with the 3D renderer).
#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub hh: f64,
    pub hip: f64,
    pub shoulder: f64,
    pub torso_w: f64,
    /// Half of the torso depth (front to back).
    pub torso_d: f64,
}

pub fn body_at(def: &CharacterDef, kin: &crate::timeline::Kin) -> Body {
    let dm = def.dims();
    let hip = lerp(dm.leg, kin.seat_h + 0.03, kin.sit);
    Body { hh: def.height(), hip, shoulder: hip + dm.torso_h, torso_w: dm.torso_w, torso_d: dm.torso_w * 0.17 }
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: V3, k: f64) -> V3 {
    [a[0] * k, a[1] * k, a[2] * k]
}

fn length(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

/// Verlet particle system with a pinned set of points.
struct Particles {
    p: Vec<V3>,
    prev: Vec<V3>,
    pinned: Vec<bool>,
}

impl Particles {
    fn new(p: Vec<V3>, pinned: Vec<bool>) -> Particles {
        Particles { prev: p.clone(), p, pinned }
    }

    fn integrate(&mut self, dt: f64, damping: f64, gravity: f64) {
        for i in 0..self.p.len() {
            if self.pinned[i] {
                continue;
            }
            let vel = scale(sub(self.p[i], self.prev[i]), damping);
            self.prev[i] = self.p[i];
            self.p[i] = add(add(self.p[i], vel), [0.0, -gravity * dt * dt, 0.0]);
        }
    }

    fn distance(&mut self, a: usize, b: usize, rest: f64, stiffness: f64) {
        let d = sub(self.p[b], self.p[a]);
        let l = length(d);
        if l < 1e-9 {
            return;
        }
        let diff = (l - rest) / l * stiffness;
        let (pa, pb) = (self.pinned[a], self.pinned[b]);
        match (pa, pb) {
            (true, true) => {}
            (true, false) => self.p[b] = sub(self.p[b], scale(d, diff)),
            (false, true) => self.p[a] = add(self.p[a], scale(d, diff)),
            (false, false) => {
                self.p[a] = add(self.p[a], scale(d, 0.5 * diff));
                self.p[b] = sub(self.p[b], scale(d, 0.5 * diff));
            }
        }
    }
}

// ------------------------------------------------------------------ cape

struct Cape {
    ps: Particles,
    col_gap: f64,
    row_gap: f64,
}

impl Cape {
    fn anchors(body: &Body, fr: &Frame) -> Vec<V3> {
        let z = -(body.torso_d + 0.015 * body.hh);
        (0..CAPE_COLS)
            .map(|c| {
                let x = (c as f64 / (CAPE_COLS - 1) as f64 - 0.5) * body.torso_w * 1.04;
                fr.world(x, body.shoulder - 0.02 * body.hh, z)
            })
            .collect()
    }

    fn new(body: &Body, fr: &Frame) -> Cape {
        let top = Cape::anchors(body, fr);
        let row_gap = body.hh * 0.46 / (CAPE_ROWS - 1) as f64;
        let col_gap = length(sub(top[1], top[0]));
        let mut p = Vec::new();
        let mut pinned = Vec::new();
        for r in 0..CAPE_ROWS {
            for c in 0..CAPE_COLS {
                // Hanging straight down, slightly flaring backwards.
                let back = scale([-fr.fwd[0], 0.0, -fr.fwd[1]], r as f64 * 0.004);
                p.push(add(add(top[c], [0.0, -(r as f64) * row_gap, 0.0]), back));
                pinned.push(r == 0);
            }
        }
        Cape { ps: Particles::new(p, pinned), col_gap, row_gap }
    }

    fn step(&mut self, dt: f64, body: &Body, fr: &Frame) {
        let top = Cape::anchors(body, fr);
        self.ps.integrate(dt, 0.985, GRAVITY);
        for c in 0..CAPE_COLS {
            self.ps.p[c] = top[c];
            self.ps.prev[c] = top[c];
        }
        for _ in 0..8 {
            for r in 0..CAPE_ROWS {
                for c in 0..CAPE_COLS {
                    let i = r * CAPE_COLS + c;
                    if c + 1 < CAPE_COLS {
                        self.ps.distance(i, i + 1, self.col_gap, 1.0);
                    }
                    if r + 1 < CAPE_ROWS {
                        self.ps.distance(i, i + CAPE_COLS, self.row_gap, 1.0);
                    }
                    // Bending resistance keeps the cloth from folding like a rope.
                    if c + 2 < CAPE_COLS {
                        self.ps.distance(i, i + 2, self.col_gap * 2.0, 0.25);
                    }
                    if r + 2 < CAPE_ROWS {
                        self.ps.distance(i, i + 2 * CAPE_COLS, self.row_gap * 2.0, 0.25);
                    }
                }
            }
            self.collide(body, fr);
        }
    }

    fn collide(&mut self, body: &Body, fr: &Frame) {
        let a = body.torso_w * 0.5 + 0.012 * body.hh;
        let b = body.torso_d + 0.012 * body.hh;
        for i in CAPE_COLS..self.ps.p.len() {
            let mut p = self.ps.p[i];
            if p[1] < 0.02 {
                p[1] = 0.02;
            }
            let (x, y, z) = fr.local(p);
            if y > body.hip - 0.06 * body.hh && y < body.shoulder + 0.03 * body.hh {
                let e = (x / a).powi(2) + (z / b).powi(2);
                if e < 1.0 {
                    // Push out of the torso (backwards when it is dead center).
                    let k = if e > 1e-6 { (1.02 / e).sqrt() } else { 1.0 };
                    let (nx, nz) = if e > 1e-6 { (x * k, z * k) } else { (0.0, -b * 1.02) };
                    p = fr.world(nx, y, nz);
                }
            }
            self.ps.p[i] = p;
        }
    }
}

// ------------------------------------------------------------------ tail

struct Tail {
    ps: Particles,
    seg: f64,
}

impl Tail {
    fn rest(body: &Body, fr: &Frame, seg: f64) -> Vec<V3> {
        let root_y = body.hip + 0.3 * (body.shoulder - body.hip);
        let z0 = -(body.torso_d + 0.01 * body.hh);
        (0..TAIL_POINTS)
            .map(|i| {
                let t = i as f64;
                // Out the back, curving up like a squirrel's.
                fr.world(0.0, root_y - 0.03 * body.hh * t.min(2.0) + 0.045 * body.hh * (t - 2.0).max(0.0), z0 - seg * t * 0.92)
            })
            .collect()
    }

    fn new(body: &Body, fr: &Frame) -> Tail {
        let seg = body.hh * 0.085;
        let rest = Tail::rest(body, fr, seg);
        let pinned = (0..TAIL_POINTS).map(|i| i == 0).collect();
        Tail { ps: Particles::new(rest, pinned), seg }
    }

    fn step(&mut self, dt: f64, body: &Body, fr: &Frame) {
        let rest = Tail::rest(body, fr, self.seg);
        self.ps.integrate(dt, 0.965, GRAVITY * 0.45);
        self.ps.p[0] = rest[0];
        self.ps.prev[0] = rest[0];
        for it in 0..6 {
            for i in 0..TAIL_POINTS - 1 {
                self.ps.distance(i, i + 1, self.seg, 1.0);
            }
            if it == 0 {
                // A soft spring toward the rest shape: the tail holds its pose and lags behind moves.
                for i in 1..TAIL_POINTS {
                    let k = 0.03 * (1.0 - 0.5 * i as f64 / TAIL_POINTS as f64);
                    self.ps.p[i] = add(self.ps.p[i], scale(sub(rest[i], self.ps.p[i]), k));
                }
            }
            for i in 1..TAIL_POINTS {
                if self.ps.p[i][1] < 0.02 {
                    self.ps.p[i][1] = 0.02;
                }
            }
        }
    }
}

/// Runs the simulation of one actor over `dur` seconds. `None` when the character has no extras or is
/// not drawn in `toon_3d`.
pub fn simulate(actor: &ActorTimeline, dur: f64) -> Option<Physics> {
    let def = &actor.def;
    if def.style != "toon_3d" || !def.toon3d.any() {
        return None;
    }
    let dur = dur.clamp(0.0, MAX_SECONDS);
    let dt = 1.0 / SIM_HZ;
    let steps = (dur * SIM_HZ).ceil() as usize + 1;
    let at = |t: f64| {
        let kin = actor.kin(t);
        (body_at(def, &kin), Frame::new(kin.pos, kin.yaw))
    };
    let (b0, f0) = at(0.0);
    let mut cape = def.toon3d.cape.map(|_| Cape::new(&b0, &f0));
    let mut tail = def.toon3d.tail.map(|_| Tail::new(&b0, &f0));
    // Let everything settle at the starting pose before time 0.
    for _ in 0..(SIM_HZ as usize) {
        if let Some(c) = &mut cape {
            c.step(dt, &b0, &f0);
        }
        if let Some(t) = &mut tail {
            t.step(dt, &b0, &f0);
        }
    }
    let mut out = Physics::default();
    for s in 0..steps {
        let (b, f) = at(s as f64 * dt);
        if let Some(c) = &mut cape {
            c.step(dt, &b, &f);
        }
        if let Some(t) = &mut tail {
            t.step(dt, &b, &f);
        }
        if s % 2 == 0 {
            if let Some(c) = &cape {
                out.cape.push(c.ps.p.clone());
            }
            if let Some(t) = &tail {
                out.tail.push(t.ps.p.clone());
            }
        }
    }
    Some(out)
}

/// Simulates every actor of the scene that needs it and stores the result in its timeline.
pub fn attach(scene: &mut Scene) {
    let dur = scene.dur;
    for a in scene.actors.iter_mut() {
        a.physics = simulate(a, dur).map(Arc::new);
    }
}
