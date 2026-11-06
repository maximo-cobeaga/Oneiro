//! Pure text helpers for the script editor (character indices, line lookups, snippet insertion).

/// Character index of the start of `line` (1-based). Past-the-end lines return the text length.
pub fn line_start(text: &str, line: u32) -> usize {
    if line <= 1 {
        return 0;
    }
    let mut count = 1;
    for (i, c) in text.chars().enumerate() {
        if c == '\n' {
            count += 1;
            if count == line {
                return i + 1;
            }
        }
    }
    text.chars().count()
}

/// 1-based line containing the character index `idx`.
pub fn line_of_char(text: &str, idx: usize) -> u32 {
    let mut line = 1;
    for (i, c) in text.chars().enumerate() {
        if i >= idx {
            break;
        }
        if c == '\n' {
            line += 1;
        }
    }
    line
}

pub fn line_col_to_char(text: &str, line: u32, col: u32) -> usize {
    let start = line_start(text, line);
    let line_len = text.chars().skip(start).take_while(|c| *c != '\n').count();
    start + (col.saturating_sub(1) as usize).min(line_len)
}

fn lines_of(text: &str) -> Vec<&str> {
    text.split('\n').collect()
}

/// Last line (1-based) of the statement that starts at `start_line`; blocks that open with `{`
/// extend to their matching `}`.
pub fn statement_end_line(text: &str, start_line: u32) -> u32 {
    let lines = lines_of(text);
    let idx = (start_line as usize).saturating_sub(1);
    if idx >= lines.len() {
        return start_line;
    }
    let first = strip_comment(lines[idx]).trim_end().to_string();
    if !first.ends_with('{') {
        return start_line;
    }
    let mut depth = 0i32;
    for (i, l) in lines.iter().enumerate().skip(idx) {
        for c in strip_comment(l).chars() {
            match c {
                '{' => depth += 1,
                '}' => depth -= 1,
                _ => {}
            }
        }
        if depth <= 0 {
            return i as u32 + 1;
        }
    }
    lines.len() as u32
}

fn strip_comment(line: &str) -> &str {
    // A '#' starts a comment unless it begins a color literal; good enough for brace matching.
    let mut in_str = false;
    let bytes = line.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        match *b {
            b'"' => in_str = !in_str,
            b'#' if !in_str => {
                let rest = &line[i + 1..];
                let hex = rest.chars().take_while(|c| c.is_ascii_hexdigit()).count();
                let boundary = rest.chars().nth(hex).map_or(true, |c| !c.is_alphanumeric());
                if !((hex == 6 || hex == 3) && boundary) {
                    return &line[..i];
                }
            }
            _ => {}
        }
    }
    line
}

fn indent_of(line: &str) -> String {
    line.chars().take_while(|c| *c == ' ' || *c == '\t').collect()
}

/// Inserts `snippet` as a new line near the cursor: after the cursor's line, or before it when
/// that line is a closing `}`. Returns the new text and the new cursor (end of the inserted line).
pub fn insert_at_cursor(text: &str, cursor: usize, snippet: &str) -> (String, usize) {
    let chars: Vec<char> = text.chars().collect();
    let cur = cursor.min(chars.len());
    let mut ls = cur;
    while ls > 0 && chars[ls - 1] != '\n' {
        ls -= 1;
    }
    let mut le = cur;
    while le < chars.len() && chars[le] != '\n' {
        le += 1;
    }
    let line: String = chars[ls..le].iter().collect();
    let trimmed = line.trim();
    let indent = indent_of(&line);
    let mut out: String = chars[..ls].iter().collect();
    let new_cursor;
    if trimmed == "}" {
        let ins = format!("{}  {}\n", indent, snippet);
        out.push_str(&ins);
        new_cursor = ls + ins.chars().count() - 1;
        out.extend(chars[ls..].iter());
    } else {
        let extra = if strip_comment(trimmed).trim_end().ends_with('{') { "  " } else { "" };
        out.extend(chars[ls..le].iter());
        let ins = format!("\n{}{}{}", indent, extra, snippet);
        out.push_str(&ins);
        new_cursor = le + ins.chars().count();
        out.extend(chars[le..].iter());
    }
    (out, new_cursor)
}

/// Inserts several lines right after `line` (1-based), using that line's indentation (or one level
/// deeper if it opens a block). Returns the new text and the index of the end of the inserted text.
pub fn insert_lines_after(text: &str, line: u32, new_lines: &[String]) -> (String, usize) {
    let lines = lines_of(text);
    let idx = (line as usize).saturating_sub(1).min(lines.len().saturating_sub(1));
    let base = lines.get(idx).copied().unwrap_or("");
    let extra = if strip_comment(base).trim_end().ends_with('{') { "  " } else { "" };
    let indent = format!("{}{}", indent_of(base), extra);
    let mut out: Vec<String> = lines.iter().map(|s| s.to_string()).collect();
    let block: Vec<String> = new_lines.iter().map(|l| format!("{}{}", indent, l)).collect();
    let at = (idx + 1).min(out.len());
    for (k, b) in block.iter().enumerate() {
        out.insert(at + k, b.clone());
    }
    let joined = out.join("\n");
    let end_line = (at + block.len()) as u32;
    let cursor = line_start(&joined, end_line) + out[at + block.len().saturating_sub(1)].chars().count();
    (joined, cursor)
}


