//! A small, safe SVG subset for vector character parts (tier B: parts drawn by an AI or by hand).
//!
//! Supported: `svg` (needs a `viewBox`), `g`, `path` (M L H V C S Q T A Z, absolute and relative),
//! `rect`, `circle`, `ellipse`, `line`, `polyline`, `polygon`; attributes `fill`, `stroke`,
//! `stroke-width`, `opacity`, `fill-opacity`, `stroke-opacity`, `transform`, `style="fill:..;stroke:.."`.
//! Not supported on purpose: scripts, images, `use`, external references, CSS, gradients, text, filters.
//! Everything is flattened into absolute paths so the renderer only has to fill and stroke them.

pub type Rgba = [u8; 4];

#[derive(Clone, Debug, PartialEq)]
pub enum Seg {
    Move(f32, f32),
    Line(f32, f32),
    Quad(f32, f32, f32, f32),
    Cubic(f32, f32, f32, f32, f32, f32),
    Close,
}

#[derive(Clone, Debug)]
pub struct Shape {
    pub segs: Vec<Seg>,
    pub fill: Option<Rgba>,
    pub stroke: Option<(Rgba, f32)>,
}

#[derive(Clone, Debug)]
pub struct SvgDoc {
    /// `viewBox`: min-x, min-y, width, height.
    pub vb: [f32; 4],
    pub shapes: Vec<Shape>,
}

pub const MAX_BYTES: usize = 200_000;
pub const MAX_SHAPES: usize = 600;
pub const MAX_SEGS: usize = 30_000;
const MAX_DEPTH: usize = 16;

impl SvgDoc {
    /// Bounding box of every point (control points included, stroke width added): (min_x, min_y, max_x, max_y).
    pub fn bounds(&self) -> Option<(f32, f32, f32, f32)> {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        let mut any = false;
        for s in &self.shapes {
            let pad = s.stroke.map_or(0.0, |(_, w)| w * 0.5);
            let mut add = |x: f32, y: f32| {
                x0 = x0.min(x - pad);
                y0 = y0.min(y - pad);
                x1 = x1.max(x + pad);
                y1 = y1.max(y + pad);
                any = true;
            };
            for g in &s.segs {
                match *g {
                    Seg::Move(x, y) | Seg::Line(x, y) => add(x, y),
                    Seg::Quad(a, b, x, y) => {
                        add(a, b);
                        add(x, y)
                    }
                    Seg::Cubic(a, b, c, d, x, y) => {
                        add(a, b);
                        add(c, d);
                        add(x, y)
                    }
                    Seg::Close => {}
                }
            }
        }
        any.then_some((x0, y0, x1, y1))
    }

    pub fn seg_count(&self) -> usize {
        self.shapes.iter().map(|s| s.segs.len()).sum()
    }

    pub fn aspect(&self) -> f32 {
        self.vb[2] / self.vb[3].max(1e-6)
    }
}

// ------------------------------------------------------------------ transforms

#[derive(Clone, Copy, Debug)]
struct Mat([f32; 6]);

