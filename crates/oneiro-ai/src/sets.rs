//! Open sets drawn by the AI: a `set` block plus SVG pictures (a backdrop for each side and some sprites).
//! Same loop as the other directors: every answer is checked and the problems are fed back.

use crate::director::{add_usage, feedback_message, DirectorOpts, Turn};
use crate::guide::{self, Mode};
use crate::parts::{all_blocks, svg_blocks};
use crate::types::*;
use oneiro_core::scenery::BACKDROP_SIDES;
use oneiro_core::{Project, SetDef};
use oneiro_lang::Diagnostic;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub struct SetResult {
    pub reply: String,
    /// The final `.onset` source (picture names replaced by project paths), when valid.
    pub source: Option<String>,
    pub name: String,
    /// (path relative to the project, SVG text).
    pub files: Vec<(String, String)>,
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub rounds: usize,
    pub usage: (u64, u64),
}

impl SetResult {
    pub fn save(&self, root: &Path) -> Result<PathBuf, String> {
        let Some(src) = &self.source else { return Err("nothing valid to save".into()) };
        save_files(root, &self.name, src, &self.files)
    }
}

pub fn save_files(root: &Path, name: &str, source: &str, files: &[(String, String)]) -> Result<PathBuf, String> {
    for (rel, text) in files {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    let path = root.join("library").join("sets").join(format!("{}.onset", name));
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, source).map_err(|e| format!("{}: {}", path.display(), e))?;
    Ok(path)
}

fn err(code: &str, msg: String) -> Diagnostic {
    Diagnostic::error(code, "set.onset", oneiro_lang::Pos { line: 1, col: 1 }, msg)
}

/// Validates an open set: the `set` source (pictures referenced by NAME) plus one SVG per name. On success
/// returns the definition, the source with every name replaced by its file path, and the files to write.
#[allow(clippy::type_complexity)]
pub fn validate_set(source: &str, svgs: &BTreeMap<String, String>) -> Result<(SetDef, String, Vec<(String, String)>), Vec<Diagnostic>> {
    let (file, mut diags) = oneiro_lang::parse(source, "set.onset");
    let blocks: Vec<_> = file.blocks.iter().filter(|b| b.kind == "set").collect();
    if blocks.len() != 1 {
        diags.push(err("E1127", format!("expected exactly one `set` block, found {}", blocks.len())));
        return Err(diags);
    }
    let mut set = SetDef::from_block(blocks[0], "set.onset", &mut diags);
    if set.name.is_empty() || !set.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') || !set.name.chars().next().map_or(false, |c| c.is_ascii_alphabetic()) {
        diags.push(err("E1127", format!("invalid set name '{}': letters, digits and underscores, starting with a letter", set.name)));
    }
    if !set.open {
        diags.push(err("E1127", "scenarios drawn by the AI must be `kind: open`".into()));
    }
    let mut files: Vec<(String, String)> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut source = source.to_string();
    let mut names: Vec<(String, bool)> = set.backdrops.values().map(|b| (b.file.clone(), true)).collect();
    names.extend(set.sprites.iter().map(|s| (s.art.file.clone(), false)));
    if set.backdrops.is_empty() {
        diags.push(err("E1127", "add a `backdrops` block with at least a `north` picture".into()));
    }
    if set.sprites.len() > 24 {
        diags.push(err("E1127", format!("{} sprites is too many: use 24 at most", set.sprites.len())));
    }
    for (name, is_backdrop) in names {
        if !seen.insert(name.clone()) {
            continue;
        }
        let Some(text) = svgs.get(&name) else {
            diags.push(err("E1128", format!("missing the ```svg {} block (it is used by the set)", name)));
            continue;
        };
        if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            diags.push(err("E1128", format!("picture name '{}': use letters, digits and underscores only (no paths or extensions)", name)));
            continue;
        }
        match oneiro_core::svg::parse_svg(text) {
            Err(es) => {
                for e in es {
                    diags.push(err("E1128", format!("picture '{}': {}", name, e)));
                }
            }
            Ok(doc) => {
                let aspect = doc.aspect();
                if is_backdrop && !(1.8..=4.5).contains(&aspect) {
                    diags.push(err("E1128", format!("backdrop '{}' has a viewBox ratio of {:.2}; it must be wide, between 1.8 and 4.5 (use for example viewBox=\"0 0 1600 600\")", name, aspect)));
                    continue;
                }
                if doc.shapes.len() < 3 {
                    diags.push(err("E1128", format!("picture '{}' has only {} shape(s); draw something recognisable", name, doc.shapes.len())));
                    continue;
                }
                let dir = if is_backdrop { "backgrounds" } else { "scenery" };
                let rel = format!("{}/{}_{}.svg", dir, set.name, name);
                source = source.replace(&format!("\"{}\"", name), &format!("\"{}\"", rel));
                files.push((rel, text.trim().to_string() + "\n"));
            }
        }
    }
    for side in set.backdrops.keys() {
        debug_assert!(BACKDROP_SIDES.contains(&side.as_str()));
    }
    if diags.iter().any(|d| d.is_error()) {
        return Err(diags);
    }
    // The final source must itself be a valid set.
    let (file2, mut d2) = oneiro_lang::parse(&source, "set.onset");
    if let Some(b) = file2.blocks.iter().find(|b| b.kind == "set") {
        set = SetDef::from_block(b, "set.onset", &mut d2);
    }
    if d2.iter().any(|d| d.is_error()) {
        return Err(d2);
    }
    Ok((set, source, files))
}

