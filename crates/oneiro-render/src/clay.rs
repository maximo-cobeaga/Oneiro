//! The `clay` style: a plasticine look applied *on top of* a character's own drawing. The
//! character is drawn into a transparent layer without outlines, then lit like a soft pillow
//! (its blurred silhouette is the height map), given a lumpy fingerprint texture that sticks to the
//! body and a glossy highlight. Time is also quantized to 12 fps with a small jitter per frame, the
//! way stop-motion is shot. Everything is a pure function of the inputs, so frames stay
//! reproducible.

use tiny_skia::Pixmap;

/// Deterministic 2D hash in 0..1.
fn hash2(x: i32, y: i32, seed: u64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E3779B185EBCA87) ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F) ^ seed.wrapping_mul(0x165667B19E3779F9);
    h ^= h >> 29;
    h = h.wrapping_mul(0xBF58476D1CE4E5B9);
    h ^= h >> 32;
    (h & 0xFFFFFF) as f32 / 0xFFFFFF as f32
}

/// Smooth value noise in -1..1.
fn noise(x: f32, y: f32, seed: u64) -> f32 {
    let (xi, yi) = (x.floor(), y.floor());
    let (fx, fy) = (x - xi, y - yi);
    let (u, v) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (xi, yi) = (xi as i32, yi as i32);
    let a = hash2(xi, yi, seed);
    let b = hash2(xi + 1, yi, seed);
    let c = hash2(xi, yi + 1, seed);
    let d = hash2(xi + 1, yi + 1, seed);
    let top = a + (b - a) * u;
    let bot = c + (d - c) * u;
    (top + (bot - top) * v) * 2.0 - 1.0
}

/// 3-pass box blur of a plane (approximates a gaussian), radius in pixels.
fn blur(src: &[f32], w: usize, h: usize, radius: usize) -> Vec<f32> {
    let mut a = src.to_vec();
    let mut b = vec![0.0f32; w * h];
    let r = radius.max(1);
    for _ in 0..3 {
        // horizontal
        for y in 0..h {
            let row = &a[y * w..(y + 1) * w];
            let out = &mut b[y * w..(y + 1) * w];
            let mut sum = 0.0f32;
            // window [x-r, x+r] with edge clamping to 0 (transparent outside the layer)
            for x in 0..=r.min(w - 1) {
                sum += row[x];
            }
            for x in 0..w {
                out[x] = sum / (2 * r + 1) as f32;
                let add = x + r + 1;
                if add < w {
                    sum += row[add];
                }
                if x >= r {
                    sum -= row[x - r];
                }
            }
        }
        // vertical
        for x in 0..w {
            let mut sum = 0.0f32;
            for y in 0..=r.min(h - 1) {
                sum += b[y * w + x];
            }
            for y in 0..h {
                a[y * w + x] = sum / (2 * r + 1) as f32;
                let add = y + r + 1;
                if add < h {
                    sum += b[add * w + x];
                }
                if y >= r {
                    sum -= b[(y - r) * w + x];
                }
            }
        }
    }
    a
}

