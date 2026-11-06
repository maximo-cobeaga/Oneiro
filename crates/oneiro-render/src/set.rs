//! Set rendering: a perspective room (floor, walls, ceiling), wall decorations and 3D props
//! built from simple boxes and prisms, all drawn flat with outlines for a cut-paper look.

use crate::draw::{Cam, Painter};
use oneiro_core::math::*;
use oneiro_core::{PropInst, Rgb, SetDef};

fn wall_pt(set: &SetDef, wall: &str, along: f64, y: f64, inset: f64) -> V3 {
    let (w, d) = (set.width, set.depth);
    match wall {
        "north" => [along, y, d / 2.0 - inset],
        "south" => [along, y, -d / 2.0 + inset],
        "east" => [w / 2.0 - inset, y, along],
        _ => [-w / 2.0 + inset, y, along],
    }
}

fn inward(wall: &str) -> V3 {
    match wall {
        "north" => [0.0, 0.0, -1.0],
        "south" => [0.0, 0.0, 1.0],
        "east" => [-1.0, 0.0, 0.0],
        _ => [1.0, 0.0, 0.0],
    }
}

fn wall_len(set: &SetDef, wall: &str) -> f64 {
    if wall == "north" || wall == "south" {
        set.width
    } else {
        set.depth
    }
}

fn wall_visible(set: &SetDef, cam: &Cam, wall: &str) -> bool {
    let origin = wall_pt(set, wall, 0.0, 0.0, 0.0);
    dot3(sub3(cam.pos, origin), inward(wall)) > 0.02
}

/// A rectangle on a wall, centered at (`along`, `y`).
fn wall_rect(set: &SetDef, wall: &str, along: f64, y: f64, w: f64, h: f64, inset: f64) -> [V3; 4] {
    [
        wall_pt(set, wall, along - w / 2.0, y - h / 2.0, inset),
        wall_pt(set, wall, along + w / 2.0, y - h / 2.0, inset),
        wall_pt(set, wall, along + w / 2.0, y + h / 2.0, inset),
        wall_pt(set, wall, along - w / 2.0, y + h / 2.0, inset),
    ]
}

pub fn draw_shell(p: &mut Painter, cam: &Cam, set: &SetDef, tod: &str, px: f32) {
    let (w, d, h) = (set.width, set.depth, set.height);
    p.ow = 2.2 * px;

    // Ceiling
    if cam.pos[1] < h {
        let quad = [[-w / 2.0, h, -d / 2.0], [w / 2.0, h, -d / 2.0], [w / 2.0, h, d / 2.0], [-w / 2.0, h, d / 2.0]];
        p.poly3(cam, &quad, set.wall.scale(0.93), false);
    }

    // Floor tiles
    if cam.pos[1] > 0.0 {
        let nx = w.round().max(1.0) as i32;
        let nz = d.round().max(1.0) as i32;
        let (tw, td) = (w / nx as f64, d / nz as f64);
        for i in 0..nx {
            for j in 0..nz {
                let x0 = -w / 2.0 + i as f64 * tw;
                let z0 = -d / 2.0 + j as f64 * td;
                let c = if (i + j) % 2 == 0 { set.floor } else { set.floor_alt };
                let quad = [[x0, 0.0, z0], [x0 + tw, 0.0, z0], [x0 + tw, 0.0, z0 + td], [x0, 0.0, z0 + td]];
                p.poly3(cam, &quad, c, false);
            }
        }
    }

    // Walls
    for wall in ["north", "south", "east", "west"] {
        if !wall_visible(set, cam, wall) {
            continue;
        }
        let len = wall_len(set, wall);
        let (a0, a1) = (-len / 2.0, len / 2.0);
        let quad = |ya: f64, yb: f64, xa: f64, xb: f64| {
            [
                wall_pt(set, wall, xa, ya, 0.0),
                wall_pt(set, wall, xb, ya, 0.0),
                wall_pt(set, wall, xb, yb, 0.0),
                wall_pt(set, wall, xa, yb, 0.0),
            ]
        };
        p.poly3(cam, &quad(0.0, h, a0, a1), set.wall, false);
        // Wallpaper stripes above the wainscot.
        let n = (len / 0.5).round() as i32;
        let sw = len / n as f64;
        for k in (1..n).step_by(2) {
            p.poly3(cam, &quad(1.05, h, a0 + k as f64 * sw, a0 + (k + 1) as f64 * sw), set.wall_alt, false);
        }
        // Wainscot with a light stripe, rail and baseboard.
        p.poly3(cam, &quad(0.0, 0.95, a0, a1), set.trim, false);
        p.poly3(cam, &quad(0.42, 0.52, a0, a1), Rgb(244, 240, 230), false);
        p.poly3(cam, &quad(0.95, 1.05, a0, a1), Rgb(246, 242, 232), true);
        p.poly3(cam, &quad(0.0, 0.1, a0, a1), set.trim.scale(0.6), false);
        // Outline the wall.
        let o = quad(0.0, h, a0, a1);
        let ow = p.ow * 1.2;
        for i in 0..4 {
            p.line3(cam, o[i], o[(i + 1) % 4], ow, Rgb(12, 10, 14));
        }
    }

    // Wall decorations
    for pr in set.props.iter().filter(|pr| pr.wall.is_some()) {
        let wall = pr.wall.as_deref().unwrap();
        if wall_visible(set, cam, wall) {
            draw_wall_prop(p, cam, set, pr, tod, px);
        }
    }
}

