//! The AI director: turns a request ("add a scene where Blob drops the pie") into a validated
//! script. The model never gets the last word: every answer is compiled by the real engine,
//! diagnostics are fed back for correction, and (when the provider can see images) the result is
//! checked visually through a contact sheet of the shots.

use crate::guide::{self, Mode};
use crate::types::*;
use oneiro_core::lock::Lock;
use oneiro_core::{compile_source, CharacterDef, Project};
use oneiro_lang::{to_json_array, Diagnostic};

pub struct ProjectCtx<'a> {
    pub project: &'a Project,
    pub lock: &'a Lock,
    pub script_label: String,
}

#[derive(Clone, Debug)]
pub struct DirectorOpts {
    pub max_rounds: usize,
    pub self_check: bool,
    pub max_tokens: u32,
}

impl Default for DirectorOpts {
    fn default() -> Self {
        DirectorOpts { max_rounds: 4, self_check: false, max_tokens: 12_000 }
    }
}

/// One previous exchange, so follow-up requests ("make it shorter") have context.
#[derive(Clone, Debug)]
pub struct Turn {
    pub user: String,
    pub assistant: String,
}

#[derive(Clone, Debug, Default)]
pub struct SceneResult {
    /// The model's short explanation (without the code block).
    pub reply: String,
    /// The proposed full script, if the model produced one.
    pub script: Option<String>,
    pub valid: bool,
    /// Remaining diagnostics of the proposed script (empty when valid).
    pub diagnostics: Vec<Diagnostic>,
    /// How many times the model was called.
    pub rounds: usize,
    pub usage: (u64, u64),
    /// Things that happened on the way (auto-fixes, visual check).
    pub notes: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CharacterResult {
    pub reply: String,
    pub source: Option<String>,
    pub def: Option<CharacterDef>,
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
    pub rounds: usize,
    pub usage: (u64, u64),
}

/// Extracts the explanation text and the code of the last fenced block whose language tag is in
/// `tags` (falling back to an untagged block that starts with `starts_with`).
pub fn extract_block(text: &str, tags: &[&str], starts_with: &str) -> (String, Option<String>) {
    struct Block {
        tag: String,
        start: usize,
        end: usize,
        code: String,
    }
    let lines: Vec<&str> = text.lines().collect();
    let mut blocks: Vec<Block> = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let l = lines[i].trim_start();
        if let Some(rest) = l.strip_prefix("```") {
            let tag = rest.trim().to_lowercase();
            let mut j = i + 1;
            let mut code = Vec::new();
            while j < lines.len() && !lines[j].trim_start().starts_with("```") {
                code.push(lines[j]);
                j += 1;
            }
            if j < lines.len() {
                blocks.push(Block { tag, start: i, end: j, code: code.join("\n") });
                i = j + 1;
                continue;
            }
        }
        i += 1;
    }
    let pick = blocks
        .iter()
        .rev()
        .find(|b| tags.contains(&b.tag.as_str()))
        .or_else(|| blocks.iter().rev().find(|b| b.tag.is_empty() && b.code.trim_start().starts_with(starts_with)));
    match pick {
        Some(b) => {
            let explanation: Vec<&str> = lines.iter().enumerate().filter(|(n, _)| *n < b.start || *n > b.end).map(|(_, l)| *l).collect();
            (explanation.join("\n").trim().to_string(), Some(b.code.clone()))
        }
        None => (text.trim().to_string(), None),
    }
}

fn errors_of(d: &[Diagnostic]) -> Vec<&Diagnostic> {
    d.iter().filter(|x| x.is_error()).collect()
}

pub(crate) fn feedback_message(what: &str, diags: &[Diagnostic]) -> String {
    let errs = errors_of(diags);
    let human: Vec<String> = errs.iter().map(|d| d.render()).collect();
    let owned: Vec<Diagnostic> = errs.iter().map(|d| (*d).clone()).collect();
    format!(
        "The engine rejected your {} with {} error(s). Fix ALL of them using only the vocabulary, characters, sets and anchors listed in the guide, and return the COMPLETE corrected {} in the same format.\n\n{}\n\nJSON:\n{}",
        what,
        errs.len(),
        human.join("\n"),
        to_json_array(&owned),
        what
    )
}