/// Turns a flat layer (premultiplied RGBA, transparent background) into plasticine.
/// `size_px` is the character's height in pixels (everything scales with it) and `anchor` the
/// character's foot position inside the layer (the texture is attached to it).
pub fn shade(layer: &mut Pixmap, size_px: f32, anchor: (f32, f32), seed: u64) {
    let (w, h) = (layer.width() as usize, layer.height() as usize);
    if w < 4 || h < 4 {
        return;
    }
    let data = layer.data_mut();
    // Only the part of the layer that has something in it is processed (a layer is mostly air).
    let (mut bx0, mut by0, mut bx1, mut by1) = (w, h, 0usize, 0usize);
    for y in 0..h {
        let row = &data[y * w * 4..(y + 1) * w * 4];
        if let (Some(a), Some(b)) = (row.chunks(4).position(|p| p[3] != 0), row.chunks(4).rposition(|p| p[3] != 0)) {
            bx0 = bx0.min(a);
            bx1 = bx1.max(b + 1);
            by0 = by0.min(y);
            by1 = by1.max(y + 1);
        }
    }
    if bx1 <= bx0 || by1 <= by0 {
        return;
    }
    let (cx0, cy0) = (bx0.saturating_sub(2), by0.saturating_sub(2));
    let (cx1, cy1) = ((bx1 + 2).min(w), (by1 + 2).min(h));
    let cw = cx1 - cx0;
    let ch = cy1 - cy0;
    let alpha: Vec<f32> = (0..cw * ch).map(|i| data[((cy0 + i / cw) * w + cx0 + i % cw) * 4 + 3] as f32 / 255.0).collect();
    let radius = ((size_px * 0.028) as usize).clamp(2, 28);
    let height = blur(&alpha, cw, ch, radius);
    // Slope scale: a pillow of half-width ~radius has gradients around 1/radius per pixel.
    let k = radius as f32 * 1.1;
    let light = {
        let (x, y, z) = (-0.48f32, -0.58f32, 0.66f32);
        let n = (x * x + y * y + z * z).sqrt();
        (x / n, y / n, z / n)
    };
    let half = {
        let (x, y, z) = (light.0, light.1, light.2 + 1.0);
        let n = (x * x + y * y + z * z).sqrt();
        (x / n, y / n, z / n)
    };
    let cell = (size_px * 0.09).max(4.0);
    let fine = (size_px * 0.011).max(1.5);
    for y in cy0.max(1)..cy1.min(h - 1) {
        for x in cx0.max(1)..cx1.min(w - 1) {
            let i = y * w + x;
            let a = data[i * 4 + 3];
            if a == 0 {
                continue;
            }
            let hi = (y - cy0) * cw + (x - cx0);
            let (xl, xr) = (if x > cx0 { hi - 1 } else { hi }, if x + 1 < cx1 { hi + 1 } else { hi });
            let (yu, yd) = (if y > cy0 { hi - cw } else { hi }, if y + 1 < cy1 { hi + cw } else { hi });
            let gx = (height[xr] - height[xl]) * 0.5 * k;
            let gy = (height[yd] - height[yu]) * 0.5 * k;
            // Lumps and fingerprints perturb the surface normal (texture coordinates relative to the body).
            let (tx, ty) = (x as f32 - anchor.0, y as f32 - anchor.1);
            let lump_x = noise(tx / cell, ty / cell, seed) * 0.055;
            let lump_y = noise(tx / cell + 31.7, ty / cell - 12.3, seed) * 0.055;
            let (nx, ny, nz) = (-gx + lump_x, -gy + lump_y, 1.0);
            let nl = (nx * nx + ny * ny + nz * nz).sqrt();
            let (nx, ny, nz) = (nx / nl, ny / nl, nz / nl);
            let diffuse = (nx * light.0 + ny * light.1 + nz * light.2).max(0.0);
            let spec = (nx * half.0 + ny * half.1 + nz * half.2).max(0.0).powi(26);
            let grain = 1.0 + 0.03 * noise(tx / fine, ty / fine, seed ^ 0x1234);
            // Straight-alpha color.
            let af = a as f32 / 255.0;
            let (r, g, b) = (data[i * 4] as f32 / 255.0 / af, data[i * 4 + 1] as f32 / 255.0 / af, data[i * 4 + 2] as f32 / 255.0 / af);
            let lum = 0.3 * r + 0.59 * g + 0.11 * b;
            // Slightly more saturated, like dyed clay.
            let sat = 1.12;
            let (r, g, b) = ((lum + (r - lum) * sat).clamp(0.0, 1.0), (lum + (g - lum) * sat).clamp(0.0, 1.0), (lum + (b - lum) * sat).clamp(0.0, 1.0));
            let shade = (0.5 + 0.5 * diffuse) * grain;
            // Highlights only on light colors (a black eyebrow stays black).
            let gloss = spec * 0.32 * (0.35 + lum);
            let out = |c: f32| ((c * shade + gloss).clamp(0.0, 1.0) * af * 255.0).round() as u8;
            data[i * 4] = out(r);
            data[i * 4 + 1] = out(g);
            data[i * 4 + 2] = out(b);
        }
    }
}

/// Per-frame stop-motion wobble in pixels (x, y), steady within one 12 fps frame.
pub fn jitter(frame12: i64, seed: u64, size_px: f32) -> (f32, f32) {
    let jx = hash2(frame12 as i32, 11, seed) * 2.0 - 1.0;
    let jy = hash2(frame12 as i32, 29, seed) * 2.0 - 1.0;
    (jx * size_px * 0.004, jy * size_px * 0.003)
}
