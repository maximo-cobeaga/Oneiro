//! Character and set definitions parsed from `.onchar` / `.onset` blocks (or inline in scripts).

use crate::catalog::*;
use crate::math::Rgb;
use oneiro_lang::suggest::did_you_mean;
use oneiro_lang::{entry, Block, Diagnostic, Entry, Pos, Value};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default)]
pub struct PartDef {
    pub kind: String,
    pub nums: BTreeMap<String, f64>,
    pub colors: BTreeMap<String, Rgb>,
}

impl PartDef {
    pub fn new(kind: &str) -> PartDef {
        PartDef { kind: kind.to_string(), ..Default::default() }
    }
    pub fn num(&self, k: &str, default: f64) -> f64 {
        self.nums.get(k).copied().unwrap_or(default)
    }
    pub fn color(&self, k: &str, default: Rgb) -> Rgb {
        self.colors.get(k).copied().unwrap_or(default)
    }
}

#[derive(Clone, Debug)]
pub struct CharacterDef {
    pub name: String,
    pub rig: String,
    pub head: PartDef,
    pub eyes: PartDef,
    pub mouth: PartDef,
    pub body: PartDef,
    pub wardrobe: Vec<String>,
    pub voice_provider: String,
    pub voice_pitch: f64,
    /// Provider-specific voice (ElevenLabs voice id, SAPI voice name, OpenAI voice...). Empty: provider default.
    pub voice_id: String,
    /// Speaking speed multiplier (1.0 = normal).
    pub voice_speed: f64,
    pub traits: Vec<String>,
    pub notes: String,
    pub styles: BTreeMap<String, String>,
}

impl CharacterDef {
    /// Total height in meters.
    pub fn height(&self) -> f64 {
        let base = match self.body.kind.as_str() {
            "adult" => 1.75,
            "round" => 1.2,
            _ => 1.15,
        };
        base * self.body.num("height", 1.0)
    }
    pub fn skin(&self) -> Rgb {
        self.head.color("skin_tone", Rgb(242, 201, 160))
    }