pub(crate) fn add_usage(total: &mut (u64, u64), r: &ChatResponse) {
    if let Some((i, o)) = r.usage {
        total.0 += i;
        total.1 += o;
    }
}

pub fn direct_scene(
    provider: &dyn Provider,
    ctx: &ProjectCtx,
    current_script: &str,
    history: &[Turn],
    request: &str,
    opts: &DirectorOpts,
    progress: &mut dyn FnMut(&str),
) -> Result<SceneResult, String> {
    let system = guide::system_prompt(ctx.project, Mode::Scene);
    let mut messages: Vec<Message> = Vec::new();
    for t in history {
        messages.push(Message::user(t.user.clone()));
        messages.push(Message::assistant(t.assistant.clone()));
    }
    messages.push(Message::user(format!("{}\n\n## Current script\n```oneiro\n{}\n```", request.trim(), current_script.trim_end())));

    let mut result = SceneResult::default();
    let vision_done = !(opts.self_check && provider.supports_images());
    let rounds = opts.max_rounds.max(1) + if vision_done { 0 } else { 1 };
    for round in 1..=rounds {
        progress(&format!("asking {} (round {})...", provider.name(), round));
        let resp = provider.complete(&ChatRequest { system: system.clone(), messages: messages.clone(), max_tokens: opts.max_tokens })?;
        add_usage(&mut result.usage, &resp);
        result.rounds = round;
        let (explanation, code) = extract_block(&resp.text, &["oneiro", "onr"], "oneiro ");
        result.reply = explanation;
        let Some(code) = code else {
            // A pure answer (no script): done.
            result.script = None;
            result.valid = true;
            return Ok(result);
        };
        progress("checking the script with the engine...");
        let (ep, diags) = compile_source(ctx.project, &ctx.script_label, &code, ctx.lock);
        result.script = Some(code.clone());
        result.diagnostics = diags.clone();
        result.valid = errors_of(&diags).is_empty();
        if !result.valid {
            if round == rounds {
                break;
            }
            result.notes.push(format!("round {}: {} error(s), asking the model to fix them", round, errors_of(&diags).len()));
            messages.push(Message::assistant(resp.text.clone()));
            messages.push(Message::user(feedback_message("script", &diags)));
            continue;
        }
        // Valid. Optionally look at the result.
        if !vision_done {
            if let Some((sheet, captions)) = oneiro_render::preview::contact_sheet(&ep, 6, 480, 2) {
                if let Ok(png) = sheet.encode_png() {
                    progress("looking at the shots...");
                    messages.push(Message::assistant(resp.text.clone()));
                    messages.push(
                        Message::user(format!(
                            "The engine accepted the script. Attached is a contact sheet with one frame from each shot (left to right, top to bottom):\n{}\n\nCheck the framing: characters cut off or hidden behind each other, a camera pointing at nothing, a shot that does not show who is speaking. If everything works, reply exactly: LOOKS GOOD. Otherwise explain briefly and return the COMPLETE corrected script.",
                            captions.join("\n")
                        ))
                        .with_images(vec![png]),
                    );
                    let check = provider.complete(&ChatRequest { system: system.clone(), messages: messages.clone(), max_tokens: opts.max_tokens })?;
                    add_usage(&mut result.usage, &check);
                    result.rounds += 1;
                    let (why, fixed) = extract_block(&check.text, &["oneiro", "onr"], "oneiro ");
                    match fixed {
                        None => {
                            result.notes.push("visual check: the model found the framing fine".into());
                        }
                        Some(fixed_code) => {
                            let (_, d2) = compile_source(ctx.project, &ctx.script_label, &fixed_code, ctx.lock);
                            if errors_of(&d2).is_empty() {
                                result.notes.push(format!("visual check: framing adjusted ({})", why.chars().take(160).collect::<String>()));
                                result.script = Some(fixed_code);
                                result.diagnostics = d2;
                                result.reply = if why.is_empty() { result.reply.clone() } else { why };
                            } else {
                                result.notes.push("visual check: the model's revision had errors, keeping the previous valid script".into());
                            }
                        }
                    }
                }
            }
        }
        return Ok(result);
    }
    Ok(result)
}

