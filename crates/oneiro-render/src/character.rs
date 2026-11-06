//! The `cutout_2d` character: a paper-cutout puppet in the South Park spirit, drawn procedurally
//! from the character definition. Drawing is view-dependent (front, profile, back) so that the
//! same puppet turns convincingly while the camera moves around the set.

use crate::draw::Painter;
use oneiro_audio::Shape;
use oneiro_core::catalog::*;
use oneiro_core::timeline::ActorFrame;
use oneiro_core::{CharacterDef, Rgb};
use std::f32::consts::{PI, TAU};
use tiny_skia::{PathBuilder, Pixmap, Transform};

pub use oneiro_core::pose::View;
pub struct Placement {
    pub sx: f32,
    pub sy: f32,
    /// Pixels per meter at the character's depth.
    pub scale: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

type P2 = (f32, f32);

fn lp(a: P2, b: P2, t: f32) -> P2 {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t))
}

struct ArmPose {
    elbow: P2,
    hand: P2,
}

pub fn draw_character(
    pm: &mut Pixmap,
    def: &CharacterDef,
    fr: &ActorFrame,
    v: &View,
    pl: &Placement,
    tint: [f64; 3],
    px: f32,
) {
    draw_character_ex(pm, def, fr, v, pl, tint, px, 1.0)
}