fn sky_color(tod: &str) -> Rgb {
    match tod {
        "night" => Rgb(14, 18, 50),
        "dusk" => Rgb(236, 142, 96),
        _ => Rgb(148, 206, 240),
    }
}

fn draw_wall_prop(p: &mut Painter, cam: &Cam, set: &SetDef, pr: &PropInst, tod: &str, px: f32) {
    let wall = pr.wall.as_deref().unwrap();
    let a = pr.along;
    match pr.kind.as_str() {
        "window" => {
            let (ww, hh) = (pr.w.unwrap_or(1.6), pr.h.unwrap_or(1.2));
            let y = if pr.y > 0.0 { pr.y } else { 1.55 };
            p.poly3(cam, &wall_rect(set, wall, a, y, ww, hh, 0.01), Rgb(112, 72, 42), true);
            let glass = wall_rect(set, wall, a, y, ww - 0.16, hh - 0.16, 0.015);
            p.poly3(cam, &glass, sky_color(tod), false);
            let pt = |u: f64, v: f64| wall_pt(set, wall, a + u * (ww - 0.16) / 2.0, y + v * (hh - 0.16) / 2.0, 0.02);
            if tod == "night" {
                // moon + stars
                let moon: Vec<V3> = (0..24)
                    .map(|i| {
                        let t = i as f64 / 24.0 * std::f64::consts::TAU;
                        let c = pt(0.45, 0.38);
                        let r = 0.11;
                        let u = wall_pt(set, wall, a + 0.45 * (ww - 0.16) / 2.0 + t.cos() * r, y + 0.38 * (hh - 0.16) / 2.0 + t.sin() * r, 0.02);
                        let _ = c;
                        u
                    })
                    .collect();
                p.poly3(cam, &moon, Rgb(252, 246, 205), false);
                for (u, v) in [(-0.7, 0.5), (-0.3, 0.2), (0.1, 0.7), (-0.55, -0.3), (0.0, -0.1), (0.7, -0.4)] {
                    let c = pt(u, v);
                    p.line3(cam, c, [c[0] + 0.004, c[1], c[2]], 3.0 * px, Rgb(255, 255, 230));
                }
            } else if tod == "day" {
                for (u, v, s) in [(-0.4, 0.4, 0.22), (0.35, 0.1, 0.17)] {
                    let cloud: Vec<V3> = (0..20)
                        .map(|i| {
                            let t = i as f64 / 20.0 * std::f64::consts::TAU;
                            wall_pt(set, wall, a + u * (ww - 0.16) / 2.0 + t.cos() * s * 1.5, y + v * (hh - 0.16) / 2.0 + t.sin() * s * 0.6, 0.02)
                        })
                        .collect();
                    p.poly3(cam, &cloud, Rgb(255, 255, 255), false);
                }
            }
            // mullions
            let fr = Rgb(112, 72, 42);
            p.line3(cam, pt(0.0, -1.0), pt(0.0, 1.0), 5.0 * px, fr);
            p.line3(cam, pt(-1.0, 0.0), pt(1.0, 0.0), 5.0 * px, fr);
        }
        "door" => {
            let (ww, hh) = (pr.w.unwrap_or(1.0), pr.h.unwrap_or(2.1));
            let y = hh / 2.0;
            p.poly3(cam, &wall_rect(set, wall, a, y, ww + 0.14, hh + 0.07, 0.01), Rgb(88, 56, 34), true);
            p.poly3(cam, &wall_rect(set, wall, a, y, ww, hh, 0.015), pr.color.unwrap_or(Rgb(160, 108, 64)), true);
            let glass = wall_rect(set, wall, a, hh * 0.68, ww - 0.24, hh * 0.38, 0.02);
            p.poly3(cam, &glass, if tod == "night" { Rgb(22, 30, 70) } else { Rgb(176, 220, 238) }, true);
            let knob = wall_pt(set, wall, a + ww * 0.36, hh * 0.45, 0.03);
            p.line3(cam, knob, [knob[0] + 0.003, knob[1], knob[2]], 9.0 * px, Rgb(226, 190, 80));
        }
        "wall_clock" => {
            let r = pr.w.unwrap_or(0.56) / 2.0;
            let y = if pr.y > 0.0 { pr.y } else { 2.15 };
            let ring = |rad: f64, inset: f64| -> Vec<V3> {
                (0..32)
                    .map(|i| {
                        let t = i as f64 / 32.0 * std::f64::consts::TAU;
                        wall_pt(set, wall, a + t.cos() * rad, y + t.sin() * rad, inset)
                    })
                    .collect()
            };
            p.poly3(cam, &ring(r, 0.012), Rgb(200, 40, 50), true);
            p.poly3(cam, &ring(r * 0.82, 0.02), Rgb(250, 248, 240), true);
            let c = wall_pt(set, wall, a, y, 0.03);
            let hand = |ang: f64, len: f64| wall_pt(set, wall, a + ang.sin() * len, y + ang.cos() * len, 0.03);
            p.line3(cam, c, hand(2.4, r * 0.45), 5.0 * px, Rgb(20, 18, 22));
            p.line3(cam, c, hand(0.9, r * 0.68), 3.5 * px, Rgb(20, 18, 22));
            for k in 0..4 {
                let ang = k as f64 * std::f64::consts::FRAC_PI_2;
                p.line3(cam, hand(ang, r * 0.66), hand(ang, r * 0.72), 4.0 * px, Rgb(20, 18, 22));
            }
        }
        _ => {}
    }
}

