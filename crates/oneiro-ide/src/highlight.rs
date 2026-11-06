//! Syntax highlighting for the Oneiro script editor.

use eframe::egui::{self, text::LayoutJob, Color32, FontId, Stroke, TextFormat};
use oneiro_core::catalog::*;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tok {
    Comment,
    Str,
    Num,
    Color,
    Keyword,
    Verb,
    Camera,
    Emotion,
    Name,
    Reference,
    Punct,
    Plain,
}

const KEYWORDS: &[&str] = &[
    "oneiro", "project", "style", "default", "use", "scene", "in", "place", "at", "facing", "camera", "cut", "to", "on",
    "from", "angle", "move", "lens", "for", "wait", "together", "sfx", "music", "audio", "volume", "fade_in", "fade_out", "once", "stop", "and", "give",
    "transition", "wound", "relation", "remember", "character", "set", "time", "intensity", "away",
];

pub fn classify(word: &str) -> Tok {
    if word.contains('.') {
        let first = word.chars().next().unwrap_or('a');
        return if first.is_uppercase() { Tok::Name } else { Tok::Reference };
    }
    if KEYWORDS.contains(&word) {
        return Tok::Keyword;
    }
    if IMPLEMENTED_VERBS.contains(&word) || PLANNED_VERBS.contains(&word) {
        return Tok::Verb;
    }
    if SHOTS.contains(&word) || ANGLES.contains(&word) || MOVES.contains(&word) || SIDES.contains(&word) {
        return Tok::Camera;
    }
    if EXPRESSIONS.contains(&word) || TIMES.contains(&word) {
        return Tok::Emotion;
    }
    if word.chars().next().map_or(false, |c| c.is_uppercase()) {
        return Tok::Name;
    }
    Tok::Plain
}

/// Splits one line into (byte_start, byte_end, kind) tokens that cover the whole line.
pub fn tokenize_line(line: &str) -> Vec<(usize, usize, Tok)> {
    let chars: Vec<(usize, char)> = line.char_indices().collect();
    let n = chars.len();
    let byte_at = |i: usize| if i >= n { line.len() } else { chars[i].0 };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        let (b, c) = chars[i];
        let start = i;
        let kind;
        if c == '#' {
            let mut j = i + 1;
            while j < n && chars[j].1.is_ascii_hexdigit() {
                j += 1;
            }
            let hex = j - i - 1;
            let boundary = j >= n || !(chars[j].1.is_alphanumeric() || chars[j].1 == '_');
            if (hex == 6 || hex == 3) && boundary {
                i = j;
                kind = Tok::Color;
            } else {
                i = n;
                kind = Tok::Comment;
            }
        } else if c == '"' {
            let mut j = i + 1;
            while j < n && chars[j].1 != '"' {
                if chars[j].1 == '\\' && j + 1 < n {
                    j += 1;
                }
                j += 1;
            }
            i = (j + 1).min(n);
            kind = Tok::Str;
        } else if c.is_ascii_digit() || (c == '-' && i + 1 < n && chars[i + 1].1.is_ascii_digit()) {
            let mut j = i + 1;
            while j < n && (chars[j].1.is_ascii_digit() || chars[j].1 == '.' || chars[j].1.is_ascii_alphabetic()) {
                j += 1;
            }
            i = j;
            kind = Tok::Num;
        } else if c.is_alphabetic() || c == '_' {
            let mut j = i + 1;
            while j < n {
                let cj = chars[j].1;
                if cj.is_alphanumeric() || cj == '_' {
                    j += 1;
                } else if cj == '.' && j + 1 < n && (chars[j + 1].1.is_alphabetic() || chars[j + 1].1 == '_') {
                    j += 1;
                } else {
                    break;
                }
            }
            i = j;
            kind = classify(&line[b..byte_at(j)]);
        } else if "{}()[]:,=".contains(c) || (c == '-' && i + 1 < n && chars[i + 1].1 == '>') {
            i += if c == '-' { 2 } else { 1 };
            kind = Tok::Punct;
        } else {
            while i < n && !(chars[i].1.is_alphanumeric() || "#\"{}()[]:,=_-".contains(chars[i].1)) {
                i += 1;
            }
            if i == start {
                i += 1;
            }
            kind = Tok::Plain;
        }
        out.push((b, byte_at(i), kind));
    }
    out
}

