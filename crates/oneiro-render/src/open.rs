//! Open sets (`kind: open`): a sky gradient, the ground, painted backdrops standing at the four edges of the
//! world (drawn as perspective-correct strips, so they look right from any side) and sprites that face the
//! camera and can sway, bob, breathe or spin.

use crate::draw::{Cam, Painter};
use crate::puppet::tinted;
use oneiro_core::math::*;
use oneiro_core::scenery::Sprite;
use oneiro_core::{Rgb, SetDef};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tiny_skia::{FilterQuality, GradientStop, LinearGradient, Paint, Pixmap, PixmapPaint, Point, Rect, SpreadMode, Transform};

/// Distance from the center of the world to its backdrops.
pub fn backdrop_radius(set: &SetDef) -> f64 {
    set.width.max(set.depth) / 2.0 + 8.0
}

fn tint_key(t: [f64; 3]) -> [u32; 3] {
    [(t[0] * 1000.0) as u32, (t[1] * 1000.0) as u32, (t[2] * 1000.0) as u32]
}

/// A tinted copy of a picture (night scenes are bluish), cached per picture and tint.
fn tinted_cached(src: &Arc<oneiro_core::raster::Raster>, tint: [f64; 3]) -> Arc<Pixmap> {
    static CACHE: OnceLock<Mutex<HashMap<(usize, [u32; 3]), Arc<Pixmap>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (Arc::as_ptr(src) as usize, tint_key(tint));
    if let Some(p) = cache.lock().unwrap().get(&key) {
        return p.clone();
    }
    let made = Arc::new(tinted(&src.pm, tint));
    let mut c = cache.lock().unwrap();
    if c.len() > 64 {
        c.clear();
    }
    c.insert(key, made.clone());
    made
}

fn horizon_y(cam: &Cam, h: f32) -> f32 {
    let f = norm2([cam.fwd[0], cam.fwd[2]]);
    let far = [cam.pos[0] + f[0] * 1.0e5, cam.pos[1], cam.pos[2] + f[1] * 1.0e5];
    let c = cam.to_cam(far);
    if c[2] > 1.0 {
        cam.project_cam(c).1.clamp(-4.0 * h, 5.0 * h)
    } else if cam.fwd[1] > 0.0 {
        5.0 * h
    } else {
        -4.0 * h
    }
}

fn shade(p: &Painter, c: Rgb) -> tiny_skia::Color {
    p.color(c)
}

