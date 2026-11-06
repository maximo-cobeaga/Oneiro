//! Camera model and the semantic camera solver.
//!
//! Scripts say `camera medium on Marta` and the solver turns that into a pose, applying simple
//! cinematography rules: shot sizes, lens-based framing distance, the 180-degree rule for
//! dialogue pairs and eyeline-matched over-the-shoulder shots.

use crate::catalog::*;
use crate::defs::{dir_yaw, SetDef};
use crate::math::*;
use crate::timeline::ActorTimeline;
use oneiro_lang::CameraStmt;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug)]
pub struct CameraPose {
    pub pos: V3,
    pub target: V3,
    /// Vertical field of view in degrees.
    pub fov: f64,
    pub roll: f64,
}

/// A solver failure: (code, message, suggestion).
pub type SolveError = (&'static str, String, Option<String>);

pub struct SolveEnv<'a> {
    pub set: &'a SetDef,
    pub actors: &'a [ActorTimeline],
    pub sides: HashMap<(String, String), f64>,
    pub aspect: f64,
}

#[derive(Clone, Debug)]
pub struct CameraSeg {
    pub t0: f64,
    pub t1: f64,
    pub base: CameraPose,
    pub follow: Option<(String, V2)>,
    pub mv: Option<String>,
    pub mv_dur: f64,
    pub source: String,
    pub pinned: bool,
    pub seed: f64,
    /// 1-based ordinal of this shot inside its scene (0 = automatic default shot).
    pub index: u32,
}

pub fn fov_from_lens(lens: f64) -> f64 {
    2.0 * (12.0 / lens.max(4.0)).atan().to_degrees()
}

impl CameraSeg {
    pub fn pose_at(&self, t: f64, actors: &[ActorTimeline]) -> CameraPose {
        let mut p = self.base;
        if let Some((name, origin)) = &self.follow {
            if let Some(a) = actors.iter().find(|a| &a.name == name) {
                let cur = a.kin(t).pos;
                let d = [cur[0] - origin[0], 0.0, cur[1] - origin[1]];
                p.pos = add3(p.pos, d);
                p.target = add3(p.target, d);
            }
        }
        if let Some(mv) = &self.mv {
            let u = smooth((t - self.t0) / self.mv_dur.max(0.1));
            apply_move(&mut p, mv, u, t, self.seed);
        }
        p
    }
}

fn rotate_y(v: V3, phi: f64) -> V3 {
    [v[0] * phi.cos() + v[2] * phi.sin(), v[1], -v[0] * phi.sin() + v[2] * phi.cos()]
}