fn explanation(text: &str) -> String {
    let mut keep = Vec::new();
    let mut inside = false;
    for l in text.lines() {
        if l.trim_start().starts_with("```") {
            inside = !inside;
            continue;
        }
        if !inside {
            keep.push(l);
        }
    }
    keep.join("\n").trim().to_string()
}

/// Designs an open set (a beach, a forest, a street...) as a `set` block and SVG pictures.
pub fn direct_set(provider: &dyn Provider, project: &Project, description: &str, history: &[Turn], opts: &DirectorOpts, progress: &mut dyn FnMut(&str)) -> Result<SetResult, String> {
    let system = guide::system_prompt(project, Mode::Set);
    let mut messages: Vec<Message> = Vec::new();
    for t in history {
        messages.push(Message::user(t.user.clone()));
        messages.push(Message::assistant(t.assistant.clone()));
    }
    messages.push(Message::user(description.trim().to_string()));
    let mut out = SetResult { reply: String::new(), source: None, name: String::new(), files: vec![], valid: false, diagnostics: vec![], rounds: 0, usage: (0, 0) };
    for round in 1..=opts.max_rounds.max(1) {
        progress(&format!("asking {} to design the scenery (round {})...", provider.name(), round));
        let resp = provider.complete(&ChatRequest { system: system.clone(), messages: messages.clone(), max_tokens: opts.max_tokens.max(12_000) })?;
        add_usage(&mut out.usage, &resp);
        out.rounds = round;
        out.reply = explanation(&resp.text);
        let blocks = all_blocks(&resp.text);
        let set_src = blocks.iter().rev().find(|(t, c)| t == "onset" || t == "oneiro" || (t.is_empty() && c.trim_start().starts_with("set "))).map(|(_, c)| c.clone());
        let Some(set_src) = set_src else {
            out.diagnostics = vec![err("E1127", "no ```onset block found".into())];
            messages.push(Message::assistant(resp.text.clone()));
            messages.push(Message::user("Reply with ONE ```onset block and one ```svg NAME block for every picture it uses, as described in the guide.".to_string()));
            continue;
        };
        match validate_set(&set_src, &svg_blocks(&blocks)) {
            Ok((set, source, files)) => {
                out.name = set.name.clone();
                out.source = Some(source);
                out.files = files;
                out.valid = true;
                out.diagnostics = vec![];
                return Ok(out);
            }
            Err(diags) => {
                out.source = Some(set_src);
                out.diagnostics = diags.clone();
                messages.push(Message::assistant(resp.text.clone()));
                messages.push(Message::user(feedback_message("set and pictures", &diags)));
            }
        }
    }
    Ok(out)
}