/// Like `draw_character`, with the outline width multiplied by `outline` (0 = no outlines, for clay).
#[allow(clippy::too_many_arguments)]
pub fn draw_character_ex(
    pm: &mut Pixmap,
    def: &CharacterDef,
    fr: &ActorFrame,
    v: &View,
    pl: &Placement,
    tint: [f64; 3],
    px: f32,
    outline: f32,
) {
    let s = pl.scale;
    let tf = Transform::from_translate(pl.sx, pl.sy).pre_scale(s, -s);
    let mut p = Painter { pm, tf, ow: 2.4 * px / s * outline, tint };
    let ow0 = p.ow;

    let hh = def.height() as f32;
    let kid = def.body.kind == "kid";
    let round = def.body.kind == "round";
    let dm = def.dims();
    let leg = dm.leg as f32;
    let torso_h = dm.torso_h as f32;
    let torso_w0 = dm.torso_w as f32;
    let head_r = dm.head_r as f32;
    let arm_len = dm.arm_len as f32;
    let neck = dm.neck as f32;

    // Wardrobe
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
    let top_c = top.map(|i| i.color).unwrap_or(Rgb(140, 140, 148));
    let top_a = top.map(|i| i.accent).unwrap_or(Rgb(100, 100, 108));
    let bottom_c = bottom.map(|i| i.color).unwrap_or(Rgb(70, 70, 92));
    let shoes_c = shoes.map(|i| i.color).unwrap_or(Rgb(30, 30, 34));
    let skin = def.skin();
    let hand_c = if kid { top_a } else { skin };

    // Kinematics
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

    // Gestures
    let g = fr.gesture.as_ref();
    let gw = g.map(|g| g.weight as f32).unwrap_or(0.0);
    let gt = g.map(|g| g.time as f32).unwrap_or(0.0);
    let gk = g.map(|g| g.kind);
    let shrug_raise = if gk == Some(Gesture::Shrug) { 0.02 * hh * gw } else { 0.0 };

    let tw = torso_w0 * (0.55 + 0.45 * af);
    let sh = hip_y + torso_h + shrug_raise + breath;

    // ---------------------------------------------------------------- legs
    let rl = 0.038 * hh;
    let hx = torso_w0 * 0.17 * (0.3 + 0.7 * af);
    for i in [-1.0f32, 1.0] {
        let phase_i = phase + if i > 0.0 { PI } else { 0.0 };
        let sw = phase_i.sin() * amp;
        let lift = 0.045 * hh * amp * phase_i.cos().max(0.0);
        let phi = sw * 0.5 * (prof + 0.25);
        let fx_w = hx * i + dir * leg * phi.sin();
        let fy_w = hip0 - leg * phi.cos() + lift;
        let fx_s = hx * i + dir * 0.05 * hh * prof;
        let fy_s = hip0 - leg * 0.92;
        let (fx, fy) = (lerp(fx_w, fx_s, sit), lerp(fy_w, fy_s, sit));
        p.capsule((hx * i, hip_y), (fx, fy + 0.03 * hh), rl, bottom_c);
        p.ellipse(fx + dir * 0.014 * hh * prof, fy + 0.02 * hh, 0.054 * hh, 0.027 * hh, shoes_c);
    }

    // ---------------------------------------------------------------- upper body
    let waddle = (phase.sin() * amp * 3.5) as f32;
    p.tf = tf.pre_rotate_at(waddle, 0.0, hip_y);

    // Arm poses
    let sh_y = sh - if round { 0.06 * hh } else { 0.035 * hh };
    let sh_x = if round { tw * 0.42 } else { tw * 0.5 * 0.97 };
    let ul = arm_len * 0.5;
    let fl = arm_len * 0.5;
    let ra = 0.028 * hh;
    let rh = 0.036 * hh;
    let mut shoulders = [(-sh_x, sh_y), (sh_x, sh_y)];
    let _ = &mut shoulders;
    let mut arms: Vec<ArmPose> = Vec::new();
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
        arms.push(ArmPose { elbow: e, hand: h });
    }
    let by_angles = |sp: P2, i: f32, ths: f32, the: f32| -> ArmPose {
        let e = (sp.0 + ul * i * ths.sin(), sp.1 - ul * ths.cos());
        let a2 = ths + the;
        let h = (e.0 + fl * i * a2.sin(), e.1 - fl * a2.cos());
        ArmPose { elbow: e, hand: h }
    };
    let head_turn_dip = 0.0f32;
    let _ = head_turn_dip;

    // Head position (needed by some gestures).
    let nod = if gk == Some(Gesture::Nod) { (TAU * 2.2 * gt).sin() * 0.03 * hh * gw } else { 0.0 };
    let speak_bob = if fr.speaking { 0.006 * hh * fr.viseme.open } else { 0.0 };
    let mut hcy = sh + neck + head_r * 0.95 + nod + speak_bob - 0.012 * hh * if gk == Some(Gesture::Shrug) { gw } else { 0.0 };
    let mut hcx = 0.0f32;
    if gk == Some(Gesture::FacePalm) {
        hcy -= 0.025 * hh * gw;
    }
    hcx += dir * 0.0;

    if let Some(kind) = gk {
        match kind {
            Gesture::Wave => {
                let sp = shoulders[1];
                let target = by_angles(sp, 1.0, 2.45, -0.25 + 0.45 * (TAU * 2.8 * gt).sin());
                arms[1] = ArmPose { elbow: lp(arms[1].elbow, target.elbow, gw), hand: lp(arms[1].hand, target.hand, gw) };
            }
            Gesture::Point => {
                let idx = if v.target_side >= 0.0 { 1 } else { 0 };
                let i = if idx == 1 { 1.0 } else { -1.0 };
                let up = (v.target_up as f32).clamp(-0.5, 0.6);
                let target = by_angles(shoulders[idx], i, 1.45 + up, 0.0);
                arms[idx] =
                    ArmPose { elbow: lp(arms[idx].elbow, target.elbow, gw), hand: lp(arms[idx].hand, target.hand, gw) };
            }
            Gesture::Shrug => {
                for idx in 0..2 {
                    let i = if idx == 1 { 1.0 } else { -1.0 };
                    let sp = (shoulders[idx].0, shoulders[idx].1 + shrug_raise);
                    let target = by_angles(sp, i, 0.45, 1.25);
                    arms[idx] =
                        ArmPose { elbow: lp(arms[idx].elbow, target.elbow, gw), hand: lp(arms[idx].hand, target.hand, gw) };
                }
            }
            Gesture::CrossArms => {
                let y = sh_y - 0.1 * hh;
                let l = ArmPose { elbow: (-sh_x * 0.95, y - 0.04 * hh), hand: (sh_x * 0.55, y) };
                let r = ArmPose { elbow: (sh_x * 0.95, y - 0.04 * hh), hand: (-sh_x * 0.55, y - 0.035 * hh) };
                arms[0] = ArmPose { elbow: lp(arms[0].elbow, l.elbow, gw), hand: lp(arms[0].hand, l.hand, gw) };
                arms[1] = ArmPose { elbow: lp(arms[1].elbow, r.elbow, gw), hand: lp(arms[1].hand, r.hand, gw) };
            }
            Gesture::FacePalm => {
                let target = ArmPose {
                    elbow: (shoulders[1].0 + 0.06 * hh, sh_y - 0.05 * hh),
                    hand: (0.12 * head_r, hcy - 0.1 * head_r),
                };
                arms[1] = ArmPose { elbow: lp(arms[1].elbow, target.elbow, gw), hand: lp(arms[1].hand, target.hand, gw) };
            }
            _ => {}
        }
    }

    let draw_arm = |p: &mut Painter, sp: P2, a: &ArmPose| {
        p.capsule(sp, a.elbow, ra, top_c);
        p.capsule(a.elbow, a.hand, ra * 0.92, top_c);
        p.circle(a.hand.0, a.hand.1, rh, hand_c);
    };
    let far_first = prof > 0.55;
    let far_idx = if dir > 0.0 { 0 } else { 1 };
    if far_first {
        draw_arm(&mut p, shoulders[far_idx], &arms[far_idx]);
    }

    // Torso
    let tw_top = tw * 0.5 * 0.92;
    let tw_bot = tw * 0.5 * 1.1;
    if round {
        p.ellipse(0.0, hip_y + torso_h * 0.5, tw * 0.5, torso_h * 0.52, top_c);
    } else {
        let pts = [(-tw_top, sh), (tw_top, sh), (tw_bot, hip_y - 0.012 * hh), (-tw_bot, hip_y - 0.012 * hh)];
        p.smooth(&pts, top_c);
    }
    if let Some(item) = top {
        match item.name {
            "apron_white" => {
                let pts = [(-tw * 0.36, sh - 0.04 * hh), (tw * 0.36, sh - 0.04 * hh), (tw * 0.4, hip_y), (-tw * 0.4, hip_y)];
                p.smooth(&pts, Rgb(255, 255, 252));
            }
            _ => {
                if front > 0.35 {
                    // collar + zipper
                    p.line((-tw_top * 0.55, sh - 0.035 * hh), (tw_top * 0.55, sh - 0.035 * hh), 0.022 * hh, top_a);
                    p.line((0.0, sh - 0.03 * hh), (0.0, hip_y + 0.02 * hh), 0.008 * hh, top_a);
                }
            }
        }
    }
    if let Some(a) = acc {
        p.ellipse(0.0, sh + 0.004 * hh, tw * 0.5, 0.032 * hh, a.color);
    }

    if !far_first {
        draw_arm(&mut p, shoulders[0], &arms[0]);
        draw_arm(&mut p, shoulders[1], &arms[1]);
    } else {
        let near = 1 - far_idx;
        draw_arm(&mut p, shoulders[near], &arms[near]);
    }

    // ---------------------------------------------------------------- head
    let r = head_r;
    let hfront = v.h_front as f32;
    let haf = hfront.abs();
    p.capsule((0.0, sh - 0.02 * hh), (0.0, sh + neck + 0.05 * hh), 0.05 * hh, skin);
    // head tilt for facepalm / shake
    let tilt = if gk == Some(Gesture::FacePalm) { 8.0 * gw } else { 0.0 };
    p.tf = p.tf.pre_rotate_at(tilt, 0.0, sh);
    match def.head.kind.as_str() {
        "square" => {
            let w = r * 0.97 * (0.82 + 0.18 * haf);
            let pts = [(hcx - w, hcy + r * 0.92), (hcx + w, hcy + r * 0.92), (hcx + w, hcy - r * 0.92), (hcx - w, hcy - r * 0.92)];
            p.smooth(&pts, skin);
        }
        "oval" => {
            p.ellipse(hcx, hcy + r * 0.08, r * 0.86 * (0.85 + 0.15 * haf), r * 1.08, skin);
        }
        _ => p.circle(hcx, hcy, r, skin),
    }

    let hair_c = def.head.color("hair", Rgb(92, 60, 38));
    let bald = def.head.num("bald", 0.0) > 0.5;
    let back = hfront < -0.05;
    if back && !bald {
        p.circle(hcx, hcy, r * 1.005, hair_c);
    } else if hat.is_none() && !bald {
        let mut pts: Vec<P2> = Vec::new();
        for k in 0..=12 {
            let a = (8.0 + 164.0 * k as f32 / 12.0) * PI / 180.0;
            pts.push((hcx + r * 1.01 * a.cos(), hcy + r * 1.01 * a.sin()));
        }
        pts.extend_from_slice(&[
            (hcx - r * 0.78, hcy + r * 0.42),
            (hcx - r * 0.3, hcy + r * 0.52),
            (hcx + r * 0.3, hcy + r * 0.46),
            (hcx + r * 0.78, hcy + r * 0.40),
        ]);
        p.poly(&pts, hair_c);
    }

    if !back {
        draw_face(&mut p, def, fr, v, hcx, hcy, r, ow0);
    } else if hat.is_none() && bald {
        // nothing: plain back of the head
    }

    // Hat
    if let Some(h) = hat {
        draw_hat(&mut p, h, hcx, hcy, r);
    }
}

