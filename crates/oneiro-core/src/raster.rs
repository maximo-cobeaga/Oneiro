//! Raster (PNG) parts for characters cut out of an illustration (tier C). A part is a PNG with
//! transparency; like an SVG part it is scaled to a fraction of the character's height and turns
//! around a pivot.

use tiny_skia::Pixmap;

pub const MAX_SIDE: u32 = 2048;
pub const MIN_SIDE: u32 = 8;

#[derive(Clone, Debug)]
pub struct Raster {
    /// Premultiplied RGBA, as the renderer wants it.
    pub pm: Pixmap,
}

impl Raster {
    pub fn from_png(bytes: &[u8]) -> Result<Raster, String> {
        let pm = Pixmap::decode_png(bytes).map_err(|e| format!("not a valid PNG: {}", e))?;
        let (w, h) = (pm.width(), pm.height());
        if w < MIN_SIDE || h < MIN_SIDE {
            return Err(format!("the image is only {}x{} px; the minimum is {}x{}", w, h, MIN_SIDE, MIN_SIDE));
        }
        if w > MAX_SIDE || h > MAX_SIDE {
            return Err(format!("the image is {}x{} px; the maximum is {}x{} (scale it down)", w, h, MAX_SIDE, MAX_SIDE));
        }
        if !pm.data().chunks(4).any(|p| p[3] > 8) {
            return Err("the image is fully transparent".into());
        }
        Ok(Raster { pm })
    }

    pub fn width(&self) -> u32 {
        self.pm.width()
    }

    pub fn height(&self) -> u32 {
        self.pm.height()
    }

    pub fn aspect(&self) -> f32 {
        self.pm.width() as f32 / self.pm.height().max(1) as f32
    }

    /// Bounding box of the visible pixels: (min_x, min_y, max_x, max_y), max exclusive.
    pub fn visible_bounds(&self) -> Option<(u32, u32, u32, u32)> {
        let (w, h) = (self.width() as usize, self.height() as usize);
        let d = self.pm.data();
        let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
        for y in 0..h {
            for x in 0..w {
                if d[(y * w + x) * 4 + 3] > 8 {
                    x0 = x0.min(x);
                    x1 = x1.max(x + 1);
                    y0 = y0.min(y);
                    y1 = y1.max(y + 1);
                }
            }
        }
        (x1 > x0).then_some((x0 as u32, y0 as u32, x1 as u32, y1 as u32))
    }
}