impl Mat {
    const ID: Mat = Mat([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    fn mul(self, o: Mat) -> Mat {
        let [a, b, c, d, e, f] = self.0;
        let [a2, b2, c2, d2, e2, f2] = o.0;
        Mat([a * a2 + c * b2, b * a2 + d * b2, a * c2 + c * d2, b * c2 + d * d2, a * e2 + c * f2 + e, b * e2 + d * f2 + f])
    }
    fn pt(&self, x: f32, y: f32) -> (f32, f32) {
        let [a, b, c, d, e, f] = self.0;
        (a * x + c * y + e, b * x + d * y + f)
    }
    fn scale_factor(&self) -> f32 {
        let [a, b, c, d, ..] = self.0;
        (a * d - b * c).abs().sqrt()
    }
}

fn nums(s: &str) -> Vec<f32> {
    s.split(|c: char| c == ',' || c.is_whitespace()).filter(|t| !t.is_empty()).filter_map(|t| t.parse().ok()).collect()
}

fn parse_transform(s: &str, errs: &mut Vec<String>) -> Mat {
    let mut m = Mat::ID;
    let mut rest = s.trim();
    while !rest.is_empty() {
        let Some(open) = rest.find('(') else {
            errs.push(format!("bad transform '{}'", s));
            return Mat::ID;
        };
        let Some(close) = rest.find(')') else {
            errs.push(format!("bad transform '{}'", s));
            return Mat::ID;
        };
        let name = rest[..open].trim().trim_start_matches(',').trim();
        let a = nums(&rest[open + 1..close]);
        let t = match (name, a.as_slice()) {
            ("translate", [x]) => Mat([1.0, 0.0, 0.0, 1.0, *x, 0.0]),
            ("translate", [x, y]) => Mat([1.0, 0.0, 0.0, 1.0, *x, *y]),
            ("scale", [x]) => Mat([*x, 0.0, 0.0, *x, 0.0, 0.0]),
            ("scale", [x, y]) => Mat([*x, 0.0, 0.0, *y, 0.0, 0.0]),
            ("rotate", [r]) => {
                let (s, c) = r.to_radians().sin_cos();
                Mat([c, s, -s, c, 0.0, 0.0])
            }
            ("rotate", [r, cx, cy]) => {
                let (s, c) = r.to_radians().sin_cos();
                Mat([1.0, 0.0, 0.0, 1.0, *cx, *cy]).mul(Mat([c, s, -s, c, 0.0, 0.0])).mul(Mat([1.0, 0.0, 0.0, 1.0, -cx, -cy]))
            }
            ("matrix", [a, b, c, d, e, f]) => Mat([*a, *b, *c, *d, *e, *f]),
            ("skewX", [r]) => Mat([1.0, 0.0, r.to_radians().tan(), 1.0, 0.0, 0.0]),
            ("skewY", [r]) => Mat([1.0, r.to_radians().tan(), 0.0, 1.0, 0.0, 0.0]),
            _ => {
                errs.push(format!("unsupported transform '{}(...)'", name));
                Mat::ID
            }
        };
        m = m.mul(t);
        rest = rest[close + 1..].trim_start_matches(|c: char| c == ',' || c.is_whitespace());
    }
    m
}

// ------------------------------------------------------------------ paint

const NAMED: &[(&str, Rgba)] = &[
    ("black", [0, 0, 0, 255]),
    ("white", [255, 255, 255, 255]),
    ("red", [255, 0, 0, 255]),
    ("green", [0, 128, 0, 255]),
    ("lime", [0, 255, 0, 255]),
    ("blue", [0, 0, 255, 255]),
    ("yellow", [255, 255, 0, 255]),
    ("orange", [255, 165, 0, 255]),
    ("purple", [128, 0, 128, 255]),
    ("pink", [255, 192, 203, 255]),
    ("brown", [139, 69, 19, 255]),
    ("gray", [128, 128, 128, 255]),
    ("grey", [128, 128, 128, 255]),
    ("cyan", [0, 255, 255, 255]),
    ("magenta", [255, 0, 255, 255]),
    ("navy", [0, 0, 128, 255]),
    ("tan", [210, 180, 140, 255]),
    ("beige", [245, 245, 220, 255]),
];

/// `Ok(None)` is "no paint" (`none`, `transparent`).
fn parse_paint(s: &str) -> Result<Option<Rgba>, String> {
    let s = s.trim();
    let low = s.to_lowercase();
    if low == "none" || low == "transparent" {
        return Ok(None);
    }
    if low == "currentcolor" {
        return Ok(Some([0, 0, 0, 255]));
    }
    if low.starts_with("url(") {
        return Err("gradients, patterns and references (url(...)) are not supported: use flat colors".into());
    }
    if let Some(h) = low.strip_prefix('#') {
        let p = |t: &str| u8::from_str_radix(t, 16).ok();
        let v = match h.len() {
            3 => (|| Some([p(&h[0..1].repeat(2))?, p(&h[1..2].repeat(2))?, p(&h[2..3].repeat(2))?, 255]))(),
            6 => (|| Some([p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?, 255]))(),
            8 => (|| Some([p(&h[0..2])?, p(&h[2..4])?, p(&h[4..6])?, p(&h[6..8])?]))(),
            _ => None,
        };
        return v.map(Some).ok_or_else(|| format!("bad color '{}'", s));
    }
    if let Some(args) = low.strip_prefix("rgb(").and_then(|r| r.strip_suffix(')')) {
        let a = nums(args);
        if a.len() == 3 {
            return Ok(Some([a[0].clamp(0.0, 255.0) as u8, a[1].clamp(0.0, 255.0) as u8, a[2].clamp(0.0, 255.0) as u8, 255]));
        }
    }
    NAMED.iter().find(|(n, _)| *n == low).map(|(_, c)| Some(*c)).ok_or_else(|| format!("unknown color '{}': use #RRGGBB", s))
}

#[derive(Clone, Debug)]
struct Style {
    fill: Option<Rgba>,
    stroke: Option<Rgba>,
    stroke_w: f32,
    opacity: f32,
    fill_op: f32,
    stroke_op: f32,
}

impl Default for Style {
    fn default() -> Self {
        Style { fill: Some([0, 0, 0, 255]), stroke: None, stroke_w: 1.0, opacity: 1.0, fill_op: 1.0, stroke_op: 1.0 }
    }
}

fn apply_style_prop(st: &mut Style, key: &str, val: &str, errs: &mut Vec<String>) {
    let num = |v: &str| v.trim().trim_end_matches("px").parse::<f32>().ok();
    match key {
        "fill" => match parse_paint(val) {
            Ok(p) => st.fill = p,
            Err(e) => errs.push(e),
        },
        "stroke" => match parse_paint(val) {
            Ok(p) => st.stroke = p,
            Err(e) => errs.push(e),
        },
        "stroke-width" => {
            if let Some(n) = num(val) {
                st.stroke_w = n.max(0.0)
            }
        }
        "opacity" => {
            if let Some(n) = num(val) {
                st.opacity *= n.clamp(0.0, 1.0)
            }
        }
        "fill-opacity" => {
            if let Some(n) = num(val) {
                st.fill_op = n.clamp(0.0, 1.0)
            }
        }
        "stroke-opacity" => {
            if let Some(n) = num(val) {
                st.stroke_op = n.clamp(0.0, 1.0)
            }
        }
        "filter" | "mask" | "clip-path" => errs.push(format!("'{}' is not supported", key)),
        _ => {}
    }
}

// ------------------------------------------------------------------ XML

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: Vec<(String, String)>,
    closing: bool,
    selfclose: bool,
}

fn attr<'a>(t: &'a Tag, k: &str) -> Option<&'a str> {
    t.attrs.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str())
}

