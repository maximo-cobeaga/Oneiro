//! Vector characters (tier B): a puppet made of four SVG parts (head, body, arm, leg) that are
//! placed on the skeleton, rotated at their pivots and flipped/compressed to turn. Oneiro draws the
//! face (eyes, brows, lipsynced mouth) on the head unless the SVG brings its own.

use crate::character::{draw_face, Placement, View};
use crate::draw::Painter;
use crate::pose::{self, Limb, P2};
use oneiro_core::defs::SvgPart;
use oneiro_core::raster::Raster;
use oneiro_core::svg::{Seg, SvgDoc};
use oneiro_core::{CharacterDef, Dims};
use oneiro_core::timeline::ActorFrame;
use tiny_skia::{FilterQuality, Path, PathBuilder, Pixmap, PixmapPaint, Transform};

/// The drawing of a part: vector (tier B) or a PNG cut from an illustration (tier C).
#[derive(Clone, Copy)]
enum Art<'a> {
    Svg(&'a SvgDoc),
    Raster(&'a Raster),
}

impl Art<'_> {
    fn aspect(&self) -> f32 {
        match self {
            Art::Svg(d) => d.aspect(),
            Art::Raster(r) => r.aspect(),
        }
    }
}

fn art_of(p: &SvgPart) -> Option<Art<'_>> {
    p.doc.as_deref().map(Art::Svg).or_else(|| p.raster.as_deref().map(Art::Raster))
}

/// Scales the colors of a premultiplied pixmap by the scene tint (night scenes are bluish).
fn tinted(src: &Pixmap, tint: [f64; 3]) -> Pixmap {
    let mut out = src.clone();
    for px in out.data_mut().chunks_mut(4) {
        for c in 0..3 {
            px[c] = (px[c] as f64 * tint[c]).clamp(0.0, 255.0) as u8;
        }
    }
    out
}

pub fn seg_path(segs: &[Seg]) -> Option<Path> {
    let mut pb = PathBuilder::new();
    for s in segs {
        match *s {
            Seg::Move(x, y) => pb.move_to(x, y),
            Seg::Line(x, y) => pb.line_to(x, y),
            Seg::Quad(a, b, x, y) => pb.quad_to(a, b, x, y),
            Seg::Cubic(a, b, c, d, x, y) => pb.cubic_to(a, b, c, d, x, y),
            Seg::Close => pb.close(),
        }
    }
    pb.finish()
}

/// Everything needed to put one part on the skeleton.
struct Place {
    anchor: P2,
    /// Rotation in radians (counter-clockwise, y up); 0 = the part hangs straight down / stands straight up.
    angle: f32,
    flip: f32,
    xs: f32,
    ys: f32,
}

fn draw_part(p: &mut Painter, base: Transform, art: Art, pivot: (f64, f64), height_m: f32, pl: &Place) {
    let doc = match art {
        Art::Svg(d) => d,
        Art::Raster(r) => {
            let (w, h) = (r.width() as f32, r.height() as f32);
            let k = height_m / h;
            let tf = base
                .pre_translate(pl.anchor.0, pl.anchor.1)
                .pre_rotate(pl.angle.to_degrees())
                .pre_scale(k * pl.flip * pl.xs, -k * pl.ys)
                .pre_translate(-(pivot.0 as f32 * w), -(pivot.1 as f32 * h));
            let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..Default::default() };
            if p.tint.iter().all(|t| (*t - 1.0).abs() < 1e-3) {
                p.pm.draw_pixmap(0, 0, r.pm.as_ref(), &paint, tf, None);
            } else {
                let t = tinted(&r.pm, p.tint);
                p.pm.draw_pixmap(0, 0, t.as_ref(), &paint, tf, None);
            }
            return;
        }
    };
    let [vx, vy, vw, vh] = doc.vb;
    let k = height_m / vh;
    let (px, py) = (vx + pivot.0 as f32 * vw, vy + pivot.1 as f32 * vh);
    p.tf = base
        .pre_translate(pl.anchor.0, pl.anchor.1)
        .pre_rotate(pl.angle.to_degrees())
        .pre_scale(k * pl.flip * pl.xs, -k * pl.ys)
        .pre_translate(-px, -py);
    for sh in &doc.shapes {
        let Some(path) = seg_path(&sh.segs) else { continue };
        if let Some(c) = sh.fill {
            p.fill_rgba(&path, c);
        }
        if let Some((c, w)) = sh.stroke {
            p.stroke_rgba(&path, c, w);
        }
    }
}