// ------------------------------------------------------------------ props

pub struct PieceRef {
    pub depth: f64,
    pub prop: usize,
    pub piece: usize,
}

fn right_of(yaw: f64) -> V3 {
    [yaw.cos(), 0.0, -yaw.sin()]
}
fn fwd_of(yaw: f64) -> V3 {
    [yaw.sin(), 0.0, yaw.cos()]
}

fn counter_pieces(pr: &PropInst) -> usize {
    (pr.w.unwrap_or(5.0) / 1.25).ceil().max(1.0) as usize
}

fn piece_center(pr: &PropInst, piece: usize) -> V3 {
    match pr.kind.as_str() {
        "counter" => {
            let n = counter_pieces(pr);
            let w = pr.w.unwrap_or(5.0);
            let off = ((piece as f64 + 0.5) / n as f64 - 0.5) * w;
            let r = right_of(pr.yaw);
            [pr.at[0] + r[0] * off, 0.5, pr.at[1] + r[2] * off]
        }
        "pie_case" => [pr.at[0], if pr.y > 0.0 { pr.y } else { 1.05 } + 0.2, pr.at[1]],
        _ => [pr.at[0], 0.4, pr.at[1]],
    }
}

pub fn prop_pieces(set: &SetDef, cam: &Cam) -> Vec<PieceRef> {
    let mut out = vec![];
    for (i, pr) in set.props.iter().enumerate() {
        if pr.wall.is_some() {
            continue;
        }
        let n = if pr.kind == "counter" && !set.prop_is_3d(pr) { counter_pieces(pr) } else { 1 };
        let bias = if pr.kind == "pie_case" { -0.45 } else { 0.0 };
        for k in 0..n {
            let c = if n == 1 && pr.kind == "counter" { cam.to_cam([pr.at[0], 0.5, pr.at[1]]) } else { cam.to_cam(piece_center(pr, k)) };
            out.push(PieceRef { depth: c[2] + bias, prop: i, piece: k });
        }
    }
    out
}

