//! Tier B characters: the AI draws four SVG parts and the engine validates them. Same loop as the
//! other directors: the model never gets the last word, every answer is checked and the problems are
//! fed back for correction.

use crate::director::{add_usage, feedback_message, DirectorOpts, Turn};
use crate::guide::{self, Mode};
use crate::types::*;
use oneiro_core::catalog::SVG_PARTS;
use oneiro_core::defs::{check_part, default_part, SvgPart, SvgRig};
use oneiro_core::{CharacterDef, Project};
use oneiro_lang::Diagnostic;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct PartsResult {
    pub reply: String,
    /// The final `.onchar` source (with the files of the parts filled in), when valid.
    pub source: Option<String>,
    pub def: Option<CharacterDef>,
    /// (path relative to the project, SVG text) for every part.
    pub svgs: Vec<(String, String)>,
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub rounds: usize,
    pub usage: (u64, u64),
}

impl PartsResult {
    /// Writes the SVG files and the `.onchar` into the project library. Returns the `.onchar` path.
    pub fn save(&self, root: &Path) -> Result<PathBuf, String> {
        let (Some(def), Some(src)) = (&self.def, &self.source) else { return Err("nothing valid to save".into()) };
        save_files(root, &def.name, src, &self.svgs)
    }
}

/// Writes a validated vector character into the project.
pub fn save_files(root: &Path, name: &str, source: &str, svgs: &[(String, String)]) -> Result<PathBuf, String> {
    for (rel, text) in svgs {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(&p, text).map_err(|e| format!("{}: {}", p.display(), e))?;
    }
    let path = root.join("library").join("characters").join(format!("{}.onchar", name));
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, source).map_err(|e| format!("{}: {}", path.display(), e))?;
    Ok(path)
}

/// Every fenced block of a reply as (info string in lowercase, code).
pub fn all_blocks(text: &str) -> Vec<(String, String)> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if let Some(rest) = lines[i].trim_start().strip_prefix("```") {
            let tag = rest.trim().to_lowercase();
            let mut j = i + 1;
            let mut code = Vec::new();
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                code.push(lines[j]);
                j += 1;
            }
            if j < lines.len() {
                out.push((tag, code.join("\n")));
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// The `svg <part>` blocks of a reply, keyed by part name.
pub fn svg_blocks(blocks: &[(String, String)]) -> BTreeMap<String, String> {
    let mut svgs = BTreeMap::new();
    for (tag, code) in blocks {
        if let Some(rest) = tag.strip_prefix("svg") {
            let name = rest.trim_matches(|c: char| c == ':' || c.is_whitespace());
            if !name.is_empty() {
                svgs.insert(name.to_string(), code.clone());
            }
        }
    }
    svgs
}

fn part_error(code: &str, msg: String) -> Diagnostic {
    Diagnostic::error(code, "character.onchar", oneiro_lang::Pos { line: 1, col: 1 }, msg)
}

/// Validates a vector character: the `character` source (whose `parts` block may omit `file`) plus one
/// SVG per part. On success returns the definition with its parts loaded, the canonical source and the
/// files to write.
#[allow(clippy::type_complexity)]
pub fn validate_parts(source: &str, svgs: &BTreeMap<String, String>) -> Result<(CharacterDef, String, Vec<(String, String)>), Vec<Diagnostic>> {
    let (file, mut diags) = oneiro_lang::parse(source, "character.onchar");
    let blocks: Vec<_> = file.blocks.iter().filter(|b| b.kind == "character").collect();
    if blocks.len() != 1 {
        diags.push(part_error("E1100", format!("expected exactly one `character` block, found {}", blocks.len())));
        return Err(diags);
    }
    let mut def = CharacterDef::from_block(blocks[0], "character.onchar", &mut diags);
    if !def.name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) || !def.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        diags.push(part_error("E1100", format!("invalid character name '{}': start with a capital letter; use only letters, digits and underscores", def.name)));
    }
    if def.svg.is_none() {
        let parts = SVG_PARTS
            .iter()
            .map(|n| {
                let (h, p) = default_part(n);
                (n.to_string(), SvgPart { file: String::new(), height: h, pivot: p, doc: None })
            })
            .collect();
        def.svg = Some(SvgRig { parts, face: true, face_x: 0.5, face_y: 0.55, face_scale: 1.0 });
    }
    let name = def.name.clone();
    let mut files = Vec::new();
    if let Some(rig) = def.svg.as_mut() {
        for part in SVG_PARTS {
            let Some(p) = rig.parts.get_mut(*part) else { continue };
            // Parts live where Oneiro saves them, whatever the model wrote.
            p.file = format!("library/parts/{}/{}.svg", name, part);
            match svgs.get(*part) {
                None => diags.push(part_error("E1118", format!("missing the ```svg {} block", part))),
                Some(text) => match oneiro_core::svg::parse_svg(text) {
                    Err(errs) => {
                        for e in errs {
                            diags.push(part_error("E1116", format!("part '{}': {}", part, e)));
                        }
                    }
                    Ok(doc) => {
                        let problems = check_part(part, &doc, p.pivot);
                        for pr in &problems {
                            diags.push(part_error("E1117", pr.clone()));
                        }
                        if problems.is_empty() {
                            p.doc = Some(Arc::new(doc));
                            files.push((p.file.clone(), text.trim().to_string() + "\n"));
                        }
                    }
                },
            }
        }
    }
    if diags.iter().any(|d| d.is_error()) {
        return Err(diags);
    }
    let src = def.to_source();
    Ok((def, src, files))
}

