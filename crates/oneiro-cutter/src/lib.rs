//! The hybrid cutter (tier C): turns an illustration of a character into a rigged Oneiro character.
//!
//! The flow is human + AI cooperation with the engine as judge:
//! 1. `extract_mask` removes the background (or uses the image's own transparency);
//! 2. a `Layout` of four boxes (head, body, arm, leg) is proposed by `propose` (heuristics) or by a
//!    vision model, and the person drags them in the IDE;
//! 3. `validate` checks the layout and reports every problem in plain words;
//! 4. `cut` produces the PNG pieces with pivots and `write_character` saves a normal `.onchar`.
//!
//! The source illustration is assumed to be a character seen from the front, standing, with the
//! arms hanging at the sides. The right arm and right leg are the mirror of the left ones.

pub mod layout;
pub mod mask;
pub mod overlay;

pub use layout::{propose, validate, Issue, Layout, RectN, Severity, PARTS};
pub use mask::{extract_mask, Mask};

use oneiro_core::defs::{SvgPart, SvgRig};
use oneiro_core::CharacterDef;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};

/// Pieces are scaled down so that no side is longer than this (pixels).
pub const MAX_PIECE_SIDE: u32 = 512;

pub struct Image {
    pub pm: Pixmap,
}

impl Image {
    pub fn from_png(bytes: &[u8]) -> Result<Image, String> {
        let pm = Pixmap::decode_png(bytes).map_err(|e| format!("not a valid image: {}", e))?;
        if pm.width() < 64 || pm.height() < 64 {
            return Err(format!("the image is only {}x{} px; use at least 64x64", pm.width(), pm.height()));
        }
        if pm.width() > 8000 || pm.height() > 8000 {
            return Err("the image is larger than 8000 px on a side; scale it down first".into());
        }
        Ok(Image { pm })
    }

    /// Loads PNG directly and any other format (jpg, webp, bmp, gif...) through `ffmpeg`.
    pub fn load(path: &Path) -> Result<Image, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {}", path.display(), e))?;
        if bytes.len() > 40_000_000 {
            return Err("the file is larger than 40 MB".into());
        }
        if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
            return Image::from_png(&bytes);
        }
        let tmp = std::env::temp_dir().join(format!("oneiro_cutter_{}_{}.png", std::process::id(), oneiro_unique()));
        let mut ff = Command::new("ffmpeg");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            ff.creation_flags(0x0800_0000);
        }
        let out = ff
            .args(["-v", "error", "-nostdin", "-y", "-i"])
            .arg(path)
            .args(["-frames:v", "1", "-pix_fmt", "rgba"])
            .arg(&tmp)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("cannot convert {} (ffmpeg is needed for formats other than PNG: {})", path.display(), e))?;
        if !out.status.success() {
            let _ = std::fs::remove_file(&tmp);
            return Err(format!("ffmpeg could not read {}: {}", path.display(), String::from_utf8_lossy(&out.stderr).trim()));
        }
        let r = std::fs::read(&tmp).map_err(|e| e.to_string()).and_then(|b| Image::from_png(&b));
        let _ = std::fs::remove_file(&tmp);
        r
    }

    pub fn width(&self) -> u32 {
        self.pm.width()
    }
    pub fn height(&self) -> u32 {
        self.pm.height()
    }
    pub fn png(&self) -> Result<Vec<u8>, String> {
        self.pm.encode_png().map_err(|e| e.to_string())
    }
}

fn oneiro_unique() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    N.fetch_add(1, Ordering::Relaxed)
}

/// One cut piece, ready to be saved as PNG.
pub struct Piece {
    pub name: &'static str,
    pub pm: Pixmap,
    /// Joint inside the piece, 0..1 from its top-left.
    pub pivot: (f32, f32),
    /// Height as a fraction of the character's total height.
    pub height: f32,
}

pub struct Cut {
    pub pieces: Vec<Piece>,
    /// Character height in source pixels (the mask's bounding box).
    pub char_h_px: f32,
}

fn premul_to_straight(p: &[u8]) -> [u8; 3] {
    let a = p[3] as f32 / 255.0;
    if a <= 0.0 {
        return [0, 0, 0];
    }
    [(p[0] as f32 / a).min(255.0) as u8, (p[1] as f32 / a).min(255.0) as u8, (p[2] as f32 / a).min(255.0) as u8]
}