    pub fn from_block(b: &Block, file: &str, diags: &mut Vec<Diagnostic>) -> CharacterDef {
        let mut c = CharacterDef {
            name: b.name.clone().unwrap_or_default(),
            rig: "biped_cutout".into(),
            head: PartDef::new("round"),
            eyes: PartDef::new("dots"),
            mouth: PartDef::new("visemes_basic"),
            body: PartDef::new("kid"),
            wardrobe: Vec::new(),
            voice_provider: "file".into(),
            voice_pitch: 160.0,
            voice_id: String::new(),
            voice_speed: 1.0,
            traits: Vec::new(),
            notes: String::new(),
            styles: BTreeMap::new(),
        };
        if b.name.is_none() {
            diags.push(
                Diagnostic::error("E1100", file, b.pos, "character block has no name")
                    .suggest("write: character Marta { ... }"),
            );
        }
        for e in &b.entries {
            match e.key.as_str() {
                "rig" => {
                    if let Some(v) = e.value().and_then(|v| v.as_str()) {
                        let kind = v.rsplit('.').next().unwrap_or(v);
                        if !RIGS.contains(&kind) {
                            diags.push(
                                Diagnostic::error("E1101", file, e.pos, format!("unknown rig '{}'", v))
                                    .suggest(did_you_mean(kind, RIGS.iter().copied())),
                            );
                        }
                        c.rig = kind.to_string();
                    }
                }
                "build" => {
                    if let Some(inner) = e.block() {
                        for pe in inner {
                            let slot = pe.key.as_str();
                            let (list, target): (&[&str], &mut PartDef) = match slot {
                                "head" => (HEADS, &mut c.head),
                                "eyes" => (EYES, &mut c.eyes),
                                "mouth" => (MOUTHS, &mut c.mouth),
                                "body" => (BODIES, &mut c.body),
                                other => {
                                    diags.push(
                                        Diagnostic::error("E1102", file, pe.pos, format!("unknown build slot '{}'", other))
                                            .suggest("valid slots: head, eyes, mouth, body"),
                                    );
                                    continue;
                                }
                            };
                            if let Some(p) = part_from_value(pe.value()) {
                                if !list.contains(&p.kind.as_str()) {
                                    diags.push(
                                        Diagnostic::error(
                                            "E1103",
                                            file,
                                            pe.pos,
                                            format!("unknown {} part '{}'", slot, p.kind),
                                        )
                                        .suggest(did_you_mean(&p.kind, list.iter().copied())),
                                    );
                                } else {
                                    *target = p;
                                }
                            }
                        }
                    }
                }
                "wardrobe" => {
                    if let Some(inner) = e.block() {
                        for we in inner {
                            if let Some(v) = we.value() {
                                for w in v.words() {
                                    if wardrobe_item(&w).is_none() {
                                        diags.push(
                                            Diagnostic::error("E1104", file, we.pos, format!("unknown wardrobe item '{}'", w))
                                                .suggest(did_you_mean(&w, wardrobe_names())),
                                        );
                                    } else {
                                        c.wardrobe.push(w);
                                    }
                                }
                            }
                        }
                    }
                }
                "voice" => {
                    if let Some(inner) = e.block() {
                        if let Some(p) = entry(inner, "provider").and_then(|x| x.value()).and_then(|v| v.as_str()) {
                            c.voice_provider = p.to_string();
                        }
                        if let Some(p) = entry(inner, "pitch").and_then(|x| x.value()).and_then(|v| v.as_num()) {
                            c.voice_pitch = p;
                        }
                        if let Some(p) = entry(inner, "voice").and_then(|x| x.value()).and_then(|v| v.as_str()) {
                            c.voice_id = p.to_string();
                        }
                        if let Some(p) = entry(inner, "speed").and_then(|x| x.value()).and_then(|v| v.as_num()) {
                            c.voice_speed = p.clamp(0.5, 2.0);
                        }
                    }
                }
                "sheet" => {
                    if let Some(inner) = e.block() {
                        if let Some(v) = entry(inner, "traits").and_then(|x| x.value()) {
                            c.traits = v.words();
                        }
                        if let Some(v) = entry(inner, "notes").and_then(|x| x.value()).and_then(|v| v.as_str()) {
                            c.notes = v.to_string();
                        }
                    }
                }
                "styles" => {
                    if let Some(inner) = e.block() {
                        for se in inner {
                            if let Some(v) = se.value().and_then(|v| v.as_str()) {
                                c.styles.insert(se.key.clone(), v.to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        c
    }
}

fn part_from_value(v: Option<&Value>) -> Option<PartDef> {
    match v? {
        Value::Call(path, entries) => {
            let mut p = PartDef::new(path.rsplit('.').next().unwrap_or(path));
            for e in entries {
                match e.value() {
                    Some(Value::Num(n, _)) => {
                        p.nums.insert(e.key.clone(), *n);
                    }
                    Some(Value::Color(c)) => {
                        if let Some(rgb) = Rgb::from_hex(c) {
                            p.colors.insert(e.key.clone(), rgb);
                        }
                    }
                    _ => {}
                }
            }
            Some(p)
        }
        Value::Ident(path) => Some(PartDef::new(path.rsplit('.').next().unwrap_or(path))),
        _ => None,
    }
}

// ------------------------------------------------------------------ sets

#[derive(Clone, Debug)]
pub struct Anchor {
    /// x (east), y (up), z (north)
    pub pos: [f64; 3],
    pub seat: bool,
    pub seat_height: f64,
    /// Yaw to face when arriving/placed here.
    pub facing: Option<f64>,
    /// Characters stop short of solid anchors (counters, walls).
    pub solid: bool,
}

#[derive(Clone, Debug)]
pub struct PropInst {
    pub name: String,
    pub kind: String,
    pub at: [f64; 2],
    pub wall: Option<String>,
    pub along: f64,
    pub y: f64,
    pub yaw: f64,
    pub w: Option<f64>,
    pub h: Option<f64>,
    pub d: Option<f64>,
    pub color: Option<Rgb>,
}

#[derive(Clone, Debug)]
pub struct SetDef {
    pub name: String,
    pub width: f64,
    pub depth: f64,
    pub height: f64,
    pub anchors: BTreeMap<String, Anchor>,
    pub props: Vec<PropInst>,
    pub wall: Rgb,
    pub wall_alt: Rgb,
    pub floor: Rgb,
    pub floor_alt: Rgb,
    pub trim: Rgb,
}

pub fn dir_yaw(name: &str) -> Option<f64> {
    use std::f64::consts::PI;
    Some(match name {
        "north" => 0.0,
        "east" => PI / 2.0,
        "south" => PI,
        "west" => -PI / 2.0,
        _ => return None,
    })
}

fn color_of(v: Option<&Value>) -> Option<Rgb> {
    match v? {
        Value::Color(c) => Rgb::from_hex(c),
        _ => None,
    }
}

impl SetDef {
    pub fn from_block(b: &Block, file: &str, diags: &mut Vec<Diagnostic>) -> SetDef {
        let mut s = SetDef {
            name: b.name.clone().unwrap_or_default(),
            width: 8.0,
            depth: 6.0,
            height: 3.0,
            anchors: BTreeMap::new(),
            props: Vec::new(),
            wall: Rgb(236, 224, 196),
            wall_alt: Rgb(226, 212, 182),
            floor: Rgb(222, 216, 200),
            floor_alt: Rgb(52, 52, 66),
            trim: Rgb(150, 40, 48),
        };
        for e in &b.entries {
            match e.key.as_str() {
                "size" => {
                    if let Some(n) = e.value().and_then(|v| v.nums()) {
                        if n.len() >= 2 {
                            s.width = n[0];
                            s.depth = n[1];
                        }
                    }
                }
                "height" => {
                    if let Some(n) = e.value().and_then(|v| v.as_num()) {
                        s.height = n;
                    }
                }
                "palette" => {
                    if let Some(inner) = e.block() {
                        for pe in inner {
                            if let Some(c) = color_of(pe.value()) {
                                match pe.key.as_str() {
                                    "wall" => s.wall = c,
                                    "wall_alt" => s.wall_alt = c,
                                    "floor" => s.floor = c,
                                    "floor_alt" => s.floor_alt = c,
                                    "trim" => s.trim = c,
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                "anchors" => {
                    if let Some(inner) = e.block() {
                        for ae in inner {
                            if let Some(a) = parse_anchor(ae, file, diags) {
                                s.anchors.insert(ae.key.clone(), a);
                            }
                        }
                    }
                }
                "props" => {
                    if let Some(inner) = e.block() {
                        for pe in inner {
                            if let Some(p) = parse_prop(pe, file, diags) {
                                s.props.push(p);
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        s
    }
}

fn parse_anchor(e: &Entry, file: &str, diags: &mut Vec<Diagnostic>) -> Option<Anchor> {
    let mut a = Anchor { pos: [0.0; 3], seat: false, seat_height: 0.62, facing: None, solid: false };
    let set_pos = |a: &mut Anchor, v: &Value| -> bool {
        match v.nums() {
            Some(n) if n.len() == 2 => {
                a.pos = [n[0], 0.0, n[1]];
                true
            }
            Some(n) if n.len() == 3 => {
                a.pos = [n[0], n[1], n[2]];
                true
            }
            _ => false,
        }
    };
    match e.value()? {
        v @ Value::Tuple(_) => {
            if !set_pos(&mut a, v) {
                diags.push(Diagnostic::error("E1110", file, e.pos, "anchor position must be (x, z) or (x, y, z)"));
                return None;
            }
        }
        Value::Map(entries) | Value::Call(_, entries) => {
            match entry(entries, "pos").and_then(|x| x.value()) {
                Some(v) if set_pos(&mut a, v) => {}
                _ => {
                    diags.push(
                        Diagnostic::error("E1110", file, e.pos, format!("anchor '{}' needs a 'pos: (x, z)'", e.key)),
                    );
                    return None;
                }
            }
            if let Some(b) = entry(entries, "seat").and_then(|x| x.value()).and_then(|v| v.as_bool()) {
                a.seat = b;
            }
            if let Some(b) = entry(entries, "solid").and_then(|x| x.value()).and_then(|v| v.as_bool()) {
                a.solid = b;
            }
            if let Some(h) = entry(entries, "seat_height").and_then(|x| x.value()).and_then(|v| v.as_num()) {
                a.seat_height = h;
            }
            if let Some(f) = entry(entries, "facing").and_then(|x| x.value()) {
                match f {
                    Value::Ident(n) => match dir_yaw(n) {
                        Some(y) => a.facing = Some(y),
                        None => diags.push(
                            Diagnostic::error("E1111", file, e.pos, format!("unknown facing '{}'", n))
                                .suggest("use north, south, east, west or an angle like 90deg"),
                        ),
                    },
                    Value::Num(d, _) => a.facing = Some(d.to_radians()),
                    _ => {}
                }
            }
        }
        _ => {
            diags.push(Diagnostic::error("E1110", file, e.pos, "anchor must be (x, z) or { pos: (x, z), ... }"));
            return None;
        }
    }
    Some(a)
}

fn parse_prop(e: &Entry, file: &str, diags: &mut Vec<Diagnostic>) -> Option<PropInst> {
    let (path, entries): (String, Vec<Entry>) = match e.value()? {
        Value::Call(p, en) => (p.clone(), en.clone()),
        Value::Ident(p) => (p.clone(), Vec::new()),
        _ => {
            diags.push(Diagnostic::error("E1112", file, e.pos, "prop must look like: name: library.props.kind { at: (x, z) }"));
            return None;
        }
    };
    let kind = path.rsplit('.').next().unwrap_or(&path).to_string();
    if !PROP_KINDS.contains(&kind.as_str()) {
        diags.push(
            Diagnostic::error("E1113", file, e.pos, format!("unknown prop kind '{}'", kind))
                .suggest(did_you_mean(&kind, PROP_KINDS.iter().copied())),
        );
        return None;
    }
    let mut p = PropInst {
        name: e.key.clone(),
        kind,
        at: [0.0, 0.0],
        wall: None,
        along: 0.0,
        y: 0.0,
        yaw: 0.0,
        w: None,
        h: None,
        d: None,
        color: None,
    };
    let num = |k: &str| entry(&entries, k).and_then(|x| x.value()).and_then(|v| v.as_num());
    match entry(&entries, "at").and_then(|x| x.value()) {
        Some(Value::Num(n, _)) => p.along = *n,
        Some(v) => {
            if let Some(n) = v.nums() {
                if n.len() >= 2 {
                    p.at = [n[0], n[1]];
                }
            }
        }
        None => {}
    }
    if let Some(Value::Ident(w)) = entry(&entries, "wall").and_then(|x| x.value()) {
        if dir_yaw(w).is_none() {
            diags.push(Diagnostic::error("E1114", file, e.pos, format!("unknown wall '{}'", w)).suggest("use north, south, east or west"));
        } else {
            p.wall = Some(w.clone());
        }
    }
    if let Some(h) = num("height_at") {
        p.y = h;
    }
    if let Some(y) = num("yaw") {
        p.yaw = y.to_radians();
    }
    p.w = num("width");
    p.h = num("height");
    p.d = num("depth");
    p.color = color_of(entry(&entries, "color").and_then(|x| x.value()));
    Some(p)
}

#[allow(dead_code)]
fn _unused(_: Pos) {}

/// Body proportions in meters, shared by the renderer and the camera solver.
#[derive(Clone, Copy, Debug)]
pub struct Dims {
    pub leg: f64,
    pub torso_h: f64,
    pub torso_w: f64,
    pub head_r: f64,
    pub arm_len: f64,
    pub neck: f64,
}

impl CharacterDef {
    pub fn dims(&self) -> Dims {
        let h = self.height();
        let (leg_f, torso_f, tw_f, head_f, arm_f) = match self.body.kind.as_str() {
            "adult" => (0.44, 0.30, 0.24, 0.105, 0.30),
            "round" => (0.16, 0.36, 0.44, 0.19, 0.24),
            _ => (0.20, 0.30, 0.30, 0.22, 0.26),
        };
        let norm = 1.0 / (leg_f + torso_f + 0.02 + 2.0 * head_f);
        Dims {
            leg: leg_f * norm * h,
            torso_h: torso_f * norm * h,
            torso_w: tw_f * norm * h * self.body.num("width", 1.0),
            head_r: head_f * norm * h * self.head.num("size", 1.0),
            arm_len: arm_f * norm * h,
            neck: 0.02 * norm * h,
        }
    }
}

impl PartDef {
    /// `{ size: 1.0, skin_tone: #F2C9A0 }` or an empty string when there are no parameters.
    fn params_source(&self) -> String {
        let mut items: Vec<String> = Vec::new();
        for (k, v) in &self.nums {
            items.push(format!("{}: {}", k, trim_num(*v)));
        }
        for (k, c) in &self.colors {
            items.push(format!("{}: {}", k, c.hex()));
        }
        if items.is_empty() {
            String::new()
        } else {
            format!(" {{ {} }}", items.join(", "))
        }
    }
}

fn trim_num(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".into()
    } else {
        s.to_string()
    }
}

impl CharacterDef {
    /// Serializes the character back to `.onchar` source (used by the IDE character builder).
    pub fn to_source(&self) -> String {
        let mut s = String::new();
        s.push_str(&format!("character {} {{\n", self.name));
        s.push_str(&format!("  rig: library.rigs.{}\n", self.rig));
        s.push_str("  build {\n");
        s.push_str(&format!("    head:  library.heads.{}{}\n", self.head.kind, self.head.params_source()));
        s.push_str(&format!("    eyes:  library.eyes.{}{}\n", self.eyes.kind, self.eyes.params_source()));
        s.push_str(&format!("    mouth: library.mouths.{}{}\n", self.mouth.kind, self.mouth.params_source()));
        s.push_str(&format!("    body:  library.bodies.{}{}\n", self.body.kind, self.body.params_source()));
        s.push_str("  }\n");
        if !self.wardrobe.is_empty() {
            s.push_str(&format!("  wardrobe {{\n    default: [{}]\n  }}\n", self.wardrobe.join(", ")));
        }
        let mut voice = format!("provider: {}, pitch: {}", self.voice_provider, trim_num(self.voice_pitch));
        if !self.voice_id.is_empty() {
            voice.push_str(&format!(", voice: \"{}\"", self.voice_id.replace('"', "")));
        }
        if (self.voice_speed - 1.0).abs() > 1e-9 {
            voice.push_str(&format!(", speed: {}", trim_num(self.voice_speed)));
        }
        s.push_str(&format!("  voice {{ {} }}\n", voice));
        if !self.traits.is_empty() || !self.notes.is_empty() {
            s.push_str("  sheet {\n");
            if !self.traits.is_empty() {
                s.push_str(&format!("    traits: [{}]\n", self.traits.join(", ")));
            }
            if !self.notes.is_empty() {
                s.push_str(&format!("    notes: \"{}\"\n", self.notes.replace('"', "'")));
            }
            s.push_str("  }\n");
        }
        if self.styles.is_empty() {
            s.push_str("  styles { cutout_2d: auto }\n");
        } else {
            let items: Vec<String> = self.styles.iter().map(|(k, v)| format!("{}: {}", k, v)).collect();
            s.push_str(&format!("  styles {{ {} }}\n", items.join(", ")));
        }
        s.push_str("}\n");
        s
    }
}