/// The text of a reply outside its fenced blocks.
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

/// Designs a character as four SVG parts.
pub fn direct_character_parts(
    provider: &dyn Provider,
    project: &Project,
    description: &str,
    history: &[Turn],
    opts: &DirectorOpts,
    progress: &mut dyn FnMut(&str),
) -> Result<PartsResult, String> {
    let system = guide::system_prompt(project, Mode::Parts);
    let mut messages: Vec<Message> = Vec::new();
    for t in history {
        messages.push(Message::user(t.user.clone()));
        messages.push(Message::assistant(t.assistant.clone()));
    }
    messages.push(Message::user(description.trim().to_string()));
    let mut out = PartsResult { reply: String::new(), source: None, def: None, svgs: vec![], valid: false, diagnostics: vec![], rounds: 0, usage: (0, 0) };
    for round in 1..=opts.max_rounds.max(1) {
        progress(&format!("asking {} to draw the character (round {})...", provider.name(), round));
        let resp = provider.complete(&ChatRequest { system: system.clone(), messages: messages.clone(), max_tokens: opts.max_tokens })?;
        add_usage(&mut out.usage, &resp);
        out.rounds = round;
        out.reply = explanation(&resp.text);
        let blocks = all_blocks(&resp.text);
        let character = blocks
            .iter()
            .rev()
            .find(|(t, c)| t == "onchar" || t == "oneiro" || (t.is_empty() && c.trim_start().starts_with("character ")))
            .map(|(_, c)| c.clone());
        let Some(character) = character else {
            out.diagnostics = vec![part_error("E1100", "no ```onchar block found".into())];
            messages.push(Message::assistant(resp.text.clone()));
            messages.push(Message::user("Reply with ONE ```onchar block and FOUR ```svg blocks (svg head, svg body, svg arm, svg leg), as described in the guide.".to_string()));
            continue;
        };
        match validate_parts(&character, &svg_blocks(&blocks)) {
            Ok((def, source, files)) => {
                out.source = Some(source);
                out.def = Some(def);
                out.svgs = files;
                out.valid = true;
                out.diagnostics = vec![];
                return Ok(out);
            }
            Err(diags) => {
                out.source = Some(character);
                out.diagnostics = diags.clone();
                messages.push(Message::assistant(resp.text.clone()));
                messages.push(Message::user(feedback_message("character and SVG parts", &diags)));
            }
        }
    }
    Ok(out)
}