/// Where the drawing really is inside its viewBox, as fractions (x0, y0, x1, y1; y from the top). AI-made
/// parts often leave margins, which would leave gaps between neck, shoulders and arms.
fn art_bbox(art: Art) -> (f64, f64, f64, f64) {
    let Art::Svg(doc) = art else { return (0.0, 0.0, 1.0, 1.0) };
    let [vx, vy, vw, vh] = doc.vb;
    let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
    for sh in &doc.shapes {
        let half = sh.stroke.map(|(_, w)| w * 0.5).unwrap_or(0.0);
        for s in &sh.segs {
            let pts: &[(f32, f32)] = match *s {
                Seg::Move(x, y) | Seg::Line(x, y) => &[(x, y)],
                Seg::Quad(_, _, x, y) => &[(x, y)],
                Seg::Cubic(_, _, _, _, x, y) => &[(x, y)],
                Seg::Close => &[],
            };
            for &(x, y) in pts {
                x0 = x0.min(x - half);
                y0 = y0.min(y - half);
                x1 = x1.max(x + half);
                y1 = y1.max(y + half);
            }
        }
    }
    if x0 > x1 || vw <= 0.0 || vh <= 0.0 {
        return (0.0, 0.0, 1.0, 1.0);
    }
    let f = |v: f32, o: f32, l: f32| (((v - o) / l) as f64).clamp(0.0, 1.0);
    (f(x0, vx, vw), f(y0, vy, vh), f(x1, vx, vw), f(y1, vy, vh))
}

/// Angle that turns a part hanging down into the direction `from -> to`, and the distance.
fn aim(l: &Limb) -> (f32, f32) {
    let (dx, dy) = (l.end.0 - l.root.0, l.end.1 - l.root.1);
    (dx.atan2(-dy), (dx * dx + dy * dy).sqrt())
}

/// Proportions of a vector character, from its parts.
pub fn dims_of(def: &CharacterDef) -> Option<Dims> {
    let rig = def.svg.as_ref()?;
    let hh = def.height();
    let (head, body, arm, leg) = (rig.parts.get("head")?, rig.parts.get("body")?, rig.parts.get("arm")?, rig.parts.get("leg")?);
    let body_h = body.height * hh;
    // The torso is what is really drawn: from the hips up to the top of the drawing, and as wide as the drawing.
    let (bx0, by0, bx1, _) = art_bbox(art_of(body)?);
    let visible_h = ((body.pivot.1 - by0) * body_h).max(0.3 * body_h);
    Some(Dims { leg: leg.height * hh, torso_h: visible_h, torso_w: body_h * art_of(body)?.aspect() as f64 * (bx1 - bx0).max(0.3), head_r: head.height * hh * 0.5, arm_len: arm.height * hh, neck: 0.0 })
}