/// Draws a box with three face colors: [top, front/back, left/right].
fn box3(p: &mut Painter, cam: &Cam, c: [f64; 2], y0: f64, size: [f64; 3], yaw: f64, cols: [Rgb; 3]) {
    let r = right_of(yaw);
    let f = fwd_of(yaw);
    let corner = |i: f64, j: f64, k: f64| -> V3 {
        [
            c[0] + r[0] * i * size[0] / 2.0 + f[0] * k * size[2] / 2.0,
            y0 + j * size[1],
            c[1] + r[2] * i * size[0] / 2.0 + f[2] * k * size[2] / 2.0,
        ]
    };
    let center = [c[0], y0 + size[1] / 2.0, c[1]];
    // (normal, corners, color index)
    let faces: [(V3, [V3; 4], usize); 5] = [
        ([0.0, 1.0, 0.0], [corner(-1.0, 1.0, -1.0), corner(1.0, 1.0, -1.0), corner(1.0, 1.0, 1.0), corner(-1.0, 1.0, 1.0)], 0),
        (f, [corner(-1.0, 0.0, 1.0), corner(1.0, 0.0, 1.0), corner(1.0, 1.0, 1.0), corner(-1.0, 1.0, 1.0)], 1),
        (mul3(f, -1.0), [corner(-1.0, 0.0, -1.0), corner(1.0, 0.0, -1.0), corner(1.0, 1.0, -1.0), corner(-1.0, 1.0, -1.0)], 1),
        (r, [corner(1.0, 0.0, -1.0), corner(1.0, 0.0, 1.0), corner(1.0, 1.0, 1.0), corner(1.0, 1.0, -1.0)], 2),
        (mul3(r, -1.0), [corner(-1.0, 0.0, -1.0), corner(-1.0, 0.0, 1.0), corner(-1.0, 1.0, 1.0), corner(-1.0, 1.0, -1.0)], 2),
    ];
    for (n, quad, ci) in faces {
        // Face center for the visibility test.
        let fc = [
            (quad[0][0] + quad[2][0]) / 2.0,
            (quad[0][1] + quad[2][1]) / 2.0,
            (quad[0][2] + quad[2][2]) / 2.0,
        ];
        let _ = center;
        if dot3(n, sub3(cam.pos, fc)) > 0.0 {
            p.poly3(cam, &quad, cols[ci], true);
        }
    }
}