fn apply_move(p: &mut CameraPose, mv: &str, u: f64, t: f64, seed: f64) {
    let fwd = norm3(sub3(p.target, p.pos));
    let right = norm3(cross3([0.0, 1.0, 0.0], fwd));
    let dist = len3(sub3(p.target, p.pos));
    match mv {
        "dolly_in" => p.pos = add3(p.pos, mul3(fwd, 0.35 * dist * u)),
        "dolly_out" => p.pos = sub3(p.pos, mul3(fwd, 0.45 * dist * u)),
        "truck_left" => {
            let d = mul3(right, -1.2 * u);
            p.pos = add3(p.pos, d);
            p.target = add3(p.target, d);
        }
        "truck_right" => {
            let d = mul3(right, 1.2 * u);
            p.pos = add3(p.pos, d);
            p.target = add3(p.target, d);
        }
        "pan" | "pan_right" | "pan_left" => {
            let sign = if mv == "pan_left" { -1.0 } else { 1.0 };
            let v = sub3(p.target, p.pos);
            p.target = add3(p.pos, rotate_y(v, sign * 22.0f64.to_radians() * u));
        }
        "tilt" | "tilt_up" | "tilt_down" => {
            let sign = if mv == "tilt_down" { -1.0 } else { 1.0 };
            p.target[1] += sign * dist * 12.0f64.to_radians().tan() * u;
        }
        "orbit" => {
            let v = sub3(p.pos, p.target);
            p.pos = add3(p.target, rotate_y(v, 38.0f64.to_radians() * u));
        }
        "crane_up" => p.pos[1] += 1.3 * u,
        "handheld" => {
            let n = |f: f64, ph: f64| (t * f + ph + seed).sin() * 0.6 + (t * f * 2.31 + ph * 1.7 + seed).sin() * 0.4;
            p.pos[0] += 0.012 * n(1.3, 0.0);
            p.pos[1] += 0.010 * n(1.7, 1.0);
            p.target[0] += 0.015 * n(1.1, 2.0);
            p.target[1] += 0.012 * n(1.9, 3.0);
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------- solver

/// (visible height as a fraction of the subject height, center height fraction, default lens mm)
fn shot_frame(name: &str) -> Option<(f64, f64, f64)> {
    Some(match name {
        "extreme_close" => (0.22, 0.78, 85.0),
        "close" => (0.50, 0.74, 70.0),
        "medium_close" => (0.70, 0.70, 55.0),
        "medium" => (1.00, 0.62, 45.0),
        "full" => (1.40, 0.52, 35.0),
        _ => return None,
    })
}

impl<'a> SolveEnv<'a> {
    fn actor(&self, name: &str) -> Option<&ActorTimeline> {
        self.actors.iter().find(|a| a.name == name)
    }

    fn pos_of(&self, name: &str, t: f64) -> Option<V2> {
        self.actor(name).map(|a| a.kin(t).pos)
    }

    fn nearest_partner(&self, name: &str, t: f64) -> Option<String> {
        let me = self.pos_of(name, t)?;
        let mut best: Option<(f64, String)> = None;
        for a in self.actors {
            if a.name == name {
                continue;
            }
            let d = dist2(me, a.kin(t).pos);
            if d < 6.0 && best.as_ref().map_or(true, |(bd, _)| d < *bd) {
                best = Some((d, a.name.clone()));
            }
        }
        best.map(|(_, n)| n)
    }

    /// World-space side of the A-B line where cameras live. Chosen once per pair (preferring
    /// the south/front side) and then kept, which implements the 180-degree rule.
    fn side_vec(&mut self, a: &str, b: &str, t: f64) -> V2 {
        let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
        let plo = self.pos_of(lo, t).unwrap_or([0.0, 0.0]);
        let phi = self.pos_of(hi, t).unwrap_or([0.0, 1.0]);
        let u = norm2([phi[0] - plo[0], phi[1] - plo[1]]);
        let perp = [-u[1], u[0]];
        let sign = *self
            .sides
            .entry((lo.to_string(), hi.to_string()))
            .or_insert_with(|| if perp[1] <= 0.0 { 1.0 } else { -1.0 });
        [perp[0] * sign, perp[1] * sign]
    }

    fn named_dir(&self, side: &str, subject: Option<(&str, f64)>) -> Option<V2> {
        match side {
            "north" | "south" | "east" | "west" => Some(fwd(dir_yaw(side).unwrap())),
            "front" | "back" | "left" | "right" => {
                let (name, t) = subject?;
                let yaw = self.actor(name)?.kin(t).yaw;
                let f = fwd(yaw);
                let r = [yaw.cos(), -yaw.sin()];
                Some(match side {
                    "front" => f,
                    "back" => [-f[0], -f[1]],
                    "right" => r,
                    _ => [-r[0], -r[1]],
                })
            }
            _ => None,
        }
    }

    fn resolve_anchor_pos(&self, name: &str) -> Option<V3> {
        let n = name.rsplit('.').next().unwrap_or(name);
        if let Some(a) = self.set.anchors.get(n) {
            return Some(a.pos);
        }
        self.set.props.iter().find(|p| p.name == n).map(|p| [p.at[0], 0.0, p.at[1]])
    }
}

pub fn solve(stmt: &CameraStmt, t: f64, env: &mut SolveEnv) -> Result<(CameraPose, Option<String>), SolveError> {
    // Validate vocabulary.
    for w in &stmt.words {
        if !SHOTS.contains(&w.as_str()) {
            return Err((
                "E1045",
                format!("unknown shot '{}'", w),
                Some(oneiro_lang::suggest::did_you_mean(w, SHOTS.iter().copied())),
            ));
        }
    }
    if let Some(a) = &stmt.angle {
        if !ANGLES.contains(&a.as_str()) {
            return Err((
                "E1045",
                format!("unknown camera angle '{}'", a),
                Some(oneiro_lang::suggest::did_you_mean(a, ANGLES.iter().copied())),
            ));
        }
    }
    if let Some(m) = &stmt.mv {
        if !MOVES.contains(&m.as_str()) {
            return Err((
                "E1045",
                format!("unknown camera move '{}'", m),
                Some(oneiro_lang::suggest::did_you_mean(m, MOVES.iter().copied())),
            ));
        }
    }
    for name in stmt.on.iter().chain(stmt.ots.iter().flat_map(|(a, b)| [a, b])) {
        if env.actor(name).is_none() {
            return Err((
                "E1041",
                format!("'{}' is not placed in this scene yet", name),
                Some(format!("add 'place {} at <anchor>' before this camera", name)),
            ));
        }
    }

    let angle = stmt.angle.as_deref().unwrap_or("eye_level");
    let shot = stmt
        .words
        .iter()
        .find(|w| w.as_str() != "establishing")
        .cloned()
        .unwrap_or_else(|| {
            if stmt.on.is_some() {
                "medium".into()
            } else {
                "wide".into()
            }
        });
    let roll = if angle == "dutch" { 12.0 } else { 0.0 };

    // ------------------------------------------------------------ over the shoulder
    if shot == "over_shoulder" {
        let (a, b) = stmt.ots.clone().ok_or(("E1043", "over_shoulder needs two characters".to_string(), Some("write: camera over_shoulder Marta -> Blob".to_string())))?;
        if a == b {
            return Err(("E1043", "over_shoulder needs two different characters".into(), None));
        }
        let pa = env.pos_of(&a, t).unwrap();
        let pb = env.pos_of(&b, t).unwrap();
        let u = norm2([pb[0] - pa[0], pb[1] - pa[1]]);
        let side = env.side_vec(&a, &b, t);
        let ha = env.actor(&a).unwrap().head_pos(t);
        let hb = env.actor(&b).unwrap().head_pos(t);
        let back = 2.6;
        let cam = [pa[0] - u[0] * back + side[0] * 0.95, ha[1] + 0.15, pa[1] - u[1] * back + side[1] * 0.95];
        let fov = fov_from_lens(stmt.lens.unwrap_or(45.0));
        let mut pose = CameraPose { pos: cam, target: [hb[0], hb[1] - 0.02, hb[2]], fov, roll };
        adjust_angle(&mut pose, angle);
        return Ok((pose, None));
    }

    // ------------------------------------------------------------ set shots (no subject)
    let is_set_shot = matches!(shot.as_str(), "wide" | "extreme_wide") && stmt.on.is_none();
    if is_set_shot {
        let center = [0.0, 1.0, 0.0];
        let mut fov = fov_from_lens(stmt.lens.unwrap_or(if shot == "extreme_wide" { 20.0 } else { 26.0 }));
        let pos;
        match stmt.from.as_deref() {
            Some(f) if env.named_dir(f, None).is_none() => {
                let a = env.resolve_anchor_pos(f).ok_or_else(|| {
                    let mut names: Vec<&str> = env.set.anchors.keys().map(|s| s.as_str()).collect();
                    names.extend(SIDES.iter().copied());
                    (
                        "E1044",
                        format!("unknown camera position '{}'", f),
                        Some(oneiro_lang::suggest::did_you_mean(f, names)),
                    )
                })?;
                let away = norm2([a[0], a[2]]);
                pos = [a[0] + away[0] * 0.5, 1.65, a[2] + away[1] * 0.5];
                fov = fov_from_lens(stmt.lens.unwrap_or(18.0));
            }
            other => {
                let dir = other.and_then(|s| env.named_dir(s, None)).unwrap_or([0.0, -1.0]);
                let hfov = 2.0 * ((fov / 2.0).to_radians().tan() * env.aspect).atan();
                let extent = if dir[0].abs() > dir[1].abs() { env.set.depth } else { env.set.width };
                let along = if dir[0].abs() > dir[1].abs() { env.set.width } else { env.set.depth };
                let mut r = (extent / 2.0 + 0.8) / (hfov / 2.0).tan() + along / 2.0 * 0.0;
                r = r.max(along / 2.0 + 1.5);
                if shot == "extreme_wide" {
                    r *= 1.35;
                }
                let h = if shot == "extreme_wide" { 3.2 } else { 1.9 };
                pos = [dir[0] * r, h, dir[1] * r];
            }
        }
        let mut pose = CameraPose { pos, target: center, fov, roll };
        adjust_angle(&mut pose, angle);
        return Ok((pose, None));
    }

    // ------------------------------------------------------------ subject shots
    let subject = stmt.on.clone().ok_or_else(|| {
        (
            "E1043",
            format!("shot '{}' needs a subject", shot),
            Some(format!("write: camera {} on Marta", shot)),
        )
    })?;
    let actor = env.actor(&subject).unwrap();
    let h = actor.def.height();
    let sp = actor.kin(t).pos;
    let syaw = actor.kin(t).yaw;
    let lift = actor.lift(t);
    let (vis_frac, center_frac, def_lens) = match shot.as_str() {
        "wide" | "extreme_wide" => (3.4, 0.45, 28.0),
        "establishing" => (3.4, 0.45, 28.0),
        s => shot_frame(s).ok_or(("E1045", format!("shot '{}' cannot be used with 'on'", s), None))?,
    };
    let vis = vis_frac * h;
    let center_y = center_frac * h + lift;
    let target = [sp[0], center_y, sp[1]];

    let mut fov = fov_from_lens(stmt.lens.unwrap_or(def_lens));
    let mut dist = vis / (2.0 * (fov / 2.0).to_radians().tan());

    let mut cam_xz: V2;
    match stmt.from.as_deref() {
        Some(f) => {
            if let Some(d) = env.named_dir(f, Some((&subject, t))) {
                cam_xz = [sp[0] + d[0] * dist, sp[1] + d[1] * dist];
            } else if let Some(a) = env.resolve_anchor_pos(f) {
                cam_xz = [a[0], a[2]];
                let real = dist2(cam_xz, sp).max(0.3);
                fov = (2.0 * (vis / (2.0 * real)).atan().to_degrees()).clamp(8.0, 75.0);
                dist = real;
            } else {
                let mut names: Vec<&str> = env.set.anchors.keys().map(|s| s.as_str()).collect();
                names.extend(SIDES.iter().copied());
                return Err((
                    "E1044",
                    format!("unknown camera position '{}'", f),
                    Some(oneiro_lang::suggest::did_you_mean(f, names)),
                ));
            }
        }
        None => {
            if let Some(partner) = env.nearest_partner(&subject, t) {
                let pp = env.pos_of(&partner, t).unwrap();
                let u = norm2([pp[0] - sp[0], pp[1] - sp[1]]);
                let side = env.side_vec(&subject, &partner, t);
                let th = 24.0f64.to_radians();
                cam_xz = [
                    sp[0] + (u[0] * th.cos() + side[0] * th.sin()) * dist,
                    sp[1] + (u[1] * th.cos() + side[1] * th.sin()) * dist,
                ];
            } else {
                // Nobody to talk to: look at the character from in front of their face (slightly off-axis).
                let f = fwd(syaw + 0.3);
                cam_xz = [sp[0] + f[0] * dist, sp[1] + f[1] * dist];
            }
        }
    }
    let _ = &mut cam_xz;
    let mut pose = CameraPose { pos: [cam_xz[0], center_y, cam_xz[1]], target, fov, roll };
    adjust_angle(&mut pose, angle);
    pose.pos[1] = pose.pos[1].max(0.15);
    let _ = dist;
    Ok((pose, Some(subject)))
}

fn adjust_angle(p: &mut CameraPose, angle: &str) {
    let d = len3(sub3(p.target, p.pos));
    match angle {
        "low" => p.pos[1] -= 0.30 * d.min(4.0),
        "high" => p.pos[1] += 0.30 * d.min(4.0),
        "top_down" => {
            let xz = [p.pos[0] - p.target[0], p.pos[2] - p.target[2]];
            let l = (xz[0] * xz[0] + xz[1] * xz[1]).sqrt().max(0.01);
            p.pos = [p.target[0] + xz[0] / l * 0.2, p.target[1] + d, p.target[2] + xz[1] / l * 0.2];
        }
        _ => {}
    }
}

pub fn describe(stmt: &CameraStmt) -> String {
    let mut s = String::from("camera");
    for w in &stmt.words {
        s.push(' ');
        s.push_str(w);
    }
    if let Some((a, b)) = &stmt.ots {
        s.push_str(&format!(" {} -> {}", a, b));
    }
    if let Some(o) = &stmt.on {
        s.push_str(&format!(" on {}", o));
    }
    if let Some(f) = &stmt.from {
        s.push_str(&format!(" from {}", f));
    }
    if let Some(a) = &stmt.angle {
        s.push_str(&format!(" angle {}", a));
    }
    if let Some(m) = &stmt.mv {
        s.push_str(&format!(" move {}", m));
    }
    s
}