/// Validates character source: exactly one `character` block that the engine accepts.
pub fn validate_character(code: &str) -> (Option<CharacterDef>, Vec<Diagnostic>) {
    let (file, mut diags) = oneiro_lang::parse(code, "character.onchar");
    let blocks: Vec<_> = file.blocks.iter().filter(|b| b.kind == "character").collect();
    if blocks.len() != 1 {
        diags.push(Diagnostic::error(
            "E1100",
            "character.onchar",
            oneiro_lang::Pos { line: 1, col: 1 },
            format!("expected exactly one `character` block, found {}", blocks.len()),
        ));
        return (None, diags);
    }
    let def = CharacterDef::from_block(blocks[0], "character.onchar", &mut diags);
    if !def.name.chars().next().map_or(false, |c| c.is_ascii_uppercase()) || !def.name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        diags.push(Diagnostic::error("E1100", "character.onchar", blocks[0].pos, format!("invalid character name '{}'", def.name)).suggest("start with a capital letter; use only letters, digits and underscores"));
    }
    if diags.iter().any(|d| d.is_error()) {
        (None, diags)
    } else {
        (Some(def), diags)
    }
}

pub fn direct_character(
    provider: &dyn Provider,
    project: &Project,
    description: &str,
    history: &[Turn],
    opts: &DirectorOpts,
    progress: &mut dyn FnMut(&str),
) -> Result<CharacterResult, String> {
    let system = guide::system_prompt(project, Mode::Character);
    let mut messages: Vec<Message> = Vec::new();
    for t in history {
        messages.push(Message::user(t.user.clone()));
        messages.push(Message::assistant(t.assistant.clone()));
    }
    messages.push(Message::user(description.trim().to_string()));
    let mut out = CharacterResult { reply: String::new(), source: None, def: None, valid: false, diagnostics: vec![], rounds: 0, usage: (0, 0) };
    for round in 1..=opts.max_rounds.max(1) {
        progress(&format!("asking {} (round {})...", provider.name(), round));
        let resp = provider.complete(&ChatRequest { system: system.clone(), messages: messages.clone(), max_tokens: opts.max_tokens.min(4000) })?;
        add_usage(&mut out.usage, &resp);
        out.rounds = round;
        let (explanation, code) = extract_block(&resp.text, &["onchar", "oneiro"], "character ");
        out.reply = explanation;
        let Some(code) = code else {
            out.valid = false;
            return Ok(out);
        };
        let (def, diags) = validate_character(&code);
        out.diagnostics = diags.clone();
        if let Some(def) = def {
            out.source = Some(def.to_source());
            out.def = Some(def);
            out.valid = true;
            return Ok(out);
        }
        out.source = Some(code);
        messages.push(Message::assistant(resp.text.clone()));
        messages.push(Message::user(feedback_message("character", &diags)));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_tagged_block_and_the_explanation() {
        let t = "Here you go.\n```oneiro\noneiro 0.1\nproject \"x\"\n```\nEnjoy.";
        let (why, code) = extract_block(t, &["oneiro"], "oneiro ");
        assert_eq!(code.unwrap(), "oneiro 0.1\nproject \"x\"");
        assert_eq!(why, "Here you go.\nEnjoy.");
    }

    #[test]
    fn prefers_the_last_block_and_accepts_untagged_ones() {
        let t = "```text\nnot this\n```\nlater\n```\noneiro 0.1\n```";
        let (_, code) = extract_block(t, &["oneiro"], "oneiro ");
        assert_eq!(code.unwrap(), "oneiro 0.1");
        let (why, code) = extract_block("just words", &["oneiro"], "oneiro ");
        assert!(code.is_none() && why == "just words");
        // an unterminated fence is ignored
        let (_, code) = extract_block("```oneiro\noneiro 0.1", &["oneiro"], "oneiro ");
        assert!(code.is_none());
    }

    #[test]
    fn character_validation_catches_bad_parts_and_names() {
        let ok = "character Luna {\n  rig: library.rigs.biped_cutout\n  build { head: library.heads.round }\n}";
        assert!(validate_character(ok).0.is_some());
        let bad = "character luna {\n  build { head: library.heads.rond }\n}";
        let (def, d) = validate_character(bad);
        assert!(def.is_none() && d.iter().any(|x| x.code == "E1103") && d.iter().any(|x| x.message.contains("invalid character name")));
        assert!(validate_character("no block here").0.is_none());
    }
}