/// Upright prism (cylinder-like) with `sides` sides.
fn prism(p: &mut Painter, cam: &Cam, c: [f64; 2], y0: f64, h: f64, r: f64, sides: usize, top: Rgb, side: Rgb) {
    let ang = |i: usize| i as f64 / sides as f64 * std::f64::consts::TAU;
    let pt = |i: usize, y: f64| -> V3 { [c[0] + ang(i).cos() * r, y, c[1] + ang(i).sin() * r] };
    for i in 0..sides {
        let mid = (ang(i) + ang(i + 1)) / 2.0;
        let n = [mid.cos(), 0.0, mid.sin()];
        let fc = [c[0] + n[0] * r, y0 + h / 2.0, c[1] + n[2] * r];
        if dot3(n, sub3(cam.pos, fc)) > 0.0 {
            let shade = 0.82 + 0.18 * n[0];
            let quad = [pt(i, y0), pt(i + 1, y0), pt(i + 1, y0 + h), pt(i, y0 + h)];
            p.poly3(cam, &quad, side.scale(shade.clamp(0.7, 1.05)), true);
        }
    }
    if cam.pos[1] > y0 + h {
        let poly: Vec<V3> = (0..sides).map(|i| pt(i, y0 + h)).collect();
        p.poly3(cam, &poly, top, true);
    }
}

fn face_stripe(p: &mut Painter, cam: &Cam, c: [f64; 2], yaw: f64, w: f64, d: f64, y0: f64, y1: f64, col: Rgb) {
    let r = right_of(yaw);
    let f = fwd_of(yaw);
    for sgn in [1.0, -1.0] {
        let off = sgn * (d / 2.0 + 0.004);
        let pt = |i: f64, y: f64| -> V3 {
            [c[0] + r[0] * i * w / 2.0 + f[0] * off, y, c[1] + r[2] * i * w / 2.0 + f[2] * off]
        };
        let n = mul3(f, sgn);
        let fc = pt(0.0, (y0 + y1) / 2.0);
        if dot3(n, sub3(cam.pos, fc)) > 0.0 {
            p.poly3(cam, &[pt(-1.0, y0), pt(1.0, y0), pt(1.0, y1), pt(-1.0, y1)], col, false);
        }
    }
}