/// Cuts the four pieces out of `img` using `mask` for transparency.
pub fn cut(img: &Image, mask: &Mask, layout: &Layout) -> Result<Cut, String> {
    let (iw, ih) = (img.width() as f32, img.height() as f32);
    let (_, by0, _, by1) = mask.bounds().ok_or("the mask is empty")?;
    let char_h = (by1 - by0) as f32;
    let mut pieces = Vec::new();
    for (name, r) in layout.rects() {
        let (x0, y0, x1, y1) = (((r.x0 * iw).floor().max(0.0)) as u32, ((r.y0 * ih).floor().max(0.0)) as u32, ((r.x1 * iw).ceil().min(iw)) as u32, ((r.y1 * ih).ceil().min(ih)) as u32);
        if x1 <= x0 + 1 || y1 <= y0 + 1 {
            return Err(format!("the {} box is empty", name));
        }
        let (w, h) = (x1 - x0, y1 - y0);
        let mut raw = Pixmap::new(w, h).ok_or("cannot allocate the piece")?;
        {
            let (src, dst) = (img.pm.data(), raw.data_mut());
            for y in 0..h {
                for x in 0..w {
                    let (sx, sy) = ((x0 + x) as usize, (y0 + y) as usize);
                    let a = mask.a[sy * mask.w + sx];
                    if a == 0 {
                        continue;
                    }
                    let s = &src[(sy * img.width() as usize + sx) * 4..][..4];
                    let c = premul_to_straight(s);
                    let o = ((y * w + x) * 4) as usize;
                    let af = a as f32 / 255.0;
                    dst[o] = (c[0] as f32 * af).round() as u8;
                    dst[o + 1] = (c[1] as f32 * af).round() as u8;
                    dst[o + 2] = (c[2] as f32 * af).round() as u8;
                    dst[o + 3] = a;
                }
            }
        }
        // Trim to the visible pixels.
        let (mut tx0, mut ty0, mut tx1, mut ty1) = (w, h, 0u32, 0u32);
        for y in 0..h {
            for x in 0..w {
                if raw.data()[((y * w + x) * 4 + 3) as usize] > 8 {
                    tx0 = tx0.min(x);
                    tx1 = tx1.max(x + 1);
                    ty0 = ty0.min(y);
                    ty1 = ty1.max(y + 1);
                }
            }
        }
        if tx1 <= tx0 || ty1 <= ty0 {
            return Err(format!("the {} box contains none of the character: move it over the {}", name, name));
        }
        let (tw, th) = (tx1 - tx0, ty1 - ty0);
        let mut trimmed = Pixmap::new(tw, th).ok_or("cannot allocate the piece")?;
        for y in 0..th {
            let s = (((ty0 + y) * w + tx0) * 4) as usize;
            let d = (y * tw * 4) as usize;
            trimmed.data_mut()[d..d + (tw * 4) as usize].copy_from_slice(&raw.data()[s..s + (tw * 4) as usize]);
        }
        // Joint positions from the piece's own silhouette.
        let band = ((th as f32 * 0.1).ceil() as u32).max(2).min(th);
        let centroid_x = |rows: std::ops::Range<u32>| -> Option<(f32, f32, f32)> {
            let (mut sum, mut n, mut lo, mut hi) = (0.0f32, 0.0f32, tw as f32, 0.0f32);
            for y in rows {
                for x in 0..tw {
                    if trimmed.data()[((y * tw + x) * 4 + 3) as usize] > 8 {
                        sum += x as f32;
                        n += 1.0;
                        lo = lo.min(x as f32);
                        hi = hi.max(x as f32 + 1.0);
                    }
                }
            }
            (n > 0.0).then(|| (sum / n + 0.5, lo, hi))
        };
        let top = centroid_x(0..band);
        let bottom = centroid_x(th - band..th);
        let pivot = match name {
            "head" => (bottom.map_or(0.5, |b| b.0 / tw as f32), 0.96),
            "body" => (bottom.map_or(0.5, |b| b.0 / tw as f32), 0.97),
            // The shoulder: the inner-top part of the arm (the arm is the left one; the right is its mirror).
            "arm" => (top.map_or(0.5, |t| (t.2 - 0.3 * (t.2 - t.1)) / tw as f32), 0.05),
            _ => (top.map_or(0.5, |t| t.0 / tw as f32), 0.04),
        };
        let height = th as f32 / char_h;
        // Scale down big pieces.
        let s = (MAX_PIECE_SIDE as f32 / tw.max(th) as f32).min(1.0);
        let pm = if s < 1.0 {
            let (nw, nh) = (((tw as f32 * s).round() as u32).max(8), ((th as f32 * s).round() as u32).max(8));
            let mut small = Pixmap::new(nw, nh).ok_or("cannot allocate the piece")?;
            let paint = PixmapPaint { quality: FilterQuality::Bilinear, ..Default::default() };
            small.draw_pixmap(0, 0, trimmed.as_ref(), &paint, Transform::from_scale(nw as f32 / tw as f32, nh as f32 / th as f32), None);
            small
        } else {
            trimmed
        };
        pieces.push(Piece { name, pm, pivot: (pivot.0.clamp(0.0, 1.0), pivot.1), height });
    }
    Ok(Cut { pieces, char_h_px: char_h })
}