fn tokenize(src: &str) -> Result<Vec<Tag>, String> {
    let b = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < b.len() {
        if b[i] != b'<' {
            i += 1;
            continue;
        }
        if src[i..].starts_with("<!--") {
            i = src[i..].find("-->").map(|e| i + e + 3).ok_or("unterminated comment")?;
            continue;
        }
        if src[i..].starts_with("<?") {
            i = src[i..].find("?>").map(|e| i + e + 2).ok_or("unterminated <? ?>")?;
            continue;
        }
        if src[i..].starts_with("<![CDATA[") {
            return Err("CDATA sections are not supported".into());
        }
        if src[i..].starts_with("<!") {
            i = src[i..].find('>').map(|e| i + e + 1).ok_or("unterminated <! >")?;
            continue;
        }
        // Find the end of the tag, respecting quotes.
        let mut j = i + 1;
        let mut q: Option<u8> = None;
        while j < b.len() {
            match (q, b[j]) {
                (None, b'"') | (None, b'\'') => q = Some(b[j]),
                (Some(c), x) if c == x => q = None,
                (None, b'>') => break,
                _ => {}
            }
            j += 1;
        }
        if j >= b.len() {
            return Err("unterminated tag".into());
        }
        let inner = &src[i + 1..j];
        let (closing, inner) = match inner.strip_prefix('/') {
            Some(r) => (true, r),
            None => (false, inner),
        };
        let (selfclose, inner) = match inner.trim_end().strip_suffix('/') {
            Some(r) => (true, r),
            None => (false, inner),
        };
        let inner = inner.trim();
        let name_end = inner.find(|c: char| c.is_whitespace()).unwrap_or(inner.len());
        let name = inner[..name_end].to_string();
        let mut attrs = Vec::new();
        let mut r = inner[name_end..].trim_start();
        while !r.is_empty() {
            let Some(eq) = r.find('=') else { break };
            let key = r[..eq].trim().to_string();
            let after = r[eq + 1..].trim_start();
            let Some(qc) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                return Err(format!("attribute '{}' needs a quoted value", key));
            };
            let Some(end) = after[1..].find(qc) else { return Err("unterminated attribute value".into()) };
            attrs.push((key, after[1..1 + end].to_string()));
            r = after[end + 2..].trim_start();
        }
        out.push(Tag { name, attrs, closing, selfclose });
        i = j + 1;
    }
    Ok(out)
}

// ------------------------------------------------------------------ path data

struct PathData<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> PathData<'a> {
    fn skip(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }
    fn at_cmd(&mut self) -> Option<u8> {
        self.skip();
        self.s.get(self.i).copied().filter(|c| c.is_ascii_alphabetic())
    }
    fn number(&mut self) -> Option<f32> {
        self.skip();
        let st = self.i;
        let mut j = self.i;
        if j < self.s.len() && (self.s[j] == b'-' || self.s[j] == b'+') {
            j += 1;
        }
        let mut dot = false;
        while j < self.s.len() {
            let c = self.s[j];
            if c.is_ascii_digit() {
                j += 1;
            } else if c == b'.' && !dot {
                dot = true;
                j += 1;
            } else {
                break;
            }
        }
        if j < self.s.len() && (self.s[j] == b'e' || self.s[j] == b'E') {
            let mut k = j + 1;
            if k < self.s.len() && (self.s[k] == b'-' || self.s[k] == b'+') {
                k += 1;
            }
            if k < self.s.len() && self.s[k].is_ascii_digit() {
                while k < self.s.len() && self.s[k].is_ascii_digit() {
                    k += 1;
                }
                j = k;
            }
        }
        if j == st {
            return None;
        }
        let v = std::str::from_utf8(&self.s[st..j]).ok()?.parse::<f32>().ok()?;
        self.i = j;
        Some(v)
    }
    fn flag(&mut self) -> Option<bool> {
        self.skip();
        let c = *self.s.get(self.i)?;
        if c == b'0' || c == b'1' {
            self.i += 1;
            Some(c == b'1')
        } else {
            None
        }
    }
}