/// (first_line, last_line) of every `scene ... {` block, 1-based. The last line is the closing `}`.
pub fn scene_ranges(text: &str) -> Vec<(u32, u32)> {
    let mut out = Vec::new();
    for (i, l) in text.split('\n').enumerate() {
        let t = l.trim_start();
        if t.starts_with("scene ") && strip_comment(t).trim_end().ends_with('{') {
            let start = i as u32 + 1;
            out.push((start, statement_end_line(text, start)));
        }
    }
    out
}

/// Where a palette insertion should go: the cursor if it is inside a scene, otherwise the end of
/// `prefer_scene` (0-based, e.g. the scene under the playhead) or of the last scene.
pub fn insertion_cursor(text: &str, cursor: usize, prefer_scene: Option<usize>) -> usize {
    let ranges = scene_ranges(text);
    if ranges.is_empty() {
        return cursor;
    }
    let line = line_of_char(text, cursor);
    if ranges.iter().any(|(a, b)| line >= *a && line <= *b) {
        return cursor;
    }
    let idx = prefer_scene.filter(|i| *i < ranges.len()).unwrap_or(ranges.len() - 1);
    line_start(text, ranges[idx].1)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_lookups() {
        let t = "ab\ncd\nef";
        assert_eq!(line_start(t, 1), 0);
        assert_eq!(line_start(t, 2), 3);
        assert_eq!(line_start(t, 3), 6);
        assert_eq!(line_of_char(t, 0), 1);
        assert_eq!(line_of_char(t, 4), 2);
        assert_eq!(line_col_to_char(t, 2, 2), 4);
    }

    #[test]
    fn block_end_is_found() {
        let t = "scene 1 \"x\" in s {\n  together {\n    A nods\n  }\n  wait 1s\n}\n";
        assert_eq!(statement_end_line(t, 2), 4);
        assert_eq!(statement_end_line(t, 1), 6);
        assert_eq!(statement_end_line(t, 5), 5);
    }

    #[test]
    fn insert_after_cursor_line_and_before_closing_brace() {
        let t = "scene 1 \"x\" in s {\n  A nods\n}\n";
        // cursor on line 2
        let (out, cur) = insert_at_cursor(t, 22, "A waves");
        assert!(out.contains("  A nods\n  A waves\n}"), "{}", out);
        assert_eq!(out.chars().nth(cur - 1), Some('s'));
        // cursor on the closing brace line -> inserted before it
        let idx = t.find("}\n").unwrap();
        let (out2, _) = insert_at_cursor(t, idx, "wait 1s");
        assert!(out2.ends_with("  A nods\n  wait 1s\n}\n"), "{}", out2);
        // cursor on a line that opens a block -> indented inside it
        let (out3, _) = insert_at_cursor(t, 3, "time: night");
        assert!(out3.contains("{\n  time: night\n  A nods"), "{}", out3);
    }

    #[test]
    fn insert_lines_after_keeps_indentation() {
        let t = "scene 1 \"x\" in s {\n  A nods\n  wait 1s\n}\n";
        let (out, cur) = insert_lines_after(t, 2, &["A walks_to (1, 2)".into(), "A waves".into()]);
        assert!(out.contains("  A nods\n  A walks_to (1, 2)\n  A waves\n  wait 1s"), "{}", out);
        assert!(cur > 0);
    }
}

#[cfg(test)]
mod scene_tests {
    use super::*;

    const T: &str = "oneiro 0.1\nscene 1 \"a\" in s {\n  wait 1s\n}\nscene 2 \"b\" in s {\n  wait 2s\n}\n";

    #[test]
    fn finds_scene_blocks() {
        assert_eq!(scene_ranges(T), vec![(2, 4), (5, 7)]);
    }

    #[test]
    fn cursor_outside_scenes_moves_to_the_preferred_scene_end() {
        // cursor at the very start (line 1)
        let c = insertion_cursor(T, 0, Some(0));
        assert_eq!(line_of_char(T, c), 4);
        let c = insertion_cursor(T, 0, None);
        assert_eq!(line_of_char(T, c), 7);
        // cursor already inside scene 1 is kept
        let inside = line_start(T, 3) + 2;
        assert_eq!(insertion_cursor(T, inside, Some(1)), inside);
        // inserting there lands before the closing brace of scene 1
        let (out, _) = insert_at_cursor(T, insertion_cursor(T, 0, Some(0)), "A waves");
        assert!(out.contains("  wait 1s\n  A waves\n}"), "{}", out);
    }
}