#[derive(Clone, Debug)]
pub struct WriteOpts {
    /// Character height in meters.
    pub height_m: f64,
    /// Optional own style (`clay`, `rubber_hose`...).
    pub style: String,
    /// Where the illustration came from (goes in the notes).
    pub source: String,
}

impl Default for WriteOpts {
    fn default() -> Self {
        WriteOpts { height_m: 1.15, style: String::new(), source: String::new() }
    }
}

pub fn valid_name(name: &str) -> bool {
    name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Builds the character definition for a cut (parts point at `library/parts/<Name>/<part>.png`).
pub fn character_of(name: &str, cut: &Cut, opts: &WriteOpts) -> Result<CharacterDef, String> {
    if !valid_name(name) {
        return Err(format!("'{}' is not a valid character name: start with a capital letter and use only letters, digits and underscores", name));
    }
    let template = format!("character {} {{\n  rig: library.rigs.biped_cutout\n  build {{\n    body: library.bodies.kid {{ height: {} }}\n  }}\n}}\n", name, (opts.height_m / 1.15).clamp(0.3, 2.0));
    let (file, mut diags) = oneiro_lang::parse(&template, "cut.onchar");
    let mut def = CharacterDef::from_block(&file.blocks[0], "cut.onchar", &mut diags);
    def.style = opts.style.clone();
    def.voice_pitch = 180.0;
    def.notes = if opts.source.is_empty() { "Cut from an illustration with the hybrid cutter.".into() } else { format!("Cut from {} with the hybrid cutter.", opts.source) };
    let parts = cut
        .pieces
        .iter()
        .map(|p| {
            (
                p.name.to_string(),
                SvgPart {
                    file: format!("library/parts/{}/{}.png", name, p.name),
                    height: (p.height as f64).clamp(0.05, 1.0),
                    pivot: (p.pivot.0 as f64, p.pivot.1 as f64),
                    doc: None,
                    raster: None,
                },
            )
        })
        .collect();
    // The illustration already has a face: Oneiro does not draw another one on top.
    def.svg = Some(SvgRig { parts, face: false, face_x: 0.5, face_y: 0.5, face_scale: 1.0 });
    Ok(def)
}

/// Writes the PNG pieces and the `.onchar` into the project. Returns the `.onchar` path.
pub fn write_character(root: &Path, name: &str, cut: &Cut, opts: &WriteOpts) -> Result<PathBuf, String> {
    let def = character_of(name, cut, opts)?;
    let dir = root.join("library").join("parts").join(name);
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {}", dir.display(), e))?;
    for p in &cut.pieces {
        let png = p.pm.encode_png().map_err(|e| e.to_string())?;
        let path = dir.join(format!("{}.png", p.name));
        std::fs::write(&path, png).map_err(|e| format!("{}: {}", path.display(), e))?;
    }
    let out = root.join("library").join("characters").join(format!("{}.onchar", name));
    std::fs::create_dir_all(out.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&out, def.to_source()).map_err(|e| format!("{}: {}", out.display(), e))?;
    Ok(out)
}