/// A hat sitting on a head of radius `r` centered at (hcx, hcy). Shared by every style.
pub(crate) fn draw_hat(p: &mut Painter, h: &WardrobeItem, hcx: f32, hcy: f32, r: f32) {
    let yb = hcy + r * 0.46;
    let rr = r * 1.06;
    let dy = yb - hcy;
    let a0 = (dy / rr).clamp(-1.0, 1.0).asin();
    let mut dome: Vec<P2> = Vec::new();
    for k in 0..=16 {
        let a = a0 + (PI - 2.0 * a0) * k as f32 / 16.0;
        dome.push((hcx + rr * a.cos(), hcy + rr * a.sin()));
    }
    let half_w = (rr * rr - dy * dy).max(0.0).sqrt();
    match h.name {
        "chef_hat" => {
            p.poly(&[(hcx - r * 0.9, yb - r * 0.1), (hcx + r * 0.9, yb - r * 0.1), (hcx + r * 0.85, yb + r * 0.3), (hcx - r * 0.85, yb + r * 0.3)], h.color);
            for (cx, cy, rad) in [(-0.5, 0.75, 0.55), (0.5, 0.75, 0.55), (0.0, 1.0, 0.6)] {
                p.circle(hcx + r * cx, yb + r * cy, r * rad, h.color);
            }
            p.poly(&[(hcx - r * 0.86, yb - r * 0.02), (hcx + r * 0.86, yb - r * 0.02), (hcx + r * 0.85, yb + r * 0.3), (hcx - r * 0.85, yb + r * 0.3)], h.color);
        }
        "green_hat" => {
            p.poly(&dome, h.color);
            p.ellipse(hcx - rr * 0.97, hcy - r * 0.05, r * 0.2, r * 0.5, h.accent);
            p.ellipse(hcx + rr * 0.97, hcy - r * 0.05, r * 0.2, r * 0.5, h.accent);
            p.poly(&[(hcx - half_w, yb), (hcx + half_w, yb), (hcx + half_w * 0.98, yb - r * 0.16), (hcx - half_w * 0.98, yb - r * 0.16)], h.accent);
        }
        "yellow_hat" => {
            p.poly(&dome, h.color);
            p.ellipse(hcx, yb - r * 0.02, half_w * 1.12, r * 0.12, h.accent);
        }
        _ => {
            // beanie
            p.poly(&dome, h.color);
            p.poly(&[(hcx - half_w * 1.03, yb + r * 0.02), (hcx + half_w * 1.03, yb + r * 0.02), (hcx + half_w * 1.0, yb - r * 0.17), (hcx - half_w * 1.0, yb - r * 0.17)], h.color.lerp(Rgb(255, 255, 255), 0.0));
            p.line((hcx - half_w, yb - r * 0.08), (hcx + half_w, yb - r * 0.08), r * 0.08, h.accent.lerp(h.color, 0.4));
            p.circle(hcx, hcy + rr + r * 0.06, r * 0.2, h.accent);
        }
    }
}

