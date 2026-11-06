//! Open sets: painted backdrops on the four sides and sprites standing in the world, so a set can be a beach,
//! a forest or a street instead of a room.

use crate::defs::SetDef;
use crate::Rgb;
use oneiro_lang::entry;
use oneiro_lang::suggest::did_you_mean;
use oneiro_lang::{Block, Diagnostic, Entry, Pos, Value};
use std::path::Path;
use std::sync::Arc;


/// A picture used as scenery: an SVG or a PNG, rasterized once when the project loads.
#[derive(Clone, Debug, Default)]
pub struct Scenery {
    pub file: String,
    pub raster: Option<Arc<crate::raster::Raster>>,
}

/// A flat picture standing in the world (a palm tree, a rock, a boat) that always faces the camera.
#[derive(Clone, Debug)]
pub struct Sprite {
    pub name: String,
    pub art: Scenery,
    pub at: [f64; 2],
    /// Height in meters.
    pub height: f64,
    /// Lift above the ground in meters (a bird, a sun).
    pub y: f64,
    pub flip: bool,
    /// `none`, `sway` (rocks about its base), `bob` (moves up and down), `pulse` (breathes), `spin` (turns in place).
    pub anim: String,
    pub amp: f64,
    pub speed: f64,
}

pub const SPRITE_ANIMS: &[&str] = &["none", "sway", "bob", "pulse", "spin"];
pub const BACKDROP_SIDES: &[&str] = &["north", "south", "east", "west"];

fn color_of(v: Option<&Value>) -> Option<Rgb> {
    match v? {
        Value::Color(c) => Rgb::from_hex(c),
        _ => None,
    }
}

fn parse_sprite(e: &Entry, file: &str, diags: &mut Vec<Diagnostic>) -> Option<Sprite> {
    let entries = match e.value()? {
        Value::Map(en) | Value::Call(_, en) => en.clone(),
        _ => {
            diags.push(Diagnostic::error("E1123", file, e.pos, format!("sprite '{}' must look like: {}: {{ image: \"scenery/x.svg\", at: (x, z), height: 3 }}", e.key, e.key)));
            return None;
        }
    };
    let num = |k: &str| entry(&entries, k).and_then(|x| x.value()).and_then(|v| v.as_num());
    let Some(image) = entry(&entries, "image").and_then(|x| x.value()).and_then(|v| v.as_str()) else {
        diags.push(Diagnostic::error("E1123", file, e.pos, format!("sprite '{}' needs an image: \"scenery/file.svg\"", e.key)));
        return None;
    };
    let at = entry(&entries, "at").and_then(|x| x.value()).and_then(|v| v.nums()).filter(|n| n.len() == 2).map(|n| [n[0], n[1]]).unwrap_or([0.0, 0.0]);
    let anim = match entry(&entries, "anim").and_then(|x| x.value()).and_then(|v| v.as_str()) {
        None => "none".to_string(),
        Some(a) if SPRITE_ANIMS.contains(&a) => a.to_string(),
        Some(a) => {
            diags.push(Diagnostic::error("E1124", file, e.pos, format!("unknown sprite animation '{}'", a)).suggest(did_you_mean(a, SPRITE_ANIMS.iter().copied())));
            "none".to_string()
        }
    };
    Some(Sprite {
        name: e.key.clone(),
        art: Scenery { file: image.to_string(), raster: None },
        at,
        height: num("height").unwrap_or(2.0).clamp(0.05, 60.0),
        y: num("y").unwrap_or(0.0),
        flip: entry(&entries, "flip").and_then(|x| x.value()).and_then(|v| v.as_bool()).unwrap_or(false),
        anim,
        amp: num("amount").unwrap_or(1.0).clamp(0.0, 4.0),
        speed: num("speed").unwrap_or(1.0).clamp(0.05, 8.0),
    })
}