pub fn color_of(t: Tok) -> Color32 {
    match t {
        Tok::Comment => Color32::from_rgb(110, 122, 108),
        Tok::Str => Color32::from_rgb(236, 176, 110),
        Tok::Num => Color32::from_rgb(120, 214, 224),
        Tok::Color => Color32::from_rgb(240, 130, 200),
        Tok::Keyword => Color32::from_rgb(186, 140, 255),
        Tok::Verb => Color32::from_rgb(110, 170, 255),
        Tok::Camera => Color32::from_rgb(110, 224, 170),
        Tok::Emotion => Color32::from_rgb(255, 150, 150),
        Tok::Name => Color32::from_rgb(250, 220, 120),
        Tok::Reference => Color32::from_rgb(160, 200, 215),
        Tok::Punct => Color32::from_rgb(150, 150, 170),
        Tok::Plain => Color32::from_rgb(220, 220, 232),
    }
}

/// Builds the colored layout for the whole text. Lines in `error_lines` get a red tint and
/// lines in `warn_lines` a yellow underline.
pub fn layout_job(text: &str, error_lines: &HashSet<u32>, warn_lines: &HashSet<u32>, font: FontId) -> LayoutJob {
    let mut job = LayoutJob::default();
    job.wrap.max_width = f32::INFINITY;
    let mut first = true;
    for (li, line) in text.split('\n').enumerate() {
        let lineno = li as u32 + 1;
        if !first {
            job.append("\n", 0.0, plain_format(&font, Color32::GRAY, None));
        }
        first = false;
        let bg = if error_lines.contains(&lineno) { Some(Color32::from_rgba_unmultiplied(220, 60, 60, 46)) } else { None };
        let underline = warn_lines.contains(&lineno);
        for (s, e, kind) in tokenize_line(line) {
            let mut f = plain_format(&font, color_of(kind), bg);
            if underline {
                f.underline = Stroke::new(1.0, Color32::from_rgb(230, 190, 70));
            }
            job.append(&line[s..e], 0.0, f);
        }
    }
    job
}

fn plain_format(font: &FontId, color: Color32, bg: Option<Color32>) -> TextFormat {
    TextFormat { font_id: font.clone(), color, background: bg.unwrap_or(Color32::TRANSPARENT), ..Default::default() }
}

#[allow(dead_code)]
fn _unused(_: &egui::Ui) {}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(String, Tok)> {
        tokenize_line(line).into_iter().map(|(s, e, k)| (line[s..e].to_string(), k)).collect()
    }

    #[test]
    fn tokens_cover_the_whole_line_without_gaps() {
        for line in [
            "  Marta (annoyed): \"Where's my pie?\" audio \"a.wav\"  # note",
            "camera over_shoulder Marta -> Blob",
            "wait 500ms",
            "  place Marta at diner.stool_2 facing diner.counter",
            "skin_tone: #F2C9A0 }",
            "",
        ] {
            let toks = tokenize_line(line);
            let mut pos = 0;
            for (s, e, _) in &toks {
                assert_eq!(*s, pos, "gap in {:?}", line);
                pos = *e;
            }
            assert_eq!(pos, line.len(), "{:?}", line);
        }
    }

    #[test]
    fn classifies_vocabulary() {
        let k = kinds("Blob walks_to diner.counter # hi");
        assert_eq!(k[0], ("Blob".into(), Tok::Name));
        assert!(k.contains(&("walks_to".into(), Tok::Verb)));
        assert!(k.contains(&("diner.counter".into(), Tok::Reference)));
        assert_eq!(k.last().unwrap().1, Tok::Comment);
        let k = kinds("camera medium on Marta angle low");
        assert!(k.contains(&("medium".into(), Tok::Camera)));
        assert!(k.contains(&("camera".into(), Tok::Keyword)));
        let k = kinds("skin_tone: #F2C9A0");
        assert!(k.contains(&("#F2C9A0".into(), Tok::Color)));
        let k = kinds("Marta (angry): \"hi # not a comment\"");
        assert!(k.contains(&("angry".into(), Tok::Emotion)));
        assert!(k.iter().any(|(t, k)| *k == Tok::Str && t.contains("not a comment")));
    }
}
