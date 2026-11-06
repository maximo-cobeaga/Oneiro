//! CPU renderer for Oneiro episodes (the `cutout_2d` style). Everything is a pure function of
//! time, so frames can be rendered in parallel and are bit-for-bit reproducible.

pub mod character;
pub mod clay;
pub mod draw;
pub mod encode;
pub mod export;
pub mod hose;
pub mod pose;
pub mod preview;
pub mod puppet;
pub mod set;

use character::{draw_character_ex, Placement, View};
use draw::{Cam, Painter};
use oneiro_core::math::*;
use oneiro_core::timeline::ActorFrame;
use oneiro_core::{Episode, Scene};
use rayon::prelude::*;
use tiny_skia::{Color, Pixmap, Transform};

pub struct Renderer<'a> {
    pub ep: &'a Episode,
    pub width: u32,
    pub height: u32,
}

fn tint_of(tod: &str) -> [f64; 3] {
    match tod {
        "night" => [0.88, 0.9, 1.0],
        "dusk" => [1.0, 0.88, 0.8],
        _ => [1.0, 1.0, 1.0],
    }
}

enum Item {
    Piece { prop: usize, piece: usize },
    Actor(usize),
}

impl<'a> Renderer<'a> {
    pub fn new(ep: &'a Episode, width: u32, height: u32) -> Self {
        Renderer { ep, width, height }
    }

    pub fn render_frame(&self, frame: usize) -> Pixmap {
        self.render_at(frame as f64 / self.ep.fps as f64)
    }

    pub fn render_at(&self, t: f64) -> Pixmap {
        let Some((si, lt)) = self.ep.locate(t) else {
            let mut pm = Pixmap::new(self.width, self.height).expect("pixmap");
            pm.fill(Color::from_rgba8(20, 18, 26, 255));
            return pm;
        };
        let sc = &self.ep.scenes[si];
        let mut frames = sc.frames_at(lt);
        // Clay characters are animated "on twos" like stop-motion: they hold each pose for 1/12 s.
        if sc.actors.iter().any(|a| a.def.style == "clay") {
            let q = (lt * 12.0).floor() / 12.0;
            let held = sc.frames_at(q);
            for fr in frames.iter_mut() {
                if sc.actors[fr.idx].def.style == "clay" {
                    if let Some(h) = held.iter().find(|f| f.idx == fr.idx) {
                        *fr = h.clone();
                    }
                }
            }
        }
        self.render_scene(sc, sc.camera_at(lt), frames)
    }

    /// Renders one scene with an explicit camera and actor frames (used by the IDE for free
    /// cameras and live puppeteering).
    pub fn render_scene(&self, sc: &Scene, pose: oneiro_core::CameraPose, frames: Vec<ActorFrame>) -> Pixmap {
        let (w, h) = (self.width, self.height);
        let mut pm = Pixmap::new(w, h).expect("pixmap");
        pm.fill(Color::from_rgba8(20, 18, 26, 255));
        let cam = Cam::new(&pose, w, h);
        let tint = tint_of(&sc.time_of_day);
        let px = h as f32 / 1080.0;

        let mut p = Painter { pm: &mut pm, tf: Transform::identity(), ow: 2.2 * px, tint };
        set::draw_shell(&mut p, &cam, &sc.set, &sc.time_of_day, px);

        // Contact shadows on the floor.
        for fr in &frames {
            let a = &sc.actors[fr.idx];
            let hgt = a.def.height();
            let rr = hgt * 0.24;
            let poly: Vec<V3> = (0..20)
                .map(|i| {
                    let ang = i as f64 / 20.0 * std::f64::consts::TAU;
                    [fr.kin.pos[0] + ang.cos() * rr, 0.004, fr.kin.pos[1] + ang.sin() * rr * 0.9]
                })
                .collect();
            p.poly3_alpha(&cam, &poly, oneiro_core::Rgb(0, 0, 0), 70);
        }

        // Painter's algorithm over props and characters.
        let mut items: Vec<(f64, Item)> = Vec::new();
        for pc in set::prop_pieces(&sc.set, &cam) {
            items.push((pc.depth, Item::Piece { prop: pc.prop, piece: pc.piece }));
        }
        for fr in &frames {
            let c = cam.to_cam([fr.kin.pos[0], 0.0, fr.kin.pos[1]]);
            items.push((c[2] - 0.2, Item::Actor(fr.idx)));
        }
        items.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        for (_, item) in items {
            match item {
                Item::Piece { prop, piece } => {
                    p.tf = Transform::identity();
                    set::draw_piece(&mut p, &cam, &sc.set, prop, piece, px);
                }
                Item::Actor(i) => {
                    let fr = &frames[i];
                    self.draw_actor(&mut p, sc, &cam, fr, tint, px);
                }
            }
        }
        pm
    }

