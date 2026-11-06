//! The four boxes that say where each part of the character is, a heuristic that proposes them, and
//! the validator that tells a person (or an AI) what is wrong with a proposal.

use crate::mask::Mask;

pub const PARTS: [&str; 4] = ["head", "body", "arm", "leg"];

/// A rectangle in normalized image coordinates (0..1, origin top-left).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RectN {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl RectN {
    pub fn new(x0: f32, y0: f32, x1: f32, y1: f32) -> RectN {
        RectN { x0, y0, x1, y1 }
    }
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn h(&self) -> f32 {
        self.y1 - self.y0
    }
    pub fn center(&self) -> (f32, f32) {
        ((self.x0 + self.x1) * 0.5, (self.y0 + self.y1) * 0.5)
    }
    pub fn contains(&self, x: f32, y: f32) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
    /// The same box mirrored around the vertical line `cx`.
    pub fn mirrored(&self, cx: f32) -> RectN {
        RectN { x0: 2.0 * cx - self.x1, y0: self.y0, x1: 2.0 * cx - self.x0, y1: self.y1 }
    }
    pub fn clamped(&self) -> RectN {
        RectN { x0: self.x0.clamp(0.0, 1.0), y0: self.y0.clamp(0.0, 1.0), x1: self.x1.clamp(0.0, 1.0), y1: self.y1.clamp(0.0, 1.0) }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub head: RectN,
    pub body: RectN,
    /// The character's left arm as seen on screen (the right arm is its mirror).
    pub arm: RectN,
    /// The left leg as seen on screen (the right leg is its mirror).
    pub leg: RectN,
}