// ---------------------------------------------------------------------- face

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_face(p: &mut Painter, def: &CharacterDef, fr: &ActorFrame, v: &View, hcx: f32, hcy: f32, r: f32, ow0: f32) {
    let hfront = v.h_front as f32;
    let hside = v.h_side as f32;
    let haf = hfront.abs().max(0.0);
    let ex = &fr.expr;
    let shift = hside * 0.62 * r;
    let cx = hcx + shift;
    let squeeze = 0.35 + 0.65 * haf;
    let spacing = def.eyes.num("spacing", 0.4) as f32;
    let eye_sep = spacing * r * 0.7 * squeeze;
    let eye_y = hcy + 0.05 * r;
    let blink = fr.blink as f32;
    let open = (ex.eye_open as f32 * (1.0 - 0.92 * blink)).max(0.06);

    let (rx0, ry0, whites) = match def.eyes.kind.as_str() {
        "ovals" => (0.2, 0.25, true),
        "wide" => (0.26, 0.33, true),
        _ => (0.075, 0.095, false),
    };
    let rx = rx0 * r * (0.55 + 0.45 * haf);
    let ry = ry0 * r * open;
    let gdx = v.eye_dx as f32;
    let gdy = v.eye_dy as f32;

    for k in [-1.0f32, 1.0] {
        let ecx = cx + k * eye_sep;
        if whites {
            p.ellipse(ecx, eye_y, rx, ry, Rgb(252, 252, 250));
            let pr = (0.075 * r).min(ry * 0.9);
            let px_ = ecx + (gdx * 0.5 + hside * 0.3) * rx;
            let py_ = eye_y + gdy * ry * 0.4;
            p.ellipse_nooutline(px_, py_, pr * (0.7 + 0.3 * haf), pr, Rgb(20, 18, 24));
        } else {
            let ox = (gdx * 0.5 + hside * 0.0) * rx0 * r;
            p.ellipse_nooutline(ecx + ox, eye_y + gdy * ry0 * r * 0.4, rx, ry, Rgb(20, 18, 24));
        }
        // eyebrow
        let by = eye_y + ry0 * r + 0.14 * r + ex.brow_y as f32 * 0.14 * r;
        let ang = (ex.brow_angle as f32).to_radians();
        let half = 0.19 * r * (0.5 + 0.5 * haf);
        // inner end is toward the face center: for k=-1 the inner end is on the right.
        let inner_sign = -k;
        let outer = (ecx - inner_sign * half, by + ang.sin() * half);
        let inner = (ecx + inner_sign * half, by - ang.sin() * half);
        p.line_black(outer, inner, 0.05 * r);
    }

    // Mouth
    let mcx = hcx + hside * 0.5 * r;
    let mcy = hcy - 0.42 * r;
    let mw_scale = 0.45 + 0.55 * haf;
    draw_mouth(p, def, fr, mcx, mcy, r, mw_scale, ow0);

    // Sweat drop
    if ex.sweat > 0.25 {
        let a = (ex.sweat as f32).min(1.0);
        let sx = hcx + r * 0.92;
        let sy = hcy + r * 0.55 - 0.05 * r * (1.0 + (fr.t as f32 * 3.0).sin());
        let mut pb = PathBuilder::new();
        pb.move_to(sx, sy + 0.2 * r);
        pb.quad_to(sx - 0.14 * r, sy - 0.02 * r, sx, sy - 0.2 * r);
        pb.quad_to(sx + 0.14 * r, sy - 0.02 * r, sx, sy + 0.2 * r);
        pb.close();
        if let Some(path) = pb.finish() {
            let col = Rgb(150, 205, 255).lerp(Rgb(255, 255, 255), (1.0 - a) as f64);
            p.shape(&path, col);
        }
    }
}