    fn draw_actor(&self, p: &mut Painter, sc: &Scene, cam: &Cam, fr: &ActorFrame, tint: [f64; 3], px: f32) {
        let def = &sc.actors[fr.idx].def;
        let kin = &fr.kin;
        let foot = cam.to_cam([kin.pos[0], 0.0, kin.pos[1]]);
        if foot[2] < 0.2 {
            return;
        }
        let (sx, sy) = cam.project_cam(foot);
        let scale = (cam.f / foot[2]) as f32;

        let to_cam = norm2([cam.pos[0] - kin.pos[0], cam.pos[2] - kin.pos[1]]);
        let right = norm2([cam.right[0], cam.right[2]]);
        let f = fwd(kin.yaw);
        let front = f[0] * to_cam[0] + f[1] * to_cam[1];
        let side = f[0] * right[0] + f[1] * right[1];

        // Head turn from gaze (blended between the previous and current target).
        let turn_to = |g: Option<V3>| -> f64 {
            match g {
                Some(pt) => {
                    angle_diff(yaw_to(kin.pos, [pt[0], pt[2]]), kin.yaw).clamp(-1.3, 1.3) * 0.85
                }
                None => 0.0,
            }
        };
        let mut head_turn = lerp(turn_to(fr.gaze_prev), turn_to(fr.gaze), fr.gaze_u);
        if let Some(g) = &fr.gesture {
            if g.kind == oneiro_core::catalog::Gesture::ShakeHead {
                head_turn += 0.55 * (std::f64::consts::TAU * 2.4 * g.time).sin() * g.weight;
            }
        }
        let hyaw = kin.yaw + head_turn;
        let hf = fwd(hyaw);
        let h_front = hf[0] * to_cam[0] + hf[1] * to_cam[1];
        let h_side = hf[0] * right[0] + hf[1] * right[1];

        // Eyes follow the gaze target within the head's residual turn.
        let (mut eye_dx, mut eye_dy) = (0.0, 0.0);
        if let Some(pt) = fr.gaze {
            let want = yaw_to(kin.pos, [pt[0], pt[2]]);
            let residual = angle_diff(want, hyaw).clamp(-0.9, 0.9);
            let vv = [fwd(hyaw + residual)[0] - hf[0], fwd(hyaw + residual)[1] - hf[1]];
            eye_dx = ((vv[0] * right[0] + vv[1] * right[1]) * 1.4).clamp(-0.6, 0.6);
            let head_y = def.height() * 0.78;
            let dist = dist2(kin.pos, [pt[0], pt[2]]).max(0.5);
            eye_dy = ((pt[1] - head_y) / dist).clamp(-0.5, 0.5);
        }

        let (mut target_side, mut target_up) = (1.0, 0.0);
        if let Some(g) = &fr.gesture {
            if let Some(t) = g.target {
                if let Some((tx, _, _)) = cam.project(t) {
                    target_side = if tx >= sx { 1.0 } else { -1.0 };
                }
                target_up = ((t[1] - def.height() * 0.6) / dist2(kin.pos, [t[0], t[2]]).max(0.5)).clamp(-0.5, 0.6);
            }
        }

        let view = View { front, side, h_front, h_side, eye_dx, eye_dy, target_side, target_up };
        let pl = Placement { sx, sy, scale };
        let phys = sc.actors[fr.idx].physics.as_deref();
        draw_styled(&mut *p.pm, def, fr, &view, &pl, tint, px, cam, phys);
    }
}