/// SVG endpoint arc -> cubic Béziers appended to `out` (absolute, untransformed coordinates).
#[allow(clippy::too_many_arguments)]
fn arc_to_cubics(out: &mut Vec<Seg>, x0: f32, y0: f32, mut rx: f32, mut ry: f32, rot: f32, large: bool, sweep: bool, x: f32, y: f32) {
    if (x0 - x).abs() < 1e-6 && (y0 - y).abs() < 1e-6 {
        return;
    }
    rx = rx.abs();
    ry = ry.abs();
    if rx < 1e-6 || ry < 1e-6 {
        out.push(Seg::Line(x, y));
        return;
    }
    let phi = rot.to_radians();
    let (sp, cp) = phi.sin_cos();
    let dx = (x0 - x) / 2.0;
    let dy = (y0 - y) / 2.0;
    let x1 = cp * dx + sp * dy;
    let y1 = -sp * dx + cp * dy;
    let lam = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lam > 1.0 {
        let s = lam.sqrt();
        rx *= s;
        ry *= s;
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = if den.abs() < 1e-12 { 0.0 } else { (num / den).max(0.0).sqrt() };
    if large == sweep {
        co = -co;
    }
    let cxp = co * rx * y1 / ry;
    let cyp = -co * ry * x1 / rx;
    let cx = cp * cxp - sp * cyp + (x0 + x) / 2.0;
    let cy = sp * cxp + cp * cyp + (y0 + y) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let a = (ux * vy - uy * vx).atan2(ux * vx + uy * vy);
        a
    };
    let th1 = ang(1.0, 0.0, (x1 - cxp) / rx, (y1 - cyp) / ry);
    let mut dth = ang((x1 - cxp) / rx, (y1 - cyp) / ry, (-x1 - cxp) / rx, (-y1 - cyp) / ry);
    if !sweep && dth > 0.0 {
        dth -= std::f32::consts::TAU;
    } else if sweep && dth < 0.0 {
        dth += std::f32::consts::TAU;
    }
    let n = (dth.abs() / (std::f32::consts::FRAC_PI_2)).ceil().max(1.0) as usize;
    let step = dth / n as f32;
    let t = 4.0 / 3.0 * (step / 4.0).tan();
    let mut a = th1;
    for _ in 0..n {
        let (s1, c1) = a.sin_cos();
        let (s2, c2) = (a + step).sin_cos();
        let p = |ex: f32, ey: f32| {
            let px = rx * ex;
            let py = ry * ey;
            (cp * px - sp * py + cx, sp * px + cp * py + cy)
        };
        let (q1x, q1y) = p(c1 - t * s1, s1 + t * c1);
        let (q2x, q2y) = p(c2 + t * s2, s2 - t * c2);
        let (ex, ey) = p(c2, s2);
        out.push(Seg::Cubic(q1x, q1y, q2x, q2y, ex, ey));
        a += step;
    }
    if let Some(Seg::Cubic(.., ex, ey)) = out.last_mut() {
        *ex = x;
        *ey = y;
    }
}