/// Sky, ground and backdrops (everything behind the props and the characters).
pub fn draw_open_shell(p: &mut Painter, cam: &Cam, set: &SetDef) {
    let (w, h) = (p.pm.width() as f32, p.pm.height() as f32);
    // Sky
    let hy = horizon_y(cam, h);
    let top = shade(p, set.sky.0);
    let low = shade(p, set.sky.1);
    let mut paint = Paint::default();
    paint.anti_alias = false;
    let y1 = hy.max(1.0).min(h * 1.5);
    if let Some(sh) = LinearGradient::new(Point::from_xy(0.0, 0.0), Point::from_xy(0.0, y1), vec![GradientStop::new(0.0, top), GradientStop::new(1.0, low)], SpreadMode::Pad, Transform::identity()) {
        paint.shader = sh;
        if let Some(r) = Rect::from_xywh(0.0, 0.0, w, h) {
            p.pm.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
    // Ground: a huge plane, with a light checker inside the play area so movement reads.
    let big = 900.0;
    p.poly3(cam, &[[-big, 0.0, -big], [big, 0.0, -big], [big, 0.0, big], [-big, 0.0, big]], set.ground, false);
    if cam.pos[1] > 0.0 {
        let tile = 2.0;
        let (nx, nz) = ((set.width / tile).ceil() as i32, (set.depth / tile).ceil() as i32);
        let alt = set.ground.scale(0.955);
        for i in 0..nx {
            for j in 0..nz {
                if (i + j) % 2 != 0 {
                    continue;
                }
                let (x0, z0) = (-set.width / 2.0 + i as f64 * tile, -set.depth / 2.0 + j as f64 * tile);
                p.poly3(cam, &[[x0, 0.0, z0], [x0 + tile, 0.0, z0], [x0 + tile, 0.0, z0 + tile], [x0, 0.0, z0 + tile]], alt, false);
            }
        }
    }
    // Backdrops
    let r = backdrop_radius(set);
    for side in ["north", "south", "east", "west"] {
        let Some(art) = set.backdrops.get(side).and_then(|a| a.raster.as_ref()) else { continue };
        let (tw, th) = (art.pm.width() as f64, art.pm.height() as f64);
        let (hgt, half) = (2.0 * r * th / tw, r);
        // Corners left-to-right as seen from inside the world.
        let (a, b) = match side {
            "north" => ([half, r], [-half, r]),
            "south" => ([-half, -r], [half, -r]),
            "east" => ([r, -half], [r, half]),
            _ => ([-r, half], [-r, -half]),
        };
        let tex = tinted_cached(art, p.tint);
        textured_quad(p.pm, cam, [a[0], 0.0, a[1]], [b[0], 0.0, b[1]], hgt, &tex);
    }
}
/// Draws `tex` on the vertical rectangle whose bottom edge goes from `bl` to `br` and which is `hgt` tall.
/// One screen column at a time: each column's ray is intersected with the plane, so the perspective is exact
/// and there are no seams between pieces.
fn textured_quad(pm: &mut Pixmap, cam: &Cam, bl: V3, br: V3, hgt: f64, tex: &Pixmap) {
    let (tw, th) = (tex.width() as f32, tex.height() as f32);
    let w = pm.width() as i32;
    // Intersection of the ray through screen column `x` (at the horizon row) with the plane; returns u in 0..1.
    let u_at = |x: f64| -> Option<(f64, f64)> {
        let k = (x - cam.cx) / cam.f;
        let dir = [cam.fwd[0] + cam.right[0] * k, cam.fwd[2] + cam.right[2] * k];
        let e = [br[0] - bl[0], br[2] - bl[2]];
        let denom = dir[0] * e[1] - dir[1] * e[0];
        if denom.abs() < 1e-9 {
            return None;
        }
        let d = [bl[0] - cam.pos[0], bl[2] - cam.pos[2]];
        let t = (d[0] * e[1] - d[1] * e[0]) / denom;
        let u = (d[0] * dir[1] - d[1] * dir[0]) / denom;
        if t <= 0.1 || !(0.0..=1.0).contains(&u) {
            return None;
        }
        Some((u, t))
    };
    for x in 0..w {
        let xc = x as f64 + 0.5;
        let (Some((u, _)), Some((u2, _))) = (u_at(xc), u_at(xc + 1.0)) else { continue };
        let at = |up: f64| -> V3 { [bl[0] + (br[0] - bl[0]) * u, hgt * up, bl[2] + (br[2] - bl[2]) * u] };
        let (cb, ct) = (cam.to_cam(at(0.0)), cam.to_cam(at(1.0)));
        if cb[2] < 0.5 || ct[2] < 0.5 {
            continue;
        }
        let (yb, yt) = (cam.project_cam(cb).1, cam.project_cam(ct).1);
        if yb - yt < 1.0 {
            continue;
        }
        let ds = (((u2 - u) as f32) * tw).abs().max(0.05);
        let sx = u as f32 * tw;
        let tf = Transform::from_row(1.0 / ds, 0.0, 0.0, (yb - yt) / th, x as f32 - sx / ds, yt);
        let mut paint = Paint::default();
        paint.anti_alias = false;
        paint.shader = tiny_skia::Pattern::new(tex.as_ref(), SpreadMode::Pad, FilterQuality::Bilinear, 1.0, tf);
        if let Some(r) = Rect::from_xywh(x as f32, yt.max(0.0), 1.0, (yb.min(pm.height() as f32) - yt.max(0.0)).max(0.0)) {
            pm.fill_rect(r, &paint, Transform::identity(), None);
        }
    }
}

fn phase_of(name: &str) -> f64 {
    (oneiro_core::math::hash_str(name) % 6283) as f64 / 1000.0
}

/// Camera-space depth of a sprite's base.
pub fn sprite_depth(cam: &Cam, sp: &Sprite) -> f64 {
    cam.to_cam([sp.at[0], sp.y, sp.at[1]])[2]
}

/// A sprite standing in the world, animated as a pure function of the time `t`.
pub fn draw_sprite(pm: &mut Pixmap, cam: &Cam, sp: &Sprite, tint: [f64; 3], t: f64) {
    let Some(art) = sp.art.raster.as_ref() else { return };
    let c = cam.to_cam([sp.at[0], sp.y, sp.at[1]]);
    if c[2] < 0.3 {
        return;
    }
    let (sx, sy) = cam.project_cam(c);
    let scale = (cam.f / c[2]) as f32;
    let ph = phase_of(&sp.name);
    let wave = (std::f64::consts::TAU * sp.speed * t * 0.5 + ph).sin();
    let (mut kx, mut ky, mut dy, mut rot) = (1.0f32, 1.0f32, 0.0f32, 0.0f32);
    match sp.anim.as_str() {
        "sway" => rot = (wave * 3.0 * sp.amp) as f32,
        "bob" => dy = (wave * 0.12 * sp.amp * sp.height) as f32 * scale,
        "pulse" => {
            kx = 1.0 + (wave * 0.04 * sp.amp) as f32;
            ky = 1.0 - (wave * 0.05 * sp.amp) as f32;
        }
        "spin" => kx = ((std::f64::consts::TAU * sp.speed * t * 0.25 + ph).cos().abs().max(0.12)) as f32,
        _ => {}
    }
    let (tw, th) = (art.pm.width() as f32, art.pm.height() as f32);
    let k = (sp.height as f32 * scale) / th;
    let tex = tinted_cached(art, tint);
    let flip = if sp.flip { -1.0 } else { 1.0 };
    let tf = Transform::from_translate(sx, sy - dy)
        .pre_rotate(rot)
        .pre_scale(k * kx * flip, k * ky)
        .pre_translate(-tw / 2.0, -th);
    let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..Default::default() };
    pm.draw_pixmap(0, 0, (*tex).as_ref(), &paint, tf, None);
}