/// Draws one character in its own style. `cutout_2d` and `rubber_hose` draw straight onto the frame; `clay`
/// draws the character (whichever base drawing it has) into a layer and shades it like plasticine.
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
pub fn draw_styled(pm: &mut Pixmap, def: &oneiro_core::CharacterDef, fr: &ActorFrame, view: &View, pl: &Placement, tint: [f64; 3], px: f32, cam: &Cam, phys: Option<&oneiro_core::physics::Physics>) {
    if def.style == "toon_3d" && !def.has_svg() {
        draw_toon3d(pm, def, fr, pl, tint, px, cam, phys);
        return;
    }
    let base = |pm: &mut Pixmap, pl: &Placement, outline: f32| {
        if def.has_svg() {
            puppet::draw_puppet(pm, def, fr, view, pl, tint, px, outline);
        } else if def.style == "rubber_hose" {
            hose::draw_hose(pm, def, fr, view, pl, tint, px, outline);
        } else {
            draw_character_ex(pm, def, fr, view, pl, tint, px, outline);
        }
    };
    if def.style != "clay" {
        base(pm, pl, 1.0);
        return;
    }
    let hh = def.height() as f32;
    let size_px = hh * pl.scale;
    let seed = hash_str(&def.name);
    let frame12 = (fr.t * 12.0).floor() as i64;
    let (jx, jy) = clay::jitter(frame12, seed, size_px);
    // A layer around the character (generous: raised arms, a hat, a wave).
    let seat = (fr.kin.sit * fr.kin.seat_h) as f32 * pl.scale; // sitting on a stool lifts the whole body
    let (hw, up, down) = (0.8 * size_px, 1.35 * size_px + seat, 0.12 * size_px);
    let x0 = ((pl.sx - hw).floor() as i64).max(0);
    let y0 = ((pl.sy - up).floor() as i64).max(0);
    let x1 = ((pl.sx + hw).ceil() as i64).min(pm.width() as i64);
    let y1 = ((pl.sy + down).ceil() as i64).min(pm.height() as i64);
    if x1 - x0 < 4 || y1 - y0 < 4 || size_px < 8.0 {
        return;
    }
    let Some(mut layer) = Pixmap::new((x1 - x0) as u32, (y1 - y0) as u32) else { return };
    let local = Placement { sx: pl.sx - x0 as f32 + jx, sy: pl.sy - y0 as f32 + jy, scale: pl.scale };
    base(&mut layer, &local, 0.0);
    clay::shade(&mut layer, size_px, (local.sx, local.sy), seed);
    pm.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
}

/// FNV-style hash of a frame, used to verify deterministic rendering.
pub fn hash_pixmap(pm: &Pixmap) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for chunk in pm.data().chunks(8) {
        let mut v = 0u64;
        for (i, b) in chunk.iter().enumerate() {
            v |= (*b as u64) << (i * 8);
        }
        h ^= v;
        h = h.wrapping_mul(0x100000001b3);
        h ^= h >> 29;
    }
    h
}

/// Renders frames `range` in parallel batches and hands them to `sink` in order.
pub fn render_range<F>(r: &Renderer, range: std::ops::Range<usize>, mut sink: F) -> Result<(), String>
where
    F: FnMut(usize, &Pixmap) -> Result<(), String>,
{
    let batch = (rayon::current_num_threads() * 2).max(8);
    let mut start = range.start;
    while start < range.end {
        let end = (start + batch).min(range.end);
        let frames: Vec<Pixmap> = (start..end).into_par_iter().map(|i| r.render_frame(i)).collect();
        for (k, f) in frames.iter().enumerate() {
            sink(start + k, f)?;
        }
        start = end;
    }
    Ok(())
}

/// A real 3D character: rendered into a layer with a camera identical to the 2D set's, then composited.
#[allow(clippy::too_many_arguments)]
fn draw_toon3d(pm: &mut Pixmap, def: &oneiro_core::CharacterDef, fr: &ActorFrame, pl: &Placement, tint: [f64; 3], px: f32, cam: &Cam, phys: Option<&oneiro_core::physics::Physics>) {
    let hh = def.height() as f32;
    let size_px = hh * pl.scale;
    let seat = (fr.kin.sit * fr.kin.seat_h) as f32 * pl.scale;
    // Generous layer: a cape streams behind and a raised arm goes above the head.
    let (hw, up, down) = (1.0 * size_px, 1.45 * size_px + seat, 0.15 * size_px);
    let x0 = ((pl.sx - hw).floor() as i64).max(0);
    let y0 = ((pl.sy - up).floor() as i64).max(0);
    let x1 = ((pl.sx + hw).ceil() as i64).min(pm.width() as i64);
    let y1 = ((pl.sy + down).ceil() as i64).min(pm.height() as i64);
    if x1 - x0 < 4 || y1 - y0 < 4 || size_px < 8.0 {
        return;
    }
    let local = oneiro_toon3d::Camera { pos: cam.pos, right: cam.right, up: cam.up, fwd: cam.fwd, f: cam.f, cx: cam.cx - x0 as f64, cy: cam.cy - y0 as f64 };
    let style = oneiro_toon3d::Style { tint: [tint[0] as f32, tint[1] as f32, tint[2] as f32], outline_px: 2.4 * px };
    let layer = oneiro_toon3d::render_layer(def, fr, phys, &local, (x1 - x0) as u32, (y1 - y0) as u32, &style, oneiro_toon3d::Backend::Auto);
    pm.draw_pixmap(x0 as i32, y0 as i32, layer.as_ref(), &tiny_skia::PixmapPaint::default(), Transform::identity(), None);
}
