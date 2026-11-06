//! Skeleton pose of a character at one instant: where the hips, feet, shoulders, hands and head
//! are, in character space (meters, y up, x to the right on screen). The styles that do not draw
//! library blocks (`rubber_hose` and vector characters) are built on top of this.

use crate::character::View;
use oneiro_core::catalog::Gesture;
use oneiro_core::timeline::ActorFrame;
use oneiro_core::Dims;
use std::f32::consts::{PI, TAU};

pub type P2 = (f32, f32);

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

pub fn lp(a: P2, b: P2, t: f32) -> P2 {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t))
}

#[derive(Clone, Copy, Debug)]
pub struct Limb {
    pub root: P2,
    pub mid: P2,
    pub end: P2,
}

#[derive(Clone, Debug)]
pub struct Pose {
    pub hh: f32,
    pub dims: Dims,
    /// +1 facing screen-right, -1 facing screen-left.
    pub dir: f32,
    pub front: f32,
    pub prof: f32,
    pub hip_y: f32,
    /// Top of the torso.
    pub sh: f32,
    /// Torso width for the current view.
    pub tw: f32,
    /// Sideways swing of the upper body while walking (degrees).
    pub waddle: f32,
    /// Legs for x = -1 and x = +1: root is the hip, end is the foot.
    pub legs: [Limb; 2],
    /// Arms (shoulder -> elbow -> hand) for the screen-left (index 0) and screen-right (index 1) arm.
    pub arms: [Limb; 2],
    /// Head center, tilt in degrees.
    pub head: P2,
    pub tilt: f32,
    pub walk_phase: f32,
    pub walk_amp: f32,
    pub sit: f32,
    pub round_body: bool,
}