#[allow(clippy::too_many_arguments)]
pub fn draw_puppet(pm: &mut Pixmap, def: &CharacterDef, fr: &ActorFrame, v: &View, pl: &Placement, tint: [f64; 3], px: f32, outline: f32) {
    let (Some(rig), Some(dm)) = (def.svg.as_ref(), dims_of(def)) else { return };
    let (Some(head), Some(body), Some(arm), Some(leg)) = (rig.parts.get("head"), rig.parts.get("body"), rig.parts.get("arm"), rig.parts.get("leg")) else { return };
    let (Some(hd), Some(bd), Some(ad), Some(ld)) = (art_of(head), art_of(body), art_of(arm), art_of(leg)) else { return };
    let hh = def.height() as f32;
    let s = pl.scale;
    let base = Transform::from_translate(pl.sx, pl.sy).pre_scale(s, -s);
    let mut p = Painter { pm, tf: base, ow: 2.4 * px / s * outline, tint };
    let pose = pose::compute(hh, &dm, false, fr, v);
    let af = pose.front.abs();
    let turn = 0.6 + 0.4 * af;
    let dir = pose.dir;
    let (leg_h, body_h, head_h, arm_h) = (dm.leg as f32, dm.torso_h as f32, (dm.head_r * 2.0) as f32, dm.arm_len as f32);

    // Legs (under the body).
    for (k, l) in pose.legs.iter().enumerate() {
        let (angle, len) = aim(l);
        let ys = (len / (leg_h * 0.97)).clamp(0.6, 1.15);
        draw_part(&mut p, base, ld, leg.pivot, leg_h, &Place { anchor: l.root, angle, flip: pair_flip(ld, k, af, dir), xs: turn, ys });
    }

    // Upper body swings a little while walking.
    let upper = base.pre_rotate_at(pose.waddle, 0.0, pose.hip_y);
    let far_first = pose.prof > 0.55;
    let far_idx = if dir > 0.0 { 0 } else { 1 };
    let arm_at = |p: &mut Painter, idx: usize| {
        let a = &pose.arms[idx];
        let (angle, len) = aim(a);
        let ys = (len / (arm_h * 0.92)).clamp(0.55, 1.2);
        draw_part(p, upper, ad, arm.pivot, arm_h, &Place { anchor: a.root, angle, flip: pair_flip(ad, idx, af, dir), xs: 0.75 + 0.25 * af, ys });
    };
    if far_first {
        arm_at(&mut p, far_idx);
    }
    draw_part(&mut p, upper, bd, body.pivot, body_h, &Place { anchor: (0.0, pose.hip_y), angle: 0.0, flip: dir, xs: 0.55 + 0.45 * af, ys: 1.0 });
    if far_first {
        arm_at(&mut p, 1 - far_idx);
    } else {
        arm_at(&mut p, 0);
        arm_at(&mut p, 1);
    }

    // Head: turns toward its own gaze, bounces a little when it talks.
    let hfront = v.h_front as f32;
    let haf = hfront.abs();
    let hdir = if v.h_side < 0.0 { -1.0f32 } else { 1.0 };
    let head_tf = upper.pre_rotate_at(pose.tilt, 0.0, pose.sh);
    let hxs = 0.62 + 0.38 * haf;
    let hys = 1.0 + if fr.speaking { 0.02 * fr.viseme.open } else { 0.0 };
    // The head sits on the neck: lowered if its drawing leaves room under the chin, and a little into the shoulders.
    let (_, _, _, hy1) = art_bbox(hd);
    let drop = ((head.pivot.1 - hy1).max(0.0) as f32 + 0.04) * head_h;
    let hsh = pose.sh - drop;
    let place = Place { anchor: (0.0, hsh), angle: 0.0, flip: hdir, xs: hxs, ys: hys };
    draw_part(&mut p, head_tf, hd, head.pivot, head_h, &place);

    if rig.face && hfront > -0.05 {
        // Face center in head space (y up from the neck pivot).
        let head_w = head_h * hd.aspect();
        let fx = hdir * (rig.face_x as f32 - head.pivot.0 as f32) * head_w * hxs;
        let fy = (head.pivot.1 as f32 - rig.face_y as f32) * head_h * hys;
        // The face is a decal on the head: it is squashed exactly like the head art (so it never leaves the
        // silhouette) and slides only a little toward the side the head turns to.
        p.tf = head_tf.pre_translate(fx, 0.0).pre_scale(hxs, 1.0).pre_translate(-fx, 0.0);
        let r = head_h * 0.5 * rig.face_scale as f32 * 0.92;
        let decal = View { h_front: 1.0, h_side: v.h_side * 0.3, ..*v };
        draw_face(&mut p, def, fr, &decal, fx, hsh + fy, r, 2.4 * px / s * outline);
    }
}

/// Parts cut from an illustration (tier C) are drawn once, for the character's left side; the right
/// arm and leg are their mirror image while the character faces the camera. Vector parts are
/// symmetric by construction and simply follow the facing direction.
fn pair_flip(art: Art, side: usize, front: f32, dir: f32) -> f32 {
    match art {
        Art::Raster(_) if front > 0.5 => {
            if side == 0 {
                1.0
            } else {
                -1.0
            }
        }
        _ => dir,
    }
}
