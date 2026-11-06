//! Builds the 3D mesh of a character at one instant from its definition (colors, wardrobe,
//! proportions) and the skeleton pose, in world space.

use crate::mesh::*;
use oneiro_audio::Shape;
use oneiro_core::catalog::*;
use oneiro_core::math::{angle_diff, yaw_to};
use oneiro_core::physics::{Frame, Physics, CAPE_COLS, CAPE_ROWS};
use oneiro_core::pose::{self, View};
use oneiro_core::timeline::ActorFrame;
use oneiro_core::{CharacterDef, Rgb};
use std::f32::consts::PI;

fn col(c: Rgb) -> V3 {
    [c.0 as f32 / 255.0, c.1 as f32 / 255.0, c.2 as f32 / 255.0]
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The character's own axes in the world: `l` to its left, `f` forward, `u` up.
struct Axes {
    l: V3,
    f: V3,
    u: V3,
    o: V3,
}

impl Axes {
    fn at(&self, x: f32, y: f32, z: f32) -> V3 {
        add(self.o, add(add(mul(self.l, x), mul(self.u, y)), mul(self.f, z)))
    }
}

/// Rotation of the upper body around the forward axis (the walking waddle), about the hips.
fn roll(x: f32, y: f32, hip_y: f32, deg: f32) -> (f32, f32) {
    let (s, c) = deg.to_radians().sin_cos();
    let dy = y - hip_y;
    (x * c - dy * s, hip_y + x * s + dy * c)
}

pub struct Built {
    pub mesh: Mesh,
}

/// The mesh of `def` at the instant `fr`. `phys` supplies the simulated cape and tail.
pub fn character_mesh(def: &CharacterDef, fr: &ActorFrame, phys: Option<&Physics>) -> Built {
    let mut m = Mesh::default();
    let hh = def.height() as f32;
    let dm = def.dims();
    let kin = &fr.kin;
    let round = def.body.kind == "round";
    let pose = pose::compute(hh, &dm, round, fr, &View::frontal());

    // Axes in the world.
    let frame = Frame::new(kin.pos, kin.yaw);
    let ax = Axes { l: [frame.left[0] as f32, 0.0, frame.left[1] as f32], f: [frame.fwd[0] as f32, 0.0, frame.fwd[1] as f32], u: [0.0, 1.0, 0.0], o: [kin.pos[0] as f32, 0.0, kin.pos[1] as f32] };

    // Wardrobe colors.
    let (mut top, mut bottom, mut shoes, mut hat, mut acc) = (None, None, None, None, None);
    for n in &def.wardrobe {
        if let Some(it) = wardrobe_item(n) {
            match it.slot {
                Slot::Top => top = Some(it),
                Slot::Bottom => bottom = Some(it),
                Slot::Shoes => shoes = Some(it),
                Slot::Hat => hat = Some(it),
                Slot::Accessory => acc = Some(it),
            }
        }
    }
    let kid = def.body.kind == "kid";
    let top_c = col(top.map(|i| i.color).unwrap_or(Rgb(140, 140, 148)));
    let top_a = col(top.map(|i| i.accent).unwrap_or(Rgb(100, 100, 108)));
    let bottom_c = col(bottom.map(|i| i.color).unwrap_or(Rgb(70, 70, 92)));
    let shoes_c = col(shoes.map(|i| i.color).unwrap_or(Rgb(30, 30, 34)));
    let skin = col(def.skin());
    let hand_c = if kid { top_a } else { skin };
    let hair_c = col(def.head.color("hair", Rgb(92, 60, 38)));
    let bald = def.head.num("bald", 0.0) > 0.5;

    // ---------------------------------------------------------------- walk cycle in depth
    let phase = pose.walk_phase;
    let amp = pose.walk_amp;
    let sit = pose.sit;
    let leg = dm.leg as f32;
    let arm_len = dm.arm_len as f32;
    let tw = dm.torso_w as f32;
    let torso_h = dm.torso_h as f32;
    let hip_y = pose.hip_y;
    let sh_y = pose.sh;

    // Legs: hip -> knee -> foot, striding forward and back when walking, thighs forward when sitting.
    let (rl, ra, rh) = (0.038 * hh, 0.028 * hh, 0.036 * hh);
    for (k, l) in pose.legs.iter().enumerate() {
        let phase_i = phase + if k == 1 { PI } else { 0.0 };
        let stride = phase_i.sin() * amp * 0.34 * leg;
        let lift = phase_i.cos().max(0.0) * amp * 0.03 * hh;
        let hip = (l.root.0, l.root.1);
        let foot_z = lerp(stride, 0.52 * leg, sit);
        let foot = (l.end.0, l.end.1 + lift);
        let knee_z = lerp(stride * 0.5 + 0.05 * hh * amp.min(1.0), 0.5 * leg, sit);
        let knee = (lerp(hip.0, foot.0, 0.5), lerp(hip.1, foot.1, 0.5) + lerp(0.0, 0.02 * hh, sit));
        let (h3, k3, f3) = (ax.at(hip.0, hip.1, 0.0), ax.at(knee.0, knee.1, knee_z), ax.at(foot.0, foot.1, foot_z));
        m.capsule(h3, k3, rl, rl * 0.95, bottom_c);
        m.capsule(k3, f3, rl * 0.95, rl * 0.85, bottom_c);
        // Shoe: a long rounded ellipsoid pointing forward.
        let shoe = Xf::basis(add(f3, mul(ax.f, 0.03 * hh)), ax.l, ax.u, ax.f, [0.05 * hh, 0.028 * hh, 0.085 * hh]);
        m.ellipsoid(&Xf::new(add(shoe.o, mul(ax.u, -0.012 * hh)), shoe.ax, shoe.ay, shoe.az), 0.8, 0.8, shoes_c);
    }

    // ---------------------------------------------------------------- upper body (rolled by the waddle)
    let rl_pt = |x: f32, y: f32, z: f32| {
        let (rx, ry) = roll(x, y, hip_y, pose.waddle);
        ax.at(rx, ry, z)
    };
    // Torso
    let depth = if round { tw * 0.42 } else { tw * 0.34 };
    let tcenter = rl_pt(0.0, hip_y + torso_h * 0.5, 0.0);
    let (tl, tu) = {
        let (s, c) = pose.waddle.to_radians().sin_cos();
        (add(mul(ax.l, c), mul(ax.u, s)), add(mul(ax.u, c), mul(ax.l, -s)))
    };
    if round {
        m.ellipsoid(&Xf::basis(tcenter, tl, tu, ax.f, [tw * 0.5, torso_h * 0.54, depth]), 1.0, 1.0, top_c);
    } else {
        m.ellipsoid(&Xf::basis(tcenter, tl, tu, ax.f, [tw * 0.5, torso_h * 0.5, depth]), 0.62, 0.7, top_c);
    }
    if let Some(it) = top {
        if it.name == "apron_white" {
            m.ellipsoid(&Xf::basis(add(tcenter, mul(ax.f, depth * 0.55)), tl, tu, ax.f, [tw * 0.36, torso_h * 0.42, depth * 0.55]), 0.7, 0.8, col(Rgb(255, 255, 252)));
        }
    }
    if let Some(a) = acc {
        // scarf: a flat ring around the neck
        m.ellipsoid(&Xf::basis(rl_pt(0.0, sh_y, 0.0), tl, tu, ax.f, [tw * 0.42, 0.034 * hh, depth * 0.95]), 1.0, 1.0, col(a.color));
    }
    // Neck
    let neck_top = rl_pt(0.0, sh_y + dm.neck as f32 + 0.03 * hh, 0.0);
    m.capsule(rl_pt(0.0, sh_y - 0.02 * hh, 0.0), neck_top, 0.05 * hh, 0.045 * hh, skin);

    // Arms: shoulder -> elbow -> hand, swinging opposite to the legs.
    for (idx, a) in pose.arms.iter().enumerate() {
        let leg_phase = phase + if idx == 1 { PI } else { 0.0 };
        let swing = -leg_phase.sin() * amp * 0.34 * arm_len * (1.0 - 0.8 * sit);
        let sh = rl_pt(a.root.0, a.root.1, 0.0);
        let el = rl_pt(a.mid.0, a.mid.1, swing * 0.5 - 0.02 * hh * amp);
        let ha = rl_pt(a.end.0, a.end.1, swing + 0.015 * hh * sit);
        m.sphere(sh, ra * 1.25, top_c);
        m.capsule(sh, el, ra, ra * 0.92, top_c);
        m.capsule(el, ha, ra * 0.92, ra * 0.85, top_c);
        m.sphere(ha, rh, hand_c);
    }

    // ---------------------------------------------------------------- head
    let head_r = dm.head_r as f32;
    // Head turns toward its gaze target (the same rule as the 2D styles).
    let turn_to = |g: Option<[f64; 3]>| -> f64 {
        match g {
            Some(pt) => angle_diff(yaw_to(kin.pos, [pt[0], pt[2]]), kin.yaw).clamp(-1.3, 1.3) * 0.85,
            None => 0.0,
        }
    };
    let mut head_turn = (turn_to(fr.gaze_prev) + (turn_to(fr.gaze) - turn_to(fr.gaze_prev)) * fr.gaze_u) as f32;
    if let Some(g) = &fr.gesture {
        if g.kind == Gesture::ShakeHead {
            head_turn += 0.55 * (std::f32::consts::TAU * 2.4 * g.time as f32).sin() * g.weight as f32;
        }
    }
    // Head axes: turned around the vertical axis (toward the character's right when positive), then tilted.
    let (hs, hc) = head_turn.sin_cos();
    let h_f = norm(add(mul(ax.f, hc), mul(ax.l, -hs)));
    let h_l = norm(add(mul(ax.l, hc), mul(ax.f, hs)));
    let (ts, tc) = pose.tilt.to_radians().sin_cos();
    let h_u = add(mul(ax.u, tc), mul(h_l, -ts));
    let h_l = add(mul(h_l, tc), mul(ax.u, ts));
    let hc3 = rl_pt(pose.head.0, pose.head.1, 0.0);
    let hw = |x: f32, y: f32, z: f32| add(hc3, add(add(mul(h_l, x), mul(h_u, y)), mul(h_f, z)));
    let (rx, ry, rz, e) = match def.head.kind.as_str() {
        "square" => (head_r * 0.97, head_r * 0.93, head_r * 0.93, (0.55, 0.5)),
        "oval" => (head_r * 0.86, head_r * 1.08, head_r * 0.92, (1.0, 1.0)),
        _ => (head_r, head_r * 0.98, head_r * 0.96, (1.0, 1.0)),
    };
    m.ellipsoid(&Xf::basis(hc3, h_l, h_u, h_f, [rx, ry, rz]), e.0, e.1, skin);

    // Hair (a cap over the top and back of the head) unless bald or wearing a hat.
    if !bald && hat.is_none() {
        let xf = Xf::basis(add(hc3, add(mul(h_u, head_r * 0.02), mul(h_f, -head_r * 0.06))), h_l, h_u, h_f, [rx * 1.03, ry * 1.03, rz * 1.05]);
        m.ellipsoid_part(&xf, e.0, e.1, 22.0, 90.0, hair_c, true);
    }
    // Hats
    if let Some(h) = hat {
        let hc_col = col(h.color);
        let ha_col = col(h.accent);
        let top_pt = hw(0.0, ry * 0.46, 0.0);
        match h.name {
            "chef_hat" => {
                m.cylinder(hw(0.0, ry * 0.5, 0.0), head_r * 0.45, head_r * 0.95, hc_col);
                for (x, z) in [(-0.5f32, 0.0f32), (0.5, 0.0), (0.0, 0.35), (0.0, -0.35)] {
                    m.sphere(hw(x * head_r, ry * 0.5 + head_r * 0.62, z * head_r), head_r * 0.52, hc_col);
                }
            }
            "green_hat" | "yellow_hat" => {
                m.ellipsoid_part(&Xf::basis(hc3, h_l, h_u, h_f, [rx * 1.07, ry * 1.07, rz * 1.07]), 1.0, 1.0, 18.0, 90.0, hc_col, true);
                // brim
                let brim = if h.name == "yellow_hat" { 1.55 } else { 1.18 };
                m.ellipsoid(&Xf::basis(top_pt, h_l, h_u, h_f, [head_r * brim, head_r * 0.07, head_r * brim]), 1.0, 1.0, ha_col);
            }
            _ => {
                // beanie
                m.ellipsoid_part(&Xf::basis(hc3, h_l, h_u, h_f, [rx * 1.07, ry * 1.07, rz * 1.07]), 1.0, 1.0, 18.0, 90.0, hc_col, true);
                m.ellipsoid_part(&Xf::basis(hc3, h_l, h_u, h_f, [rx * 1.09, ry * 1.09, rz * 1.09]), 1.0, 1.0, 14.0, 30.0, ha_col, true);
                m.sphere(hw(0.0, ry * 1.12, 0.0), head_r * 0.2, ha_col);
            }
        }
    }

    // ---------------------------------------------------------------- face
    let ex = &fr.expr;
    let spacing = def.eyes.num("spacing", 0.4) as f32;
    let surf = |x: f32, y: f32| -> f32 {
        let q = 1.0 - (x / rx).powi(2) - (y / ry).powi(2);
        if def.head.kind == "square" {
            rz * 0.97
        } else {
            rz * q.max(0.0).sqrt()
        }
    };
    let eye_y = head_r * 0.06;
    let eye_x = spacing * head_r * 0.72;
    let blink = fr.blink as f32;
    let open = (ex.eye_open as f32 * (1.0 - 0.92 * blink)).max(0.07);
    let (erx, ery, whites) = match def.eyes.kind.as_str() {
        "ovals" => (0.2, 0.25, true),
        "wide" => (0.26, 0.33, true),
        _ => (0.075, 0.095, false),
    };
    // Where the eyes look.
    let (mut gdx, mut gdy) = (0.0f32, 0.0f32);
    if let Some(pt) = fr.gaze {
        let hyaw = kin.yaw + head_turn as f64;
        let residual = angle_diff(yaw_to(kin.pos, [pt[0], pt[2]]), hyaw).clamp(-0.9, 0.9) as f32;
        gdx = -residual * 0.5;
        let dist = ((pt[0] - kin.pos[0]).powi(2) + (pt[2] - kin.pos[1]).powi(2)).sqrt().max(0.5);
        gdy = (((pt[1] as f32) - hh * 0.78) / dist as f32).clamp(-0.5, 0.5);
    }
    for k in [-1.0f32, 1.0] {
        let x = k * eye_x;
        let z = surf(x, eye_y) + head_r * 0.01;
        let c = hw(x, eye_y, z);
        if whites {
            let (wx, wy) = (head_r * erx, head_r * ery * open);
            m.ellipsoid(&Xf::basis(c, h_l, h_u, h_f, [wx, wy, head_r * 0.07]), 1.0, 1.0, [0.99, 0.99, 0.98]);
            let pr = (0.075 * head_r).min(wy * 0.9);
            let pc = hw(x + gdx * wx * 0.5, eye_y + gdy * wy * 0.4, z + head_r * 0.05);
            m.ellipsoid(&Xf::basis(pc, h_l, h_u, h_f, [pr * 0.9, pr, pr * 0.5]), 1.0, 1.0, [0.08, 0.07, 0.09]);
        } else {
            let pc = hw(x + gdx * head_r * 0.07, eye_y + gdy * head_r * 0.03, z + head_r * 0.02);
            m.ellipsoid(&Xf::basis(pc, h_l, h_u, h_f, [head_r * erx, head_r * ery * open, head_r * 0.05]), 1.0, 1.0, [0.08, 0.07, 0.09]);
        }
        // Eyebrow
        let by = eye_y + head_r * (ery + 0.14) + ex.brow_y as f32 * 0.14 * head_r;
        let ang = (ex.brow_angle as f32).to_radians();
        let half = 0.19 * head_r;
        let inner = -k;
        let (p0x, p0y) = (x - inner * half, by + ang.sin() * half);
        let (p1x, p1y) = (x + inner * half, by - ang.sin() * half);
        let zb = |x: f32, y: f32| surf(x, y) + head_r * 0.015;
        m.capsule(hw(p0x, p0y, zb(p0x, p0y)), hw(p1x, p1y, zb(p1x, p1y)), 0.026 * head_r, 0.026 * head_r, [0.08, 0.07, 0.09]);
    }
    // Mouth, shaped by the viseme while speaking (same table as the 2D styles).
    let smile = ex.smile as f32;
    let speaking = fr.speaking;
    let mut mopen = if speaking { fr.viseme.open } else { 0.0 };
    mopen = (mopen + ex.mouth_bias as f32 * if speaking { 0.5 } else { 1.0 }).min(1.0);
    let shape = if speaking {
        fr.viseme.shape
    } else if ex.mouth_bias > 0.15 {
        if smile > 0.5 {
            Shape::A
        } else {
            Shape::O
        }
    } else {
        Shape::Rest
    };
    let my = -head_r * 0.42 + smile * 0.05 * head_r;
    let mz = surf(0.0, my) + head_r * 0.005;
    if shape == Shape::Rest || mopen < 0.07 {
        // A smooth curve (the same quadratic as the 2D mouth) sampled in short capsules.
        let w = 0.34 * head_r;
        let (corner_y, ctrl_y) = (my + smile * 0.10 * head_r, my - smile * 0.20 * head_r);
        let p = |x: f32, y: f32| hw(x, y, surf(x, y) + head_r * 0.01);
        let at = |t: f32| (lerp(-w, w, t), (1.0 - t) * (1.0 - t) * corner_y + 2.0 * t * (1.0 - t) * ctrl_y + t * t * corner_y);
        for k in 0..8 {
            let (a, b) = (at(k as f32 / 8.0), at((k + 1) as f32 / 8.0));
            m.capsule(p(a.0, a.1), p(b.0, b.1), 0.022 * head_r, 0.022 * head_r, [0.08, 0.07, 0.09]);
        }
    } else {
        let (mw, mh) = match shape {
            Shape::A => (0.40, 0.10 + 0.40 * mopen),
            Shape::E => (0.50, 0.06 + 0.22 * mopen),
            Shape::O => (0.24, 0.08 + 0.34 * mopen),
            Shape::U => (0.17, 0.06 + 0.22 * mopen),
            Shape::S => (0.42, 0.07 + 0.08 * mopen),
            Shape::FV => (0.38, 0.08 + 0.08 * mopen),
            Shape::Rest => (0.3, 0.05),
        };
        let c = hw(0.0, my - mh * head_r * 0.15, mz);
        m.ellipsoid(&Xf::basis(c, h_l, h_u, h_f, [mw * head_r, mh * head_r, head_r * 0.06]), 1.0, 1.0, [0.23, 0.05, 0.09]);
        if matches!(shape, Shape::A | Shape::O) && mh > 0.14 {
            m.ellipsoid(&Xf::basis(hw(0.0, my - mh * head_r * 0.55, mz + head_r * 0.01), h_l, h_u, h_f, [mw * head_r * 0.5, mh * head_r * 0.3, head_r * 0.05]), 1.0, 1.0, [0.81, 0.34, 0.39]);
        }
    }

    // ---------------------------------------------------------------- simulated extras
    if let (Some(p), Some(c)) = (phys, def.toon3d.cape) {
        if let Some(pts) = p.cape_at(fr.t) {
            let pts: Vec<V3> = pts.iter().map(|q| [q[0] as f32, q[1] as f32, q[2] as f32]).collect();
            m.sheet(&pts, CAPE_ROWS, CAPE_COLS, 0.012 * hh, col(c));
        }
    }
    if let (Some(p), Some(c)) = (phys, def.toon3d.tail) {
        if let Some(pts) = p.tail_at(fr.t) {
            let n = pts.len();
            for i in 0..n - 1 {
                let r0 = lerp(0.045, 0.085, (i as f32 / (n - 1) as f32).sin()) * hh;
                let r1 = lerp(0.045, 0.085, ((i + 1) as f32 / (n - 1) as f32).sin()) * hh;
                let f = |q: [f64; 3]| [q[0] as f32, q[1] as f32, q[2] as f32];
                m.capsule(f(pts[i]), f(pts[i + 1]), r0, r1 * if i + 2 == n { 0.6 } else { 1.0 }, col(c));
            }
        }
    }
    Built { mesh: m }
}
