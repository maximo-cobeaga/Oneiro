//! The `rubber_hose` style: 1930s cartoons. Noodle limbs without joints, pie-cut eyes, white
//! gloves, a bouncing body and the constant 12 fps "boil" of hand-inked animation. It uses the same
//! character definition (colors, wardrobe, proportions) and skeleton as the other styles.

use crate::character::{draw_hat, draw_mouth, Placement, View};
use crate::draw::Painter;
use crate::pose::{self, Limb, P2};
use oneiro_core::catalog::*;
use oneiro_core::timeline::ActorFrame;
use oneiro_core::{CharacterDef, Rgb};
use std::f32::consts::{PI, TAU};
use tiny_skia::{PathBuilder, Pixmap, Transform};

/// An outlined noodle: a curve from `a` through the bend `mid` to `b`.
fn tube(p: &mut Painter, limb: &Limb, wobble: P2, r: f32, color: Rgb) {
    // Quadratic control point chosen so that the curve passes through `mid`.
    let ctrl = (2.0 * limb.mid.0 - 0.5 * (limb.root.0 + limb.end.0) + wobble.0, 2.0 * limb.mid.1 - 0.5 * (limb.root.1 + limb.end.1) + wobble.1);
    let mut pb = PathBuilder::new();
    pb.move_to(limb.root.0, limb.root.1);
    pb.quad_to(ctrl.0, ctrl.1, limb.end.0, limb.end.1);
    if let Some(path) = pb.finish() {
        let ow = p.ow;
        if ow > 0.0 {
            p.stroke_black(&path, r * 2.0 + ow * 2.0);
        }
        p.stroke(&path, color, r * 2.0);
    }
}

fn perp(a: P2, b: P2, mag: f32) -> P2 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let l = (dx * dx + dy * dy).sqrt().max(1e-5);
    (-dy / l * mag, dx / l * mag)
}