impl Layout {
    pub fn rects(&self) -> [(&'static str, RectN); 4] {
        [("head", self.head), ("body", self.body), ("arm", self.arm), ("leg", self.leg)]
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut RectN> {
        match name {
            "head" => Some(&mut self.head),
            "body" => Some(&mut self.body),
            "arm" => Some(&mut self.arm),
            "leg" => Some(&mut self.leg),
            _ => None,
        }
    }

    /// `{"head":[x0,y0,x1,y1], "body":[...], "arm":[...], "leg":[...]}` with coordinates 0..1.
    pub fn to_json(&self) -> String {
        let r = |r: RectN| format!("[{:.4}, {:.4}, {:.4}, {:.4}]", r.x0, r.y0, r.x1, r.y1);
        format!("{{\"head\": {}, \"body\": {}, \"arm\": {}, \"leg\": {}}}", r(self.head), r(self.body), r(self.arm), r(self.leg))
    }

    /// Parses the JSON above. It may be surrounded by other text (a model's reply): the first `{...}` is used.
    pub fn from_json(text: &str) -> Result<Layout, String> {
        let start = text.find('{').ok_or("no JSON object found")?;
        let end = text.rfind('}').ok_or("no JSON object found")?;
        let v: serde_json::Value = serde_json::from_str(&text[start..=end]).map_err(|e| format!("invalid JSON: {}", e))?;
        let rect = |name: &str| -> Result<RectN, String> {
            let a = v.get(name).and_then(|x| x.as_array()).ok_or(format!("missing \"{}\": [x0, y0, x1, y1]", name))?;
            let n: Vec<f32> = a.iter().filter_map(|x| x.as_f64().map(|f| f as f32)).collect();
            if n.len() != 4 {
                return Err(format!("\"{}\" needs exactly four numbers [x0, y0, x1, y1]", name));
            }
            if n.iter().any(|x| !x.is_finite()) {
                return Err(format!("\"{}\" has an invalid number", name));
            }
            Ok(RectN::new(n[0].min(n[2]), n[1].min(n[3]), n[0].max(n[2]), n[1].max(n[3])))
        };
        Ok(Layout { head: rect("head")?, body: rect("body")?, arm: rect("arm")?, leg: rect("leg")? })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Debug)]
pub struct Issue {
    pub severity: Severity,
    pub code: &'static str,
    pub message: String,
    pub part: Option<&'static str>,
}

impl Issue {
    fn err(code: &'static str, part: Option<&'static str>, message: String) -> Issue {
        Issue { severity: Severity::Error, code, message, part }
    }
    fn warn(code: &'static str, part: Option<&'static str>, message: String) -> Issue {
        Issue { severity: Severity::Warning, code, message, part }
    }
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
    pub fn render(&self) -> String {
        format!("{}[{}] {}", if self.is_error() { "error" } else { "warning" }, self.code, self.message)
    }
}

// ------------------------------------------------------------------ the heuristic

fn runs(mask: &Mask, y: usize, min_len: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    for x in 0..mask.w {
        let s = mask.solid(x, y);
        match (s, start) {
            (true, None) => start = Some(x),
            (false, Some(a)) => {
                if x - a >= min_len {
                    out.push((a, x));
                }
                start = None;
            }
            _ => {}
        }
    }
    if let Some(a) = start {
        if mask.w - a >= min_len {
            out.push((a, mask.w));
        }
    }
    out
}

fn extent(mask: &Mask, y0: usize, y1: usize) -> Option<(usize, usize)> {
    let (mut lo, mut hi) = (usize::MAX, 0usize);
    for y in y0..y1.min(mask.h) {
        for x in 0..mask.w {
            if mask.solid(x, y) {
                lo = lo.min(x);
                hi = hi.max(x + 1);
            }
        }
    }
    (hi > lo).then_some((lo, hi))
}

/// Proposes the four boxes for a front-facing, standing character: finds the neck (narrowest row under
/// the head), the hips (where the legs separate) and the torso (the central run of the body rows).
pub fn propose(mask: &Mask) -> Result<Layout, String> {
    let (bx0, by0, bx1, by1) = mask.bounds().ok_or("the mask is empty")?;
    let (bx0, by0, bx1, by1) = (bx0 as usize, by0 as usize, bx1 as usize, by1 as usize);
    let h = (by1 - by0) as f32;
    let width = |y: usize| (0..mask.w).filter(|x| mask.solid(*x, y)).count() as f32;
    let smooth = |y: usize| {
        let (a, b) = (y.saturating_sub(2), (y + 3).min(mask.h));
        (a..b).map(width).sum::<f32>() / (b - a) as f32
    };
    let row = |f: f32| by0 + ((h * f) as usize).min(by1 - by0 - 1);

    // Neck: the narrowest row below the widest part of the head (which may be a big round head,
    // up to half the height), if it is clearly narrower than the head.
    let (mut head_row, mut head_w) = (by0, 0.0f32);
    for y in by0..=row(0.50) {
        let w = smooth(y);
        if w > head_w {
            head_w = w;
            head_row = y;
        }
    }
    let (mut neck, mut neck_w) = (row(0.27), f32::MAX);
    for y in (head_row + (0.05 * h) as usize).min(row(0.60))..=row(0.62) {
        let w = smooth(y);
        if w < neck_w {
            neck_w = w;
            neck = y;
        }
    }
    if neck_w >= 0.70 * head_w {
        neck = row(0.27);
    }

    // Hips: going up from the feet, the row where the two legs merge into one run.
    let min_run = (mask.w / 100).max(2);
    let mut hip = row(0.60);
    let mut seen_split = false;
    for y in (row(0.45)..by1).rev() {
        let n = runs(mask, y, min_run).len();
        if n >= 2 {
            seen_split = true;
        } else if seen_split && n == 1 {
            hip = y;
            break;
        }
    }
    hip = hip.clamp(row(0.45), row(0.80));

    // Torso: the central run of the middle rows. With separated arms that is exactly the torso;
    // with arms attached to it, the middle 64%.
    let mid = (neck + hip) / 2;
    let rs = runs(mask, mid, min_run);
    let cx = (mask.axis() * mask.w as f32) as usize;
    let (rl, rr) = rs.iter().copied().find(|(a, b)| *a <= cx && cx < *b).or_else(|| rs.first().copied()).unwrap_or((bx0, bx1));
    let (t0, t1) = if rs.len() >= 3 {
        (rl as f32, rr as f32)
    } else {
        let rw = (rr - rl) as f32;
        (rl as f32 + 0.18 * rw, rr as f32 - 0.18 * rw)
    };
    let tw = t1 - t0;
    let (hx0, hx1) = extent(mask, by0, neck).unwrap_or((bx0, bx1));
    let (ax0, _) = extent(mask, neck, hip).unwrap_or((bx0, bx1));
    // Feet region only: hands hanging at the hips must not widen the leg box.
    let (lx0, _) = extent(mask, (hip + (0.06 * h) as usize).min(by1 - 1), by1).unwrap_or((bx0, cx));

    let (iw, ih) = (mask.w as f32, mask.h as f32);
    let n = |x: f32, y: f32| (x / iw, y / ih);
    let r = |x0: f32, y0: f32, x1: f32, y1: f32| {
        let (a, b) = n(x0, y0);
        let (c, d) = n(x1, y1);
        RectN::new(a, b, c, d).clamped()
    };
    let pad = 0.02 * h;
    Ok(Layout {
        head: r(hx0 as f32, by0 as f32, hx1 as f32, neck as f32 + pad),
        body: r(t0, neck as f32 - 0.01 * h, t1, hip as f32 + 0.05 * h),
        arm: r((ax0 as f32).min(t0 - 0.05 * tw), neck as f32 + 0.01 * h, t0 + 0.12 * tw, hip as f32 + 0.05 * h),
        leg: r(lx0 as f32, hip as f32 - 0.02 * h, cx as f32, by1 as f32),
    })
}

// ------------------------------------------------------------------ the validator

/// Checks a layout against the mask. Errors block saving; warnings are advice.
pub fn validate(layout: &Layout, mask: &Mask) -> Vec<Issue> {
    let mut out = Vec::new();
    let Some((bx0, by0, bx1, by1)) = mask.bounds() else {
        return vec![Issue::err("E1135", None, "the mask is empty: no character was found".into())];
    };
    let (iw, ih) = (mask.w as f32, mask.h as f32);
    let (cx, top, bottom) = (mask.axis(), by0 as f32 / ih, by1 as f32 / ih);
    let ch = bottom - top;

    if mask.area() as f32 / (iw * ih) < 0.04 {
        out.push(Issue::warn("E1135", None, "the character is very small in the image; use a tighter crop for a sharper result".into()));
    }
    if by0 == 0 || by1 as usize >= mask.h || bx0 == 0 || bx1 as usize >= mask.w {
        out.push(Issue::warn("E1135", None, "the character touches the edge of the image: it may be cut off (head or feet missing)".into()));
    }

    for (name, r) in layout.rects() {
        let p = PARTS.iter().copied().find(|p| *p == name);
        if r.x0 < -0.001 || r.y0 < -0.001 || r.x1 > 1.001 || r.y1 > 1.001 {
            out.push(Issue::err("E1130", p, format!("the {} box goes outside the image", name)));
        }
        if r.w() < 0.03 || r.h() < 0.03 {
            out.push(Issue::err("E1130", p, format!("the {} box is too small ({:.0}% x {:.0}% of the image)", name, r.w() * 100.0, r.h() * 100.0)));
            continue;
        }
        // How much of the character is inside the box.
        let (x0, y0, x1, y1) = ((r.x0 * iw) as usize, (r.y0 * ih) as usize, ((r.x1 * iw) as usize).min(mask.w), ((r.y1 * ih) as usize).min(mask.h));
        let mut solid = 0usize;
        for y in y0..y1 {
            for x in x0..x1 {
                if mask.solid(x, y) {
                    solid += 1;
                }
            }
        }
        if solid == 0 {
            out.push(Issue::err("E1131", p, format!("the {} box contains none of the character: move it over the {}", name, name)));
        } else if (solid as f32) < 0.04 * ((x1 - x0) * (y1 - y0)) as f32 {
            out.push(Issue::warn("E1131", p, format!("the {} box is mostly empty background; tighten it around the {}", name, name)));
        }
    }

    let (h, b, a, l) = (layout.head, layout.body, layout.arm, layout.leg);
    if h.center().1 >= b.center().1 {
        out.push(Issue::err("E1132", Some("head"), "the head box must be above the body box".into()));
    }
    if l.center().1 <= b.center().1 {
        out.push(Issue::err("E1132", Some("leg"), "the leg box must be below the body box".into()));
    }
    if (h.y1 - b.y0) > 0.45 * h.h().max(b.h()) {
        out.push(Issue::warn("E1132", Some("head"), "the head and body boxes overlap a lot; the head box should end near the neck".into()));
    }
    if a.center().0 >= cx {
        out.push(Issue::err("E1133", Some("arm"), "the arm box must be on the LEFT side of the character as seen in the image (the right arm is its mirror)".into()));
    }
    if l.center().0 >= cx {
        out.push(Issue::err("E1133", Some("leg"), "the leg box must be on the LEFT side of the character as seen in the image (the right leg is its mirror)".into()));
    }
    // The body must sit between the two shoulders: a body box that reaches past the arms (tail, wings,
    // the arms themselves) would push the arms away from the trunk when the puppet is assembled.
    let (arm_c, arm_m) = (a.center().0, 2.0 * cx - a.center().0);
    if b.x0 < arm_c - 0.02 || b.x1 > arm_m + 0.02 {
        out.push(Issue::err(
            "E1137",
            Some("body"),
            "the body box is wider than the space between the arms: it must only contain the trunk, from shoulder to shoulder (a tail or wings are left out of it; arms and legs are separate boxes)".into(),
        ));
    }
    if a.w() > a.h() {
        out.push(Issue::warn("E1136", Some("arm"), "the arm box is wider than tall: an arm hanging down is tall and thin".into()));
    }
    if l.w() > l.h() {
        out.push(Issue::warn("E1136", Some("leg"), "the leg box is wider than tall: a leg is tall and thin".into()));
    }
    if (b.center().0 - cx).abs() > 0.15 * (bx1 - bx0) as f32 / iw + 0.02 {
        out.push(Issue::warn("E1133", Some("body"), "the body box is not centered on the character: the puppet will look lopsided".into()));
    }
    let hh = (layout.leg.y1 - layout.head.y0).max(1e-3);
    if (layout.leg.y1 - bottom).abs() > 0.06 * ch.max(hh) {
        out.push(Issue::warn("E1132", Some("leg"), "the leg box should reach the feet (the bottom of the character)".into()));
    }
    if (layout.head.y0 - top).abs() > 0.06 * ch {
        out.push(Issue::warn("E1132", Some("head"), "the head box should start at the top of the character (hat or hair included)".into()));
    }

    // Coverage: every solid pixel should lie inside a piece (arms and legs count on both sides).
    let boxes = [h, b, a, a.mirrored(cx), l, l.mirrored(cx)];
    let (mut total, mut uncovered) = (0usize, 0usize);
    for y in 0..mask.h {
        for x in 0..mask.w {
            if mask.solid(x, y) {
                total += 1;
                let (fx, fy) = ((x as f32 + 0.5) / iw, (y as f32 + 0.5) / ih);
                if !boxes.iter().any(|r| r.contains(fx, fy)) {
                    uncovered += 1;
                }
            }
        }
    }
    let frac = uncovered as f32 / total.max(1) as f32;
    if frac > 0.12 {
        out.push(Issue::err("E1134", None, format!("{:.0}% of the character is not inside any box (a tail, hat, wings or hands outside the boxes?). Extend the boxes to cover it", frac * 100.0)));
    } else if frac > 0.05 {
        out.push(Issue::warn("E1134", None, format!("{:.0}% of the character is not inside any box; extend a box if that part matters", frac * 100.0)));
    }
    out
}
