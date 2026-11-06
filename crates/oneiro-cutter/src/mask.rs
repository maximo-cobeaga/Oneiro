//! Background removal. An illustration usually has a plain backdrop (or real transparency): the
//! backdrop color is learned from the border of the image and flood-filled away from the edges, so
//! colors inside the character that happen to match it are kept.

use crate::Image;

#[derive(Clone, Debug)]
pub struct Mask {
    pub w: usize,
    pub h: usize,
    /// Soft alpha of the character, 0 = background.
    pub a: Vec<u8>,
}

impl Mask {
    /// Bounding box of the solid pixels: (min_x, min_y, max_x, max_y), max exclusive.
    pub fn bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (self.w, self.h, 0usize, 0usize);
        for y in 0..self.h {
            for x in 0..self.w {
                if self.a[y * self.w + x] > 128 {
                    x0 = x0.min(x);
                    x1 = x1.max(x + 1);
                    y0 = y0.min(y);
                    y1 = y1.max(y + 1);
                }
            }
        }
        (x1 > x0).then_some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
    }

    /// The vertical axis of symmetry as a fraction of the width: the center of the head (the top quarter of
    /// the character), which a tail or a hand held out does not move; the bounding box center as a fallback.
    pub fn axis(&self) -> f32 {
        let Some((x0, y0, x1, y1)) = self.bounds() else { return 0.5 };
        let (mut sum, mut n) = (0.0f64, 0.0f64);
        for y in y0 as usize..(y0 as usize + ((y1 - y0) as f32 * 0.25) as usize).max(y0 as usize + 1).min(self.h) {
            for x in 0..self.w {
                if self.solid(x, y) {
                    sum += x as f64 + 0.5;
                    n += 1.0;
                }
            }
        }
        if n > 0.0 {
            (sum / n) as f32 / self.w as f32
        } else {
            (x0 + x1) as f32 * 0.5 / self.w as f32
        }
    }

    pub fn solid(&self, x: usize, y: usize) -> bool {
        self.a[y * self.w + x] > 128
    }

    pub fn area(&self) -> usize {
        self.a.iter().filter(|a| **a > 128).count()
    }
}

fn straight(p: &[u8]) -> [f32; 3] {
    let a = p[3] as f32 / 255.0;
    if a <= 0.0 {
        return [0.0; 3];
    }
    [p[0] as f32 / a, p[1] as f32 / a, p[2] as f32 / a]
}

fn dist(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

/// `tolerance`: color distance (0-441) treated as background; `None` picks it from the border noise.
pub fn extract_mask(img: &Image, tolerance: Option<f32>) -> Result<Mask, String> {
    let (w, h) = (img.width() as usize, img.height() as usize);
    let data = img.pm.data();
    let n = w * h;

    // Real transparency wins.
    let translucent = data.chunks(4).filter(|p| p[3] < 250).count();
    let solid = data.chunks(4).filter(|p| p[3] >= 250).count();
    if translucent * 50 > n && solid * 50 > n {
        let mask = Mask { w, h, a: data.chunks(4).map(|p| p[3]).collect() };
        return finish(mask, n);
    }

    // Backdrop color: per-channel median of the border ring.
    let ring = (w.min(h) / 100).max(2);
    let mut samples: Vec<[f32; 3]> = Vec::new();
    for y in 0..h {
        for x in 0..w {
            if x < ring || y < ring || x >= w - ring || y >= h - ring {
                samples.push(straight(&data[(y * w + x) * 4..][..4]));
            }
        }
    }
    let med = |c: usize| {
        let mut v: Vec<f32> = samples.iter().map(|s| s[c]).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    };
    let bg = [med(0), med(1), med(2)];
    let spread = samples.iter().map(|s| dist(*s, bg)).sum::<f32>() / samples.len() as f32;
    let tol = tolerance.unwrap_or((spread * 3.0 + 14.0).clamp(18.0, 60.0));

    // Flood fill the backdrop from the borders.
    let color = |i: usize| straight(&data[i * 4..][..4]);
    let mut is_bg = vec![false; n];
    let mut stack: Vec<usize> = Vec::new();
    let push = |i: usize, is_bg: &mut Vec<bool>, stack: &mut Vec<usize>| {
        if !is_bg[i] && dist(color(i), bg) <= tol {
            is_bg[i] = true;
            stack.push(i);
        }
    };
    for x in 0..w {
        push(x, &mut is_bg, &mut stack);
        push((h - 1) * w + x, &mut is_bg, &mut stack);
    }
    for y in 0..h {
        push(y * w, &mut is_bg, &mut stack);
        push(y * w + w - 1, &mut is_bg, &mut stack);
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        if x > 0 {
            push(i - 1, &mut is_bg, &mut stack);
        }
        if x + 1 < w {
            push(i + 1, &mut is_bg, &mut stack);
        }
        if y > 0 {
            push(i - w, &mut is_bg, &mut stack);
        }
        if y + 1 < h {
            push(i + w, &mut is_bg, &mut stack);
        }
    }

    // Keep every sizeable blob (a head may float apart from the body) and drop specks, signatures, stray marks.
    let mut label = vec![0u32; n];
    let (mut best, mut best_size, mut next) = (0u32, 0usize, 1u32);
    let mut sizes: Vec<usize> = vec![0];
    for start in 0..n {
        if is_bg[start] || label[start] != 0 {
            continue;
        }
        let mut size = 0usize;
        label[start] = next;
        stack.push(start);
        while let Some(i) = stack.pop() {
            size += 1;
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize, stack: &mut Vec<usize>| {
                if !is_bg[j] && label[j] == 0 {
                    label[j] = next;
                    stack.push(j);
                }
            };
            if x > 0 {
                visit(i - 1, &mut stack);
            }
            if x + 1 < w {
                visit(i + 1, &mut stack);
            }
            if y > 0 {
                visit(i - w, &mut stack);
            }
            if y + 1 < h {
                visit(i + w, &mut stack);
            }
        }
        sizes.push(size);
        if size > best_size {
            best_size = size;
            best = next;
        }
        next += 1;
    }

    // Soft edge: foreground pixels touching the backdrop fade with their color distance.
    let mut a = vec![0u8; n];
    for i in 0..n {
        let l = label[i] as usize;
        if l == 0 || best == 0 || sizes[l] * 100 < best_size * 3 {
            continue;
        }
        let (x, y) = (i % w, i / w);
        let edge = (x > 0 && is_bg[i - 1]) || (x + 1 < w && is_bg[i + 1]) || (y > 0 && is_bg[i - w]) || (y + 1 < h && is_bg[i + w]);
        a[i] = if edge { (((dist(color(i), bg) - 0.5 * tol) / (0.5 * tol)).clamp(0.4, 1.0) * 255.0) as u8 } else { 255 };
    }
    finish(Mask { w, h, a }, n)
}

fn finish(mask: Mask, n: usize) -> Result<Mask, String> {
    let area = mask.area();
    if area * 50 < n {
        return Err("could not find a character: it covers less than 2% of the image. Use a plain background or a PNG with transparency".into());
    }
    if area * 100 > n * 92 {
        return Err("the background could not be separated (the character would cover more than 92% of the image). Use a plain, uniform backdrop or a PNG with transparency".into());
    }
    Ok(mask)
}