/// Computes the pose. `dims` gives the proportions (library blocks or the vector parts).
pub fn compute(hh: f32, dims: &Dims, round_body: bool, fr: &ActorFrame, v: &View) -> Pose {
    let leg = dims.leg as f32;
    let torso_h = dims.torso_h as f32;
    let torso_w0 = dims.torso_w as f32;
    let head_r = dims.head_r as f32;
    let arm_len = dims.arm_len as f32;
    let neck = dims.neck as f32;

    let kin = &fr.kin;
    let t = fr.t as f32;
    let sit = kin.sit as f32;
    let seat_h = kin.seat_h as f32;
    let stride = 0.95 * hh / 1.15;
    let phase = (std::f64::consts::TAU * kin.odo / stride as f64) as f32;
    let amp = (kin.speed as f32 / 0.55).min(1.0) * if kin.running { 1.35 } else { 1.0 };
    let front = v.front as f32;
    let side = v.side as f32;
    let af = front.abs();
    let prof = side.abs();
    let dir = if side < 0.0 { -1.0f32 } else { 1.0 };
    let bob = amp * 0.012 * hh * (1.0 - (2.0 * phase).cos()) * 0.5;
    let breath = 0.004 * hh * (TAU * 0.4 * t).sin();
    let hip0 = lerp(leg, seat_h + 0.03, sit);
    let hip_y = hip0 + bob * (1.0 - sit);

    let g = fr.gesture.as_ref();
    let gw = g.map(|g| g.weight as f32).unwrap_or(0.0);
    let gt = g.map(|g| g.time as f32).unwrap_or(0.0);
    let gk = g.map(|g| g.kind);
    let shrug_raise = if gk == Some(Gesture::Shrug) { 0.02 * hh * gw } else { 0.0 };

    let tw = torso_w0 * (0.55 + 0.45 * af);
    let sh = hip_y + torso_h + shrug_raise + breath;

    // Legs
    let hx = torso_w0 * 0.17 * (0.3 + 0.7 * af);
    let mut legs = [Limb { root: (0.0, 0.0), mid: (0.0, 0.0), end: (0.0, 0.0) }; 2];
    for (k, i) in [-1.0f32, 1.0].iter().enumerate() {
        let i = *i;
        let phase_i = phase + if i > 0.0 { PI } else { 0.0 };
        let sw = phase_i.sin() * amp;
        let lift = 0.045 * hh * amp * phase_i.cos().max(0.0);
        let phi = sw * 0.5 * (prof + 0.25);
        let fx_w = hx * i + dir * leg * phi.sin();
        let fy_w = hip0 - leg * phi.cos() + lift;
        let fx_s = hx * i + dir * 0.05 * hh * prof;
        let fy_s = hip0 - leg * 0.92;
        let (fx, fy) = (lerp(fx_w, fx_s, sit), lerp(fy_w, fy_s, sit));
        let root = (hx * i, hip_y);
        let end = (fx, fy + 0.03 * hh);
        legs[k] = Limb { root, mid: lp(root, end, 0.5), end };
    }

    // Arms
    let sh_y = sh - if round_body { 0.06 * hh } else { 0.035 * hh };
    let sh_x = if round_body { tw * 0.42 } else { tw * 0.5 * 0.97 };
    let ul = arm_len * 0.5;
    let fl = arm_len * 0.5;
    let shoulders = [(-sh_x, sh_y), (sh_x, sh_y)];
    let mut arms = [Limb { root: shoulders[0], mid: shoulders[0], end: shoulders[0] }; 2];
    for (idx, i) in [-1.0f32, 1.0].iter().enumerate() {
        let i = *i;
        let sp = shoulders[idx];
        let phase_i = phase + if i > 0.0 { PI } else { 0.0 };
        let psi = -phase_i.sin() * amp * 0.45 * (0.35 + 0.65 * prof);
        let out = 0.14f32;
        let e = (sp.0 + ul * (i * out.sin() + dir * psi.sin()), sp.1 - ul * out.cos() * psi.cos());
        let h = (
            e.0 + fl * (i * (out * 0.7).sin() + dir * (psi * 1.2 + 0.1 * amp).sin()),
            e.1 - fl * (out * 0.7).cos() * (psi * 1.2).cos(),
        );
        arms[idx] = Limb { root: sp, mid: e, end: h };
    }
    let by_angles = |sp: P2, i: f32, ths: f32, the: f32| -> Limb {
        let e = (sp.0 + ul * i * ths.sin(), sp.1 - ul * ths.cos());
        let a2 = ths + the;
        let h = (e.0 + fl * i * a2.sin(), e.1 - fl * a2.cos());
        Limb { root: sp, mid: e, end: h }
    };
    let blend = |a: Limb, b: Limb, w: f32| Limb { root: a.root, mid: lp(a.mid, b.mid, w), end: lp(a.end, b.end, w) };

    // Head
    let nod = if gk == Some(Gesture::Nod) { (TAU * 2.2 * gt).sin() * 0.03 * hh * gw } else { 0.0 };
    let speak_bob = if fr.speaking { 0.006 * hh * fr.viseme.open } else { 0.0 };
    let mut hcy = sh + neck + head_r * 0.95 + nod + speak_bob - 0.012 * hh * if gk == Some(Gesture::Shrug) { gw } else { 0.0 };
    if gk == Some(Gesture::FacePalm) {
        hcy -= 0.025 * hh * gw;
    }

    if let Some(kind) = gk {
        match kind {
            Gesture::Wave => {
                let target = by_angles(shoulders[1], 1.0, 2.45, -0.25 + 0.45 * (TAU * 2.8 * gt).sin());
                arms[1] = blend(arms[1], target, gw);
            }
            Gesture::Point => {
                let idx = if v.target_side >= 0.0 { 1 } else { 0 };
                let i = if idx == 1 { 1.0 } else { -1.0 };
                let up = (v.target_up as f32).clamp(-0.5, 0.6);
                let target = by_angles(shoulders[idx], i, 1.45 + up, 0.0);
                arms[idx] = blend(arms[idx], target, gw);
            }
            Gesture::Shrug => {
                for idx in 0..2 {
                    let i = if idx == 1 { 1.0 } else { -1.0 };
                    let sp = (shoulders[idx].0, shoulders[idx].1 + shrug_raise);
                    let target = by_angles(sp, i, 0.45, 1.25);
                    arms[idx] = blend(arms[idx], target, gw);
                }
            }
            Gesture::CrossArms => {
                let y = sh_y - 0.1 * hh;
                let l = Limb { root: shoulders[0], mid: (-sh_x * 0.95, y - 0.04 * hh), end: (sh_x * 0.55, y) };
                let r = Limb { root: shoulders[1], mid: (sh_x * 0.95, y - 0.04 * hh), end: (-sh_x * 0.55, y - 0.035 * hh) };
                arms[0] = blend(arms[0], l, gw);
                arms[1] = blend(arms[1], r, gw);
            }
            Gesture::FacePalm => {
                let target = Limb { root: shoulders[1], mid: (shoulders[1].0 + 0.06 * hh, sh_y - 0.05 * hh), end: (0.12 * head_r, hcy - 0.1 * head_r) };
                arms[1] = blend(arms[1], target, gw);
            }
            _ => {}
        }
    }
    let tilt = if gk == Some(Gesture::FacePalm) { 8.0 * gw } else { 0.0 };
    let waddle = phase.sin() * amp * 3.5;

    Pose { hh, dims: dims.clone(), dir, front, prof, hip_y, sh, tw, waddle, legs, arms, head: (0.0, hcy), tilt, walk_phase: phase, walk_amp: amp, sit, round_body }
}
