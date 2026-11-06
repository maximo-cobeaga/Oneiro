//! The `ink` post style: a character drawn in its own style is re-inked as a pen drawing. Tones collapse
//! to three washes of one ink colour, shadows get diagonal hatching, and the silhouette gets a heavy line.

use tiny_skia::Pixmap;

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

pub fn shade(layer: &mut Pixmap, size_px: f32, anchor: (f32, f32), _seed: u64) {
    let (w, h) = (layer.width() as i32, layer.height() as i32);
    let src: Vec<u8> = layer.data().to_vec();
    let alpha = |x: i32, y: i32| -> u8 { if x < 0 || y < 0 || x >= w || y >= h { 0 } else { src[((y * w + x) * 4 + 3) as usize] } };
    let paper = [244.0, 236.0, 218.0];
    let ink = [26.0, 24.0, 40.0];
    let spacing = (size_px / 70.0).max(2.5);
    let line = (size_px / 150.0).max(1.0).round() as i32;
    let d = layer.data_mut();
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            let a = src[i + 3];
            if a == 0 {
                continue;
            }
            // Un-premultiply to get the real colour.
            let af = a as f32 / 255.0;
            let (r, g, b) = (src[i] as f32 / af, src[i + 1] as f32 / af, src[i + 2] as f32 / af);
            let lum = ((0.299 * r + 0.587 * g + 0.114 * b) / 255.0).clamp(0.0, 1.0);
            // Three washes: paper, light wash, dark wash; hatching deepens the darkest.
            let mut level = if lum > 0.62 { 0.0 } else if lum > 0.30 { 0.28 } else { 0.55 };
            if lum <= 0.30 {
                let u = (x as f32 - anchor.0 + y as f32 - anchor.1).rem_euclid(spacing * 2.0);
                if u < spacing * 0.7 {
                    level = 0.9;
                }
            }
            // Silhouette: any neighbour within `line` pixels that is empty.
            let mut edge = false;
            'o: for dy in -line..=line {
                for dx in -line..=line {
                    if alpha(x + dx, y + dy) < 40 {
                        edge = true;
                        break 'o;
                    }
                }
            }
            if edge {
                level = 1.0;
            }
            let c = mix(paper, ink, level);
            d[i] = (c[0] * af).round() as u8;
            d[i + 1] = (c[1] * af).round() as u8;
            d[i + 2] = (c[2] * af).round() as u8;
        }
    }
}