pub(crate) fn draw_mouth(p: &mut Painter, _def: &CharacterDef, fr: &ActorFrame, cx: f32, cy: f32, r: f32, wscale: f32, ow0: f32) {
    let ex = &fr.expr;
    let smile = ex.smile as f32;
    let speaking = fr.speaking;
    let mut open = if speaking { fr.viseme.open } else { 0.0 };
    open = (open + ex.mouth_bias as f32 * if speaking { 0.5 } else { 1.0 }).min(1.0);
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
    let dark = Rgb(58, 12, 22);
    let w = 0.36 * r * wscale;

    if shape == Shape::Rest || open < 0.07 {
        let corner_y = cy + smile * 0.10 * r;
        let ctrl_y = cy - smile * 0.20 * r;
        let mut pb = PathBuilder::new();
        pb.move_to(cx - w, corner_y);
        pb.quad_to(cx, ctrl_y, cx + w, corner_y);
        if let Some(path) = pb.finish() {
            p.stroke_black(&path, 0.05 * r);
        }
        return;
    }
    let (mw, mh) = match shape {
        Shape::A => (0.40, 0.10 + 0.40 * open),
        Shape::E => (0.50, 0.06 + 0.22 * open),
        Shape::O => (0.24, 0.08 + 0.34 * open),
        Shape::U => (0.17, 0.06 + 0.22 * open),
        Shape::S => (0.42, 0.07 + 0.08 * open),
        Shape::FV => (0.38, 0.08 + 0.08 * open),
        Shape::Rest => (0.3, 0.05),
    };
    let (mw, mh) = (mw * r * wscale.max(0.6), mh * r);
    let cy2 = cy + smile * 0.05 * r - mh * 0.15;
    p.ow = ow0 * 0.8;
    p.ellipse(cx, cy2, mw, mh, dark);
    if matches!(shape, Shape::A | Shape::O) && mh > 0.14 * r {
        p.ellipse_nooutline(cx, cy2 - mh * 0.45, mw * 0.55, mh * 0.34, Rgb(206, 86, 98));
    }
    if matches!(shape, Shape::E | Shape::S | Shape::FV) {
        p.ellipse_nooutline(cx, cy2 + mh * 0.5, mw * 0.82, mh * 0.34, Rgb(252, 252, 250));
    }
    p.ow = ow0;
}