impl SetDef {
    /// Reads `kind`, `sky`, `ground`, `backdrops` and `sprites` of a set block.
    pub fn parse_open(&mut self, b: &Block, file: &str, diags: &mut Vec<Diagnostic>) {
        for e in &b.entries {
            match e.key.as_str() {
                "kind" => match e.value().and_then(|v| v.as_str()) {
                    Some("open") => {
                        self.open = true;
                        if !b.entries.iter().any(|x| x.key == "size") {
                            self.width = 16.0;
                            self.depth = 12.0;
                        }
                    }
                    Some("room") => self.open = false,
                    _ => diags.push(Diagnostic::error("E1125", file, e.pos, "set kind must be room or open").suggest("kind: open")),
                },
                "sky" => {
                    let cols: Vec<Rgb> = match e.value() {
                        Some(Value::List(v)) | Some(Value::Tuple(v)) => v.iter().filter_map(|x| color_of(Some(x))).collect(),
                        v => color_of(v).into_iter().collect(),
                    };
                    match cols.len() {
                        1 => self.sky = (cols[0], cols[0]),
                        2 => self.sky = (cols[0], cols[1]),
                        _ => diags.push(Diagnostic::error("E1125", file, e.pos, "sky must be a color or two colors: sky: [#5BB8F0, #FFF1C9] (top, horizon)")),
                    }
                }
                "ground" => {
                    if let Some(c) = color_of(e.value()) {
                        self.ground = c;
                    }
                }
                "backdrops" => {
                    for be in e.block().unwrap_or(&[]) {
                        if !BACKDROP_SIDES.contains(&be.key.as_str()) {
                            diags.push(Diagnostic::error("E1125", file, be.pos, format!("unknown backdrop side '{}'", be.key)).suggest("sides: north, south, east, west"));
                        } else if let Some(f) = be.value().and_then(|v| v.as_str()) {
                            self.backdrops.insert(be.key.clone(), Scenery { file: f.to_string(), raster: None });
                        }
                    }
                }
                "solids" => {
                    for se in e.block().unwrap_or(&[]) {
                        if let Some(sd) = parse_solid(se, file, diags) {
                            self.solids.push(sd);
                        }
                    }
                }
                "sprites" => {
                    for se in e.block().unwrap_or(&[]) {
                        if let Some(sp) = parse_sprite(se, file, diags) {
                            self.sprites.push(sp);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Reads the pictures (SVG or PNG) of the backdrops and sprites; paths are relative to `root`.
    pub fn load_scenery(&mut self, root: &Path, label: &str, diags: &mut Vec<Diagnostic>) {
        let at = Pos { line: 1, col: 1 };
        fn load(root: &Path, art: &mut Scenery, what: &str, tall: u32, label: &str, at: Pos, diags: &mut Vec<Diagnostic>) {
            let file = art.file.clone();
            if Path::new(&file).is_absolute() || file.split(['/', '\\']).any(|s| s == "..") {
                diags.push(Diagnostic::error("E1126", label, at, format!("{}: the picture must be inside the project (no absolute paths or '..')", what)));
                return;
            }
            let path = root.join(&file);
            let res: Result<crate::raster::Raster, String> = if file.to_lowercase().ends_with(".png") {
                std::fs::read(&path).map_err(|e| format!("cannot read {}: {}", file, e)).and_then(|b| crate::raster::Raster::from_png(&b))
            } else {
                std::fs::read_to_string(&path)
                    .map_err(|e| format!("cannot read {}: {}", file, e))
                    .and_then(|t| crate::svg::parse_svg(&t).map_err(|e| format!("{}: {}", file, e.join("; "))))
                    .and_then(|d| d.rasterize(tall).ok_or_else(|| format!("{}: could not draw it", file)))
                    .map(|pm| crate::raster::Raster { pm })
            };
            match res {
                Ok(r) => art.raster = Some(Arc::new(r)),
                Err(e) => diags.push(Diagnostic::error("E1126", label, at, format!("{}: {}", what, e))),
            }
        }
        for (side, art) in self.backdrops.iter_mut() {
            load(root, art, &format!("backdrop {}", side), 900, label, at, diags);
        }
        for sp in self.sprites.iter_mut() {
            load(root, &mut sp.art, &format!("sprite {}", sp.name), 640, label, at, diags);
        }
    }
}

// ------------------------------------------------------------------ solids: 3D shapes built by hand or by the AI

pub const SOLID_SHAPES: &[&str] = &["box", "cylinder", "cone", "sphere", "roof", "pyramid"];

/// A 3D shape standing in the set. Many of them make a house, a tower, a boat or a stage: real geometry, so any
/// camera side sees it correctly.
#[derive(Clone, Debug)]
pub struct Solid {
    pub name: String,
    /// `box`, `cylinder`, `cone`, `sphere`, `roof` (a gabled roof), `pyramid`.
    pub shape: String,
    pub at: [f64; 2],
    /// Height of its base above the ground.
    pub y: f64,
    /// Width (x), height (y) and depth (z) in meters, before `yaw`.
    pub size: [f64; 3],
    pub color: Rgb,
    /// Rotation about the vertical axis, radians.
    pub yaw: f64,
    /// Characters walk around it (boxes and cylinders only).
    pub solid: bool,
}

pub fn parse_solid(e: &Entry, file: &str, diags: &mut Vec<Diagnostic>) -> Option<Solid> {
    let entries = match e.value()? {
        Value::Map(en) | Value::Call(_, en) => en.clone(),
        _ => {
            diags.push(Diagnostic::error("E1150", file, e.pos, format!("solid '{}' must look like: {}: {{ shape: box, at: (x, z), size: (w, h, d), color: #AA7744 }}", e.key, e.key)));
            return None;
        }
    };
    let shape = entry(&entries, "shape").and_then(|x| x.value()).and_then(|v| v.as_str()).unwrap_or("box").to_string();
    if !SOLID_SHAPES.contains(&shape.as_str()) {
        diags.push(Diagnostic::error("E1151", file, e.pos, format!("unknown shape '{}'", shape)).suggest(did_you_mean(&shape, SOLID_SHAPES.iter().copied())));
        return None;
    }
    let num = |k: &str| entry(&entries, k).and_then(|x| x.value()).and_then(|v| v.as_num());
    let at = entry(&entries, "at").and_then(|x| x.value()).and_then(|v| v.nums()).filter(|n| n.len() == 2).map(|n| [n[0], n[1]]).unwrap_or([0.0, 0.0]);
    let size = match entry(&entries, "size").and_then(|x| x.value()).and_then(|v| v.nums()) {
        Some(n) if n.len() == 3 => [n[0], n[1], n[2]],
        Some(n) if n.len() == 1 => [n[0], n[0], n[0]],
        None => [1.0, 1.0, 1.0],
        _ => {
            diags.push(Diagnostic::error("E1152", file, e.pos, format!("solid '{}': size must be (width, height, depth) in meters", e.key)));
            return None;
        }
    };
    if size.iter().any(|s| !(0.01..=200.0).contains(s)) {
        diags.push(Diagnostic::error("E1152", file, e.pos, format!("solid '{}': every size must be between 0.01 and 200 meters", e.key)));
        return None;
    }
    let color = color_of(entry(&entries, "color").and_then(|x| x.value())).unwrap_or(Rgb(196, 190, 180));
    let solid_default = matches!(shape.as_str(), "box" | "cylinder") && size[1] > 0.4;
    Some(Solid {
        name: e.key.clone(),
        shape,
        at,
        y: num("y").unwrap_or(0.0),
        size,
        color,
        yaw: num("yaw").unwrap_or(0.0).to_radians(),
        solid: entry(&entries, "solid").and_then(|x| x.value()).and_then(|v| v.as_bool()).unwrap_or(solid_default),
    })
}