fn parse_path(d: &str) -> Result<Vec<Seg>, String> {
    let mut p = PathData { s: d.as_bytes(), i: 0 };
    let mut out: Vec<Seg> = Vec::new();
    let (mut cx, mut cy) = (0.0f32, 0.0f32);
    let (mut sx, mut sy) = (0.0f32, 0.0f32);
    let mut last_ctrl: Option<(f32, f32, bool)> = None; // (x, y, is_cubic)
    let mut cmd = 0u8;
    loop {
        if let Some(c) = p.at_cmd() {
            cmd = c;
            p.i += 1;
            if out.is_empty() && cmd.to_ascii_uppercase() != b'M' {
                return Err("path data must start with M (moveto)".into());
            }
        } else {
            p.skip();
            if p.i >= p.s.len() {
                break;
            }
            if cmd == 0 {
                return Err("path data must start with M".into());
            }
            // implicit repetition; after M the repeat is L
            if cmd == b'M' {
                cmd = b'L';
            } else if cmd == b'm' {
                cmd = b'l';
            }
        }
        let rel = cmd.is_ascii_lowercase();
        let (ox, oy) = if rel { (cx, cy) } else { (0.0, 0.0) };
        macro_rules! n {
            () => {
                p.number().ok_or_else(|| format!("bad number in path near '{}'", &d[p.i.min(d.len())..].chars().take(12).collect::<String>()))?
            };
        }
        match cmd.to_ascii_uppercase() {
            b'M' => {
                let (x, y) = (n!() + ox, n!() + oy);
                out.push(Seg::Move(x, y));
                cx = x;
                cy = y;
                sx = x;
                sy = y;
                last_ctrl = None;
            }
            b'L' => {
                let (x, y) = (n!() + ox, n!() + oy);
                out.push(Seg::Line(x, y));
                cx = x;
                cy = y;
                last_ctrl = None;
            }
            b'H' => {
                let x = n!() + ox;
                out.push(Seg::Line(x, cy));
                cx = x;
                last_ctrl = None;
            }
            b'V' => {
                let y = n!() + oy;
                out.push(Seg::Line(cx, y));
                cy = y;
                last_ctrl = None;
            }
            b'C' => {
                let (x1, y1, x2, y2, x, y) = (n!() + ox, n!() + oy, n!() + ox, n!() + oy, n!() + ox, n!() + oy);
                out.push(Seg::Cubic(x1, y1, x2, y2, x, y));
                last_ctrl = Some((x2, y2, true));
                cx = x;
                cy = y;
            }
            b'S' => {
                let (x2, y2, x, y) = (n!() + ox, n!() + oy, n!() + ox, n!() + oy);
                let (x1, y1) = match last_ctrl {
                    Some((lx, ly, true)) => (2.0 * cx - lx, 2.0 * cy - ly),
                    _ => (cx, cy),
                };
                out.push(Seg::Cubic(x1, y1, x2, y2, x, y));
                last_ctrl = Some((x2, y2, true));
                cx = x;
                cy = y;
            }
            b'Q' => {
                let (x1, y1, x, y) = (n!() + ox, n!() + oy, n!() + ox, n!() + oy);
                out.push(Seg::Quad(x1, y1, x, y));
                last_ctrl = Some((x1, y1, false));
                cx = x;
                cy = y;
            }
            b'T' => {
                let (x, y) = (n!() + ox, n!() + oy);
                let (x1, y1) = match last_ctrl {
                    Some((lx, ly, false)) => (2.0 * cx - lx, 2.0 * cy - ly),
                    _ => (cx, cy),
                };
                out.push(Seg::Quad(x1, y1, x, y));
                last_ctrl = Some((x1, y1, false));
                cx = x;
                cy = y;
            }
            b'A' => {
                let (rx, ry, rot) = (n!(), n!(), n!());
                let large = p.flag().ok_or("bad arc flag")?;
                let sweep = p.flag().ok_or("bad arc flag")?;
                let (x, y) = (n!() + ox, n!() + oy);
                arc_to_cubics(&mut out, cx, cy, rx, ry, rot, large, sweep, x, y);
                cx = x;
                cy = y;
                last_ctrl = None;
            }
            b'Z' => {
                out.push(Seg::Close);
                cx = sx;
                cy = sy;
                last_ctrl = None;
            }
            other => return Err(format!("unsupported path command '{}'", other as char)),
        }
        if out.len() > MAX_SEGS {
            return Err(format!("path has more than {} segments", MAX_SEGS));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------------ shapes

const K: f32 = 0.552_284_75;

fn ellipse_segs(cx: f32, cy: f32, rx: f32, ry: f32) -> Vec<Seg> {
    vec![
        Seg::Move(cx + rx, cy),
        Seg::Cubic(cx + rx, cy + K * ry, cx + K * rx, cy + ry, cx, cy + ry),
        Seg::Cubic(cx - K * rx, cy + ry, cx - rx, cy + K * ry, cx - rx, cy),
        Seg::Cubic(cx - rx, cy - K * ry, cx - K * rx, cy - ry, cx, cy - ry),
        Seg::Cubic(cx + K * rx, cy - ry, cx + rx, cy - K * ry, cx + rx, cy),
        Seg::Close,
    ]
}

fn rect_segs(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> Vec<Seg> {
    let (rx, ry) = (rx.min(w / 2.0), ry.min(h / 2.0));
    if rx <= 0.0 || ry <= 0.0 {
        return vec![Seg::Move(x, y), Seg::Line(x + w, y), Seg::Line(x + w, y + h), Seg::Line(x, y + h), Seg::Close];
    }
    vec![
        Seg::Move(x + rx, y),
        Seg::Line(x + w - rx, y),
        Seg::Cubic(x + w - rx + K * rx, y, x + w, y + ry - K * ry, x + w, y + ry),
        Seg::Line(x + w, y + h - ry),
        Seg::Cubic(x + w, y + h - ry + K * ry, x + w - rx + K * rx, y + h, x + w - rx, y + h),
        Seg::Line(x + rx, y + h),
        Seg::Cubic(x + rx - K * rx, y + h, x, y + h - ry + K * ry, x, y + h - ry),
        Seg::Line(x, y + ry),
        Seg::Cubic(x, y + ry - K * ry, x + rx - K * rx, y, x + rx, y),
        Seg::Close,
    ]
}

fn transform_segs(segs: &[Seg], m: &Mat) -> Vec<Seg> {
    segs.iter()
        .map(|s| match *s {
            Seg::Move(x, y) => {
                let (a, b) = m.pt(x, y);
                Seg::Move(a, b)
            }
            Seg::Line(x, y) => {
                let (a, b) = m.pt(x, y);
                Seg::Line(a, b)
            }
            Seg::Quad(x1, y1, x, y) => {
                let (a, b) = m.pt(x1, y1);
                let (c, d) = m.pt(x, y);
                Seg::Quad(a, b, c, d)
            }
            Seg::Cubic(x1, y1, x2, y2, x, y) => {
                let (a, b) = m.pt(x1, y1);
                let (c, d) = m.pt(x2, y2);
                let (e, f) = m.pt(x, y);
                Seg::Cubic(a, b, c, d, e, f)
            }
            Seg::Close => Seg::Close,
        })
        .collect()
}

fn mul_alpha(c: Rgba, k: f32) -> Rgba {
    [c[0], c[1], c[2], (c[3] as f32 * k.clamp(0.0, 1.0)).round() as u8]
}

/// Parses and flattens an SVG. On failure returns every problem found (at most 12).
pub fn parse_svg(src: &str) -> Result<SvgDoc, Vec<String>> {
    if src.len() > MAX_BYTES {
        return Err(vec![format!("the SVG is {} KB; the limit is {} KB. Simplify the drawing", src.len() / 1000, MAX_BYTES / 1000)]);
    }
    let tags = tokenize(src).map_err(|e| vec![format!("invalid XML: {}", e)])?;
    let mut errs: Vec<String> = Vec::new();
    let mut vb: Option<[f32; 4]> = None;
    let mut shapes: Vec<Shape> = Vec::new();
    // Stack of (matrix, style) per open <g>/<svg>.
    let mut stack: Vec<(Mat, Style)> = vec![(Mat::ID, Style::default())];
    let mut open_names: Vec<String> = Vec::new();
    let mut seg_total = 0usize;
    for t in &tags {
        if errs.len() >= 12 {
            break;
        }
        if t.closing {
            if matches!(t.name.as_str(), "g" | "svg") && stack.len() > 1 {
                stack.pop();
            }
            open_names.pop();
            continue;
        }
        let (pm, ps) = stack.last().cloned().unwrap();
        let mut st = ps.clone();
        let mut m = pm;
        match t.name.as_str() {
            "svg" | "g" | "path" | "rect" | "circle" | "ellipse" | "line" | "polyline" | "polygon" => {}
            "title" | "desc" | "metadata" | "defs" | "sodipodi:namedview" => {
                if !t.selfclose {
                    open_names.push(t.name.clone());
                }
                continue;
            }
            "script" | "image" | "use" | "foreignObject" | "style" | "text" | "tspan" | "filter" | "linearGradient" | "radialGradient" | "pattern" | "mask" | "clipPath" | "symbol" | "marker" | "animate" | "set" | "a" => {
                errs.push(format!("<{}> is not supported (allowed: svg, g, path, rect, circle, ellipse, line, polyline, polygon)", t.name));
                if !t.selfclose {
                    open_names.push(t.name.clone());
                }
                continue;
            }
            other => {
                errs.push(format!("unknown element <{}>", other));
                if !t.selfclose {
                    open_names.push(t.name.clone());
                }
                continue;
            }
        }
        // Presentation attributes, then the style attribute.
        for (k, v) in &t.attrs {
            if matches!(k.as_str(), "fill" | "stroke" | "stroke-width" | "opacity" | "fill-opacity" | "stroke-opacity") {
                apply_style_prop(&mut st, k, v, &mut errs);
            }
            if k == "href" || k == "xlink:href" || k.starts_with("on") {
                errs.push(format!("attribute '{}' is not allowed", k));
            }
        }
        if let Some(sty) = attr(t, "style") {
            for decl in sty.split(';') {
                if let Some((k, v)) = decl.split_once(':') {
                    apply_style_prop(&mut st, k.trim(), v.trim(), &mut errs);
                }
            }
        }
        if let Some(tr) = attr(t, "transform") {
            m = pm.mul(parse_transform(tr, &mut errs));
        }
        let f = |k: &str, d: f32| attr(t, k).and_then(|v| v.trim().trim_end_matches("px").parse::<f32>().ok()).unwrap_or(d);
        let segs: Option<Vec<Seg>> = match t.name.as_str() {
            "svg" => {
                if stack.len() == 1 && vb.is_none() {
                    match attr(t, "viewBox").map(nums) {
                        Some(v) if v.len() == 4 && v[2] > 0.0 && v[3] > 0.0 => vb = Some([v[0], v[1], v[2], v[3]]),
                        _ => errs.push("the <svg> needs a viewBox=\"minx miny width height\" with positive size".into()),
                    }
                }
                None
            }
            "g" => None,
            "path" => match attr(t, "d") {
                Some(d) => match parse_path(d) {
                    Ok(s) => Some(s),
                    Err(e) => {
                        errs.push(format!("path: {}", e));
                        None
                    }
                },
                None => {
                    errs.push("<path> without d".into());
                    None
                }
            },
            "rect" => Some(rect_segs(f("x", 0.0), f("y", 0.0), f("width", 0.0), f("height", 0.0), f("rx", f("ry", 0.0)), f("ry", f("rx", 0.0)))),
            "circle" => Some(ellipse_segs(f("cx", 0.0), f("cy", 0.0), f("r", 0.0), f("r", 0.0))),
            "ellipse" => Some(ellipse_segs(f("cx", 0.0), f("cy", 0.0), f("rx", 0.0), f("ry", 0.0))),
            "line" => Some(vec![Seg::Move(f("x1", 0.0), f("y1", 0.0)), Seg::Line(f("x2", 0.0), f("y2", 0.0))]),
            "polyline" | "polygon" => {
                let pts = nums(attr(t, "points").unwrap_or(""));
                if pts.len() < 4 || pts.len() % 2 != 0 {
                    errs.push(format!("<{}> needs an even list of at least two points", t.name));
                    None
                } else {
                    let mut v = vec![Seg::Move(pts[0], pts[1])];
                    for c in pts[2..].chunks(2) {
                        v.push(Seg::Line(c[0], c[1]));
                    }
                    if t.name == "polygon" {
                        v.push(Seg::Close);
                    }
                    Some(v)
                }
            }
            _ => None,
        };
        if let Some(segs) = segs {
            let degenerate = matches!(t.name.as_str(), "rect" | "circle" | "ellipse") && segs.iter().all(|s| matches!(s, Seg::Move(..) | Seg::Close | Seg::Line(..) | Seg::Cubic(..))) && {
                let b = SvgDoc { vb: [0.0; 4], shapes: vec![Shape { segs: segs.clone(), fill: None, stroke: None }] }.bounds();
                b.map_or(true, |(a, bb, c, d)| (c - a) < 1e-6 || (d - bb) < 1e-6)
            };
            if degenerate {
                errs.push(format!("<{}> has zero size", t.name));
            } else {
                seg_total += segs.len();
                let sf = m.scale_factor();
                let fill = st.fill.map(|c| mul_alpha(c, st.opacity * st.fill_op));
                let stroke = st.stroke.filter(|_| st.stroke_w > 0.0).map(|c| (mul_alpha(c, st.opacity * st.stroke_op), st.stroke_w * sf));
                // A line/polyline has no area: fill only makes sense for closed shapes.
                let fill = if matches!(t.name.as_str(), "line") { None } else { fill };
                if fill.is_some() || stroke.is_some() {
                    shapes.push(Shape { segs: transform_segs(&segs, &m), fill, stroke });
                }
            }
        }
        if shapes.len() > MAX_SHAPES {
            errs.push(format!("more than {} shapes; simplify the drawing", MAX_SHAPES));
            break;
        }
        if seg_total > MAX_SEGS {
            errs.push(format!("more than {} path segments; simplify the drawing", MAX_SEGS));
            break;
        }
        if matches!(t.name.as_str(), "g" | "svg") && !t.selfclose {
            if stack.len() >= MAX_DEPTH {
                errs.push("groups are nested too deeply".into());
                break;
            }
            stack.push((m, st));
        }
        if !t.selfclose && !matches!(t.name.as_str(), "g" | "svg") {
            open_names.push(t.name.clone());
        }
    }
    if vb.is_none() && errs.iter().all(|e| !e.contains("viewBox")) {
        errs.push("no <svg> element with a viewBox was found".into());
    }
    if !errs.is_empty() {
        errs.dedup();
        errs.truncate(12);
        return Err(errs);
    }
    if shapes.is_empty() {
        return Err(vec!["the SVG draws nothing: it needs at least one filled or stroked shape".into()]);
    }
    Ok(SvgDoc { vb: vb.unwrap(), shapes })
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
  <!-- a round head -->
  <g transform="translate(10 10) scale(0.8)">
    <circle cx="50" cy="50" r="45" fill="#f2c9a0" stroke="#222" stroke-width="3"/>
    <path d="M20 40 Q50 5 80 40 L80 30 Z" style="fill:#5a3a22;stroke:none"/>
  </g>
</svg>"##;

    #[test]
    fn parses_shapes_groups_transforms_and_styles() {
        let d = parse_svg(HEAD).unwrap();
        assert_eq!(d.vb, [0.0, 0.0, 100.0, 100.0]);
        assert_eq!(d.shapes.len(), 2);
        let (x0, y0, x1, y1) = d.bounds().unwrap();
        // circle: center 50 -> 10 + 50*0.8 = 50, radius 45*0.8 = 36 (+ stroke 1.2)
        assert!((x0 - (50.0 - 36.0 - 1.2)).abs() < 0.5 && (x1 - (50.0 + 36.0 + 1.2)).abs() < 0.5, "{} {}", x0, x1);
        assert!(y0 < 15.0 && y1 > 85.0);
        assert_eq!(d.shapes[0].fill, Some([0xf2, 0xc9, 0xa0, 255]));
        assert!((d.shapes[0].stroke.unwrap().1 - 2.4).abs() < 1e-3, "stroke scales with the transform");
        assert_eq!(d.shapes[1].fill, Some([0x5a, 0x3a, 0x22, 255]));
        assert!(d.shapes[1].stroke.is_none());
    }

    #[test]
    fn path_commands_relative_arcs_and_implicit_repeats() {
        let d = parse_svg(r#"<svg viewBox="0 0 10 10"><path d="m1 1 2 0 0 2h-2z M5,5 a2 2 0 1 1 3 0 S 9 9 8 8 t1 1"/></svg>"#).unwrap();
        let s = &d.shapes[0].segs;
        assert_eq!(s[0], Seg::Move(1.0, 1.0));
        assert_eq!(s[1], Seg::Line(3.0, 1.0));
        assert_eq!(s[2], Seg::Line(3.0, 3.0));
        assert_eq!(s[3], Seg::Line(1.0, 3.0));
        assert_eq!(s[4], Seg::Close);
        // the arc becomes cubics ending exactly at (8, 5)
        let arc_end = s.iter().filter_map(|g| if let Seg::Cubic(.., x, y) = g { Some((*x, *y)) } else { None }).next().unwrap();
        let _ = arc_end;
        assert!(s.iter().any(|g| matches!(g, Seg::Cubic(.., x, y) if (x - 8.0).abs() < 1e-3 && (y - 5.0).abs() < 1e-3)));
        // numbers glued together: "1.5-2.5" and ".5.5"
        let d2 = parse_svg(r#"<svg viewBox="0 0 10 10"><path d="M1.5-2.5L.5.5"/></svg>"#).unwrap();
        assert_eq!(d2.shapes[0].segs[0], Seg::Move(1.5, -2.5));
        assert_eq!(d2.shapes[0].segs[1], Seg::Line(0.5, 0.5));
    }

    #[test]
    fn rejects_dangerous_or_unsupported_content_with_clear_messages() {
        let bad = |s: &str| parse_svg(s).unwrap_err().join(" | ");
        assert!(bad(r#"<svg viewBox="0 0 1 1"><script>alert(1)</script></svg>"#).contains("<script> is not supported"));
        assert!(bad(r#"<svg viewBox="0 0 1 1"><image href="http://x/y.png"/></svg>"#).contains("<image>"));
        assert!(bad(r##"<svg viewBox="0 0 1 1"><use href="#a"/></svg>"##).contains("<use>"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" fill="url(#g)"/></svg>"#).contains("gradients"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><circle cx="5" cy="5" r="4" onclick="x()"/></svg>"#).contains("onclick"));
        assert!(bad(r#"<svg><circle cx="5" cy="5" r="4"/></svg>"#).contains("viewBox"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><path d="M 1 1 L x"/></svg>"#).contains("bad number"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><path d="L 1 1"/></svg>"#).contains("start with M"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><rect width="0" height="3"/></svg>"#).contains("zero size"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><g/></svg>"#).contains("draws nothing"));
        assert!(bad(r#"<svg viewBox="0 0 10 10"><circle r="3" fill="chartreuse2"/></svg>"#).contains("unknown color"));
        assert!(bad("<svg viewBox=\"0 0 1 1\"><circle r=\"1\"").contains("invalid XML"));
        let big = format!(r#"<svg viewBox="0 0 10 10">{}</svg>"#, "<circle cx='1' cy='1' r='1'/>".repeat(700));
        assert!(bad(&big).contains("more than"));
        assert!(bad(&"x".repeat(MAX_BYTES + 1)).contains("limit"));
    }

    #[test]
    fn rect_with_rounded_corners_and_polygons() {
        let d = parse_svg(r##"<svg viewBox="0 0 20 20"><rect x="2" y="2" width="10" height="6" rx="2" fill="#fff"/><polygon points="0,0 4,0 2,3" fill="red"/><line x1="0" y1="0" x2="5" y2="5" stroke="blue" stroke-width="2"/></svg>"##).unwrap();
        assert_eq!(d.shapes.len(), 3);
        assert!(d.shapes[0].segs.iter().any(|s| matches!(s, Seg::Cubic(..))));
        assert!(d.shapes[2].fill.is_none() && d.shapes[2].stroke.is_some(), "a line has a stroke and no fill");
    }
}