#[allow(clippy::too_many_arguments)]
pub fn draw_hose(pm: &mut Pixmap, def: &CharacterDef, fr: &ActorFrame, v: &View, pl: &Placement, tint: [f64; 3], px: f32, outline: f32) {
    let s = pl.scale;
    let base = Transform::from_translate(pl.sx, pl.sy).pre_scale(s, -s);
    let mut p = Painter { pm, tf: base, ow: 3.9 * px / s * outline, tint };
    let hh = def.height() as f32;
    let dm = def.dims();
    let pose = pose::compute(hh, &dm, def.body.kind == "round", fr, v);

    // Hand-inked animation holds each drawing for 2 frames: wobble and bounce run at 12 fps.
    let t12 = ((fr.t * 12.0).floor() / 12.0) as f32;
    let beat = (TAU * 2.0 * t12).sin();
    let busy = pose.walk_amp.max(if fr.speaking { 0.6 } else { 0.0 });
    let squash = 0.016 * beat * (0.45 + busy) + if fr.speaking { 0.012 * fr.viseme.open } else { 0.0 };
    p.tf = base.pre_scale(1.0 - squash, 1.0 + squash);

    // Colors: the character's own, nudged toward cream like old film stock.
    let cream = Rgb(255, 246, 222);
    let (mut top, mut bottom, mut hat) = (None, None, None);
    for n in &def.wardrobe {
        if let Some(it) = wardrobe_item(n) {
            match it.slot {
                Slot::Top => top = Some(it),
                Slot::Bottom => bottom = Some(it),
                Slot::Hat => hat = Some(it),
                _ => {}
            }
        }
    }
    let skin = def.skin().lerp(cream, 0.35);
    let top_c = top.map(|i| i.color).unwrap_or(Rgb(130, 130, 138)).lerp(cream, 0.12);
    let bottom_c = bottom.map(|i| i.color).unwrap_or(Rgb(60, 60, 80)).lerp(cream, 0.08);
    let shoe_c = Rgb(24, 22, 26);
    let glove = Rgb(255, 255, 255);
    let ink = Rgb(20, 18, 22);

    let (rl, ra, rh) = (0.034 * hh, 0.027 * hh, 0.044 * hh);
    let wob = |k: f32, mag: f32| (t12 * 17.0 + k * 2.3).sin() * mag;

    // ---------------------------------------------------------------- legs
    for (k, leg) in pose.legs.iter().enumerate() {
        let walk_wave = (pose.walk_phase * 2.0 + k as f32 * PI).sin() * 0.05 * hh * pose.walk_amp;
        let w = perp(leg.root, leg.end, walk_wave + wob(k as f32, 0.006 * hh));
        tube(&mut p, leg, w, rl, bottom_c);
        // Big round shoes with a shine.
        let (fx, fy) = leg.end;
        let toe = pose.dir * 0.03 * hh * (0.4 + pose.prof);
        p.ellipse(fx + toe, fy - 0.012 * hh, 0.082 * hh, 0.04 * hh, shoe_c);
        p.ellipse_nooutline(fx + toe - 0.02 * hh, fy + 0.002 * hh, 0.03 * hh, 0.011 * hh, Rgb(210, 210, 215));
    }

    // ---------------------------------------------------------------- upper body
    p.tf = p.tf.pre_rotate_at(pose.waddle, 0.0, pose.hip_y);
    let far_first = pose.prof > 0.55;
    let far_idx = if pose.dir > 0.0 { 0 } else { 1 };
    let draw_arm = |p: &mut Painter, idx: usize| {
        let arm = &pose.arms[idx];
        let w = perp(arm.root, arm.end, wob(idx as f32 + 4.0, 0.012 * hh) + (pose.walk_phase + idx as f32 * PI).sin() * 0.025 * hh * pose.walk_amp);
        tube(p, arm, w, ra, top_c);
        p.circle(arm.end.0, arm.end.1, rh, glove);
        // two knuckle lines, the classic glove detail
        p.line_black((arm.end.0 - rh * 0.35, arm.end.1 + rh * 0.55), (arm.end.0 - rh * 0.25, arm.end.1 + rh * 0.15), rh * 0.12);
        p.line_black((arm.end.0 + rh * 0.1, arm.end.1 + rh * 0.6), (arm.end.0 + rh * 0.12, arm.end.1 + rh * 0.2), rh * 0.12);
    };
    if far_first {
        draw_arm(&mut p, far_idx);
    }
    // Pear-shaped body
    let torso_h = dm.torso_h as f32;
    p.ellipse(0.0, pose.hip_y + torso_h * 0.5, pose.tw * 0.54, torso_h * 0.6, top_c);
    if pose.front > 0.35 {
        for k in 0..2 {
            p.circle(0.0, pose.hip_y + torso_h * (0.62 - 0.26 * k as f32), 0.012 * hh, glove);
        }
    }
    if far_first {
        draw_arm(&mut p, 1 - far_idx);
    } else {
        draw_arm(&mut p, 0);
        draw_arm(&mut p, 1);
    }

    // ---------------------------------------------------------------- head
    let r = dm.head_r as f32 * 1.06;
    let (hcx, hcy) = (pose.head.0 + wob(9.0, 0.004 * hh), pose.head.1);
    p.tf = p.tf.pre_rotate_at(pose.tilt, 0.0, pose.sh);
    p.capsule((0.0, pose.sh - 0.01 * hh), (0.0, pose.sh + 0.05 * hh), 0.045 * hh, skin);
    let hfront = v.h_front as f32;
    let hside = v.h_side as f32;
    let haf = hfront.abs();
    let back = hfront < -0.05;
    let hair = Rgb(24, 22, 28);
    let bald = def.head.num("bald", 0.0) > 0.5;

    // Round ears, behind the head.
    if !back {
        let ex = r * (0.98 + 0.0) * (0.35 + 0.65 * haf);
        for k in [-1.0f32, 1.0] {
            if haf > 0.3 || k * hside > 0.0 {
                p.circle(hcx + k * ex, hcy + 0.05 * r, r * 0.3, skin);
            }
        }
    }
    p.ellipse(hcx, hcy, r * (0.9 + 0.1 * haf), r * 0.98, skin);
    if back && !bald {
        p.ellipse(hcx, hcy, r * 0.9, r * 0.98, hair);
    } else if !back && !bald && hat.is_none() {
        // slicked-back black hair cap
        let mut pts: Vec<P2> = Vec::new();
        for k in 0..=12 {
            let a = (12.0 + 156.0 * k as f32 / 12.0) * PI / 180.0;
            pts.push((hcx + r * 0.93 * a.cos(), hcy + r * 0.99 * a.sin()));
        }
        pts.extend_from_slice(&[(hcx - r * 0.7, hcy + r * 0.52), (hcx, hcy + r * 0.66), (hcx + r * 0.7, hcy + r * 0.5)]);
        p.poly(&pts, hair);
    }

    if !back {
        let ex = &fr.expr;
        let cx = hcx + hside * 0.6 * r;
        let squeeze = 0.35 + 0.65 * haf;
        let eye_sep = r * 0.36 * squeeze;
        let eye_y = hcy + 0.12 * r;
        let blink = fr.blink as f32;
        let open = (ex.eye_open as f32 * (1.0 - 0.9 * blink)).max(0.08);
        let (rx, ry) = (0.17 * r * (0.6 + 0.4 * haf), 0.27 * r * open);
        let gdx = (v.eye_dx as f32).clamp(-0.6, 0.6);
        for k in [-1.0f32, 1.0] {
            let ecx = cx + k * eye_sep + gdx * rx * 0.25;
            p.ellipse_nooutline(ecx, eye_y, rx, ry, ink);
            // The pie cut: a white wedge toward the nose.
            let sgn = -k;
            p.fill_poly(&[(ecx, eye_y - ry * 0.1), (ecx + sgn * rx * 0.8, eye_y + ry * 0.45), (ecx + sgn * rx * 0.1, eye_y + ry * 0.92)], glove);
            let by = eye_y + 0.32 * r + ex.brow_y as f32 * 0.1 * r;
            let ang = (ex.brow_angle as f32).to_radians();
            let half = 0.2 * r * (0.5 + 0.5 * haf);
            let inner = -k;
            p.line_black((ecx - inner * half, by + ang.sin() * half), (ecx + inner * half, by - ang.sin() * half), 0.075 * r);
        }
        // Round black nose
        p.ellipse_nooutline(cx + hside * 0.05 * r, hcy - 0.1 * r, 0.14 * r * (0.7 + 0.3 * haf), 0.1 * r, ink);
        let ow0 = p.ow;
        draw_mouth(&mut p, def, fr, hcx + hside * 0.5 * r, hcy - 0.46 * r, r, 0.45 + 0.55 * haf, ow0);
    }
    if let Some(h) = hat {
        draw_hat(&mut p, h, hcx, hcy, r * 0.97);
    }
}