pub fn draw_piece(p: &mut Painter, cam: &Cam, set: &SetDef, prop: usize, piece: usize, px: f32) {
    let pr = &set.props[prop];
    p.ow = 2.2 * px;
    match pr.kind.as_str() {
        "counter" => {
            let n = counter_pieces(pr);
            let w = pr.w.unwrap_or(5.0);
            let d = pr.d.unwrap_or(0.8);
            let h = pr.h.unwrap_or(1.0);
            let pw = w / n as f64;
            let c3 = piece_center(pr, piece);
            let c = [c3[0], c3[2]];
            let body = pr.color.unwrap_or(set.trim);
            box3(p, cam, c, 0.0, [pw, h - 0.05, d], pr.yaw, [body, body, body.scale(0.78)]);
            face_stripe(p, cam, c, pr.yaw, pw, d, 0.42, 0.52, Rgb(244, 240, 230));
            face_stripe(p, cam, c, pr.yaw, pw, d, 0.06, 0.12, Rgb(190, 190, 196));
            box3(
                p,
                cam,
                c,
                h - 0.05,
                [pw + 0.004, 0.07, d + 0.14],
                pr.yaw,
                [Rgb(232, 232, 236), Rgb(190, 190, 196), Rgb(170, 170, 176)],
            );
        }
        "stool" => {
            let c = pr.at;
            let seat_y = pr.h.unwrap_or(0.6);
            prism(p, cam, c, 0.0, 0.035, 0.2, 12, Rgb(170, 170, 178), Rgb(150, 150, 158));
            prism(p, cam, c, 0.03, seat_y - 0.1, 0.035, 8, Rgb(200, 200, 208), Rgb(180, 180, 188));
            let seat = pr.color.unwrap_or(Rgb(196, 44, 56));
            prism(p, cam, c, seat_y - 0.07, 0.08, 0.2, 14, seat.lerp(Rgb(255, 255, 255), 0.15), seat);
        }
        "table" => {
            let (w, d, h) = (pr.w.unwrap_or(1.2), pr.d.unwrap_or(0.8), pr.h.unwrap_or(0.74));
            let r = right_of(pr.yaw);
            let f = fwd_of(pr.yaw);
            for (i, k) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                let lc = [
                    pr.at[0] + r[0] * i * (w / 2.0 - 0.08) + f[0] * k * (d / 2.0 - 0.08),
                    pr.at[1] + r[2] * i * (w / 2.0 - 0.08) + f[2] * k * (d / 2.0 - 0.08),
                ];
                box3(p, cam, lc, 0.0, [0.06, h - 0.04, 0.06], pr.yaw, [Rgb(170, 170, 178), Rgb(150, 150, 158), Rgb(130, 130, 138)]);
            }
            let top = pr.color.unwrap_or(Rgb(236, 230, 214));
            box3(p, cam, pr.at, h - 0.05, [w, 0.05, d], pr.yaw, [top, top.scale(0.8), top.scale(0.7)]);
        }
        "pie_case" => {
            let (w, d) = (pr.w.unwrap_or(0.9), pr.d.unwrap_or(0.5));
            let y0 = if pr.y > 0.0 { pr.y } else { 1.05 };
            let r = right_of(pr.yaw);
            box3(p, cam, pr.at, y0, [w, 0.08, d], pr.yaw, [Rgb(210, 210, 216), Rgb(180, 180, 188), Rgb(160, 160, 168)]);
            let cols = [Rgb(214, 160, 90), Rgb(196, 56, 70), Rgb(240, 222, 150)];
            for (k, col) in cols.iter().enumerate() {
                let off = (k as f64 - 1.0) * w * 0.3;
                let c = [pr.at[0] + r[0] * off, pr.at[1] + r[2] * off];
                prism(p, cam, c, y0 + 0.08, 0.06, 0.12, 12, col.lerp(Rgb(255, 255, 255), 0.2), *col);
            }
            // Glass dome (translucent).
            let rr = right_of(pr.yaw);
            let ff = fwd_of(pr.yaw);
            let gh = 0.3;
            let corner = |i: f64, j: f64, k: f64| -> V3 {
                [
                    pr.at[0] + rr[0] * i * w / 2.0 + ff[0] * k * d / 2.0,
                    y0 + 0.08 + j * gh,
                    pr.at[1] + rr[2] * i * w / 2.0 + ff[2] * k * d / 2.0,
                ]
            };
            let glass = Rgb(190, 235, 245);
            let faces: [(V3, [V3; 4]); 5] = [
                (ff, [corner(-1.0, 0.0, 1.0), corner(1.0, 0.0, 1.0), corner(1.0, 1.0, 1.0), corner(-1.0, 1.0, 1.0)]),
                (mul3(ff, -1.0), [corner(-1.0, 0.0, -1.0), corner(1.0, 0.0, -1.0), corner(1.0, 1.0, -1.0), corner(-1.0, 1.0, -1.0)]),
                (rr, [corner(1.0, 0.0, -1.0), corner(1.0, 0.0, 1.0), corner(1.0, 1.0, 1.0), corner(1.0, 1.0, -1.0)]),
                (mul3(rr, -1.0), [corner(-1.0, 0.0, -1.0), corner(-1.0, 0.0, 1.0), corner(-1.0, 1.0, 1.0), corner(-1.0, 1.0, -1.0)]),
                ([0.0, 1.0, 0.0], [corner(-1.0, 1.0, -1.0), corner(1.0, 1.0, -1.0), corner(1.0, 1.0, 1.0), corner(-1.0, 1.0, 1.0)]),
            ];
            for (n, quad) in faces {
                let fc = [(quad[0][0] + quad[2][0]) / 2.0, (quad[0][1] + quad[2][1]) / 2.0, (quad[0][2] + quad[2][2]) / 2.0];
                if dot3(n, sub3(cam.pos, fc)) > 0.0 {
                    p.poly3_alpha(cam, &quad, glass, 80);
                    for i in 0..4 {
                        p.line3(cam, quad[i], quad[(i + 1) % 4], 1.6 * px, Rgb(235, 250, 255));
                    }
                }
            }
        }
        _ => {}
    }
}
