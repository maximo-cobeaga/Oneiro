//! The AI tab: describe what you want, the director writes it, the engine validates it, and you
//! review a diff before anything touches your script. Works with any provider (Claude/Codex CLI
//! under your own login, Anthropic API, local OpenAI-compatible servers).

use crate::app::{App, RightTab};
use eframe::egui::{self, Color32, RichText};
use oneiro_ai::config::ProviderKind;
use oneiro_ai::{AiConfig, CharacterResult, DirectorOpts, ProjectCtx, SceneResult, Turn};
use std::sync::mpsc::{channel, Receiver};
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AiMode {
    Scene,
    Character,
}

pub enum Proposal {
    None,
    Script { text: String, valid: bool, problems: Vec<String>, diff: Vec<(DiffTag, String)>, state: ProposalState },
    /// `svgs` is non-empty for characters drawn as vector parts (tier B): (path in the project, SVG text).
    Character { source: String, name: String, svgs: Vec<(String, String)>, state: ProposalState },
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProposalState {
    Pending,
    Applied,
    Rejected,
}

pub enum ChatItem {
    User(String),
    Ai { text: String, notes: Vec<String>, proposal: Proposal, footer: String },
    Error(String),
    Info(String),
}

pub enum AiMsg {
    Progress(String),
    Scene(Result<SceneResult, String>, String),
    Character(Result<CharacterResult, String>, String),
    Parts(Result<oneiro_ai::PartsResult, String>, String),
    Test(Result<String, String>),
}

pub struct AiJob {
    pub rx: Receiver<AiMsg>,
    pub started: Instant,
    pub status: String,
}

pub struct AiState {
    pub cfg: AiConfig,
    pub session_key: String,
    pub mode: AiMode,
    pub input: String,
    pub chat: Vec<ChatItem>,
    pub history: Vec<Turn>,
    pub job: Option<AiJob>,
    pub undo: Vec<String>,
    pub show_settings: bool,
    /// Character mode: draw the character as vector (SVG) parts instead of picking library blocks.
    pub parts: bool,
}

impl Default for AiState {
    fn default() -> Self {
        AiState {
            cfg: AiConfig::load(),
            session_key: String::new(),
            mode: AiMode::Scene,
            input: String::new(),
            chat: vec![ChatItem::Info("Describe what you want. For example: \"add a scene where Blob drops the pie and Marta laughs\", \"make the ending funnier\" or \"cut it to 20 seconds\". Nothing is changed until you press Apply.".into())],
            history: vec![],
            job: None,
            undo: vec![],
            show_settings: false,
            parts: false,
        }
    }
}

// ---------------------------------------------------------------------------- diff

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DiffTag {
    Same,
    Added,
    Removed,
}

/// Line diff (longest common subsequence), good enough for scripts of a few hundred lines.
pub fn diff_lines(old: &str, new: &str) -> Vec<(DiffTag, String)> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    let (n, m) = (a.len(), b.len());
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            dp[i][j] = if a[i] == b[j] { dp[i + 1][j + 1] + 1 } else { dp[i + 1][j].max(dp[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((DiffTag::Same, a[i].to_string()));
            i += 1;
            j += 1;
        } else if dp[i + 1][j] >= dp[i][j + 1] {
            out.push((DiffTag::Removed, a[i].to_string()));
            i += 1;
        } else {
            out.push((DiffTag::Added, b[j].to_string()));
            j += 1;
        }
    }
    while i < n {
        out.push((DiffTag::Removed, a[i].to_string()));
        i += 1;
    }
    while j < m {
        out.push((DiffTag::Added, b[j].to_string()));
        j += 1;
    }
    out
}

pub fn diff_stats(d: &[(DiffTag, String)]) -> (usize, usize) {
    (d.iter().filter(|x| x.0 == DiffTag::Added).count(), d.iter().filter(|x| x.0 == DiffTag::Removed).count())
}

// ---------------------------------------------------------------------------- app integration

impl App {
    pub fn ai_busy(&self) -> bool {
        self.ai.job.is_some()
    }

    /// Starts a scene request in a background thread.
    pub fn ai_send(&mut self, ctx: &egui::Context, request: String) {
        let request = request.trim().to_string();
        if request.is_empty() || self.ai_busy() {
            return;
        }
        let Some(ws) = &self.ws else {
            self.ai.chat.push(ChatItem::Error("Open a project first.".into()));
            return;
        };
        let project = ws.project.clone();
        let lock = ws.lock.clone();
        let script = ws.script_doc().text.clone();
        let label = ws.label();
        let cfg = self.ai.cfg.clone();
        let key = self.ai.session_key.clone();
        let history = self.ai.history.clone();
        let mode = self.ai.mode;
        let parts = self.ai.parts;
        self.ai.chat.push(ChatItem::User(request.clone()));
        let (tx, rx) = channel();
        self.ai.job = Some(AiJob { rx, started: Instant::now(), status: "starting...".into() });
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let provider = match cfg.build_provider(Some(&key)) {
                Ok(p) => p,
                Err(e) => {
                    let _ = tx.send(match mode {
                        AiMode::Scene => AiMsg::Scene(Err(e), request),
                        AiMode::Character if parts => AiMsg::Parts(Err(e), request),
                        AiMode::Character => AiMsg::Character(Err(e), request),
                    });
                    c2.request_repaint();
                    return;
                }
            };
            let opts = DirectorOpts { max_rounds: cfg.max_rounds, self_check: cfg.self_check, ..Default::default() };
            let tx2 = tx.clone();
            let c3 = c2.clone();
            let mut progress = move |m: &str| {
                let _ = tx2.send(AiMsg::Progress(m.to_string()));
                c3.request_repaint();
            };
            let msg = match mode {
                AiMode::Scene => {
                    let pctx = ProjectCtx { project: &project, lock: &lock, script_label: label };
                    AiMsg::Scene(oneiro_ai::direct_scene(provider.as_ref(), &pctx, &script, &history, &request, &opts, &mut progress), request)
                }
                AiMode::Character if parts => AiMsg::Parts(oneiro_ai::direct_character_parts(provider.as_ref(), &project, &request, &history, &opts, &mut progress), request),
                AiMode::Character => AiMsg::Character(oneiro_ai::direct_character(provider.as_ref(), &project, &request, &history, &opts, &mut progress), request),
            };
            let _ = tx.send(msg);
            c2.request_repaint();
        });
    }

    pub fn ai_test_connection(&mut self, ctx: &egui::Context) {
        if self.ai_busy() {
            return;
        }
        let cfg = self.ai.cfg.clone();
        let key = self.ai.session_key.clone();
        let (tx, rx) = channel();
        self.ai.job = Some(AiJob { rx, started: Instant::now(), status: "testing the connection...".into() });
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let r = cfg.build_provider(Some(&key)).and_then(|p| {
                let name = p.name();
                p.complete(&oneiro_ai::ChatRequest {
                    system: "You are a connection test. Follow the instruction exactly.".into(),
                    messages: vec![oneiro_ai::Message::user("Reply with the single word OK and nothing else.")],
                    max_tokens: 20,
                })
                .map(|r| format!("{} answered: {}", name, r.text.trim()))
            });
            let _ = tx.send(AiMsg::Test(r));
            c2.request_repaint();
        });
    }

    /// Called every frame: collects messages from the background job.
    pub fn ai_poll(&mut self) {
        let mut finished: Option<AiMsg> = None;
        if let Some(job) = &mut self.ai.job {
            while let Ok(m) = job.rx.try_recv() {
                match m {
                    AiMsg::Progress(s) => job.status = s,
                    other => {
                        finished = Some(other);
                        break;
                    }
                }
            }
        }
        let Some(msg) = finished else { return };
        let elapsed = self.ai.job.as_ref().map(|j| j.started.elapsed().as_secs_f32()).unwrap_or(0.0);
        self.ai.job = None;
        match msg {
            AiMsg::Progress(_) => {}
            AiMsg::Test(Ok(t)) => self.ai.chat.push(ChatItem::Info(format!("Connection OK. {}", t))),
            AiMsg::Test(Err(e)) => self.ai.chat.push(ChatItem::Error(e)),
            AiMsg::Scene(Err(e), _) | AiMsg::Character(Err(e), _) | AiMsg::Parts(Err(e), _) => self.ai.chat.push(ChatItem::Error(e)),
            AiMsg::Scene(Ok(r), request) => {
                let footer = format!("{} model call(s), {:.0}s{}", r.rounds, elapsed, if r.usage.0 + r.usage.1 > 0 { format!(", {} in / {} out tokens", r.usage.0, r.usage.1) } else { String::new() });
                let problems: Vec<String> = r.diagnostics.iter().filter(|d| d.is_error()).map(|d| d.render()).collect();
                let proposal = match r.script {
                    Some(text) => {
                        let current = self.ws.as_ref().map(|w| w.script_doc().text.clone()).unwrap_or_default();
                        let diff = diff_lines(&current, &text);
                        Proposal::Script { text, valid: r.valid, problems, diff, state: ProposalState::Pending }
                    }
                    None => Proposal::None,
                };
                self.ai.history.push(Turn { user: request, assistant: r.reply.clone() });
                self.ai.chat.push(ChatItem::Ai { text: r.reply, notes: r.notes, proposal, footer });
            }
            AiMsg::Parts(Ok(r), request) => {
                let footer = format!("{} model call(s), {:.0}s", r.rounds, elapsed);
                let proposal = match (&r.def, &r.source) {
                    (Some(def), Some(src)) if r.valid => Proposal::Character { source: src.clone(), name: def.name.clone(), svgs: r.svgs.clone(), state: ProposalState::Pending },
                    _ => Proposal::None,
                };
                let mut text = r.reply.clone();
                if !r.valid {
                    text.push_str("\n\nThe drawing still has problems:\n");
                    for d in r.diagnostics.iter().filter(|d| d.is_error()) {
                        text.push_str(&format!("{}\n", d.render()));
                    }
                }
                self.ai.history.push(Turn { user: request, assistant: r.reply });
                self.ai.chat.push(ChatItem::Ai { text, notes: vec![], proposal, footer });
            }
            AiMsg::Character(Ok(r), request) => {
                let footer = format!("{} model call(s), {:.0}s", r.rounds, elapsed);
                let proposal = match (&r.def, &r.source) {
                    (Some(def), Some(src)) if r.valid => Proposal::Character { source: src.clone(), name: def.name.clone(), svgs: vec![], state: ProposalState::Pending },
                    _ => Proposal::None,
                };
                let mut text = r.reply.clone();
                if !r.valid {
                    text.push_str("\n\nThe design still has problems:\n");
                    for d in r.diagnostics.iter().filter(|d| d.is_error()) {
                        text.push_str(&format!("{}\n", d.render()));
                    }
                }
                self.ai.history.push(Turn { user: request, assistant: r.reply });
                self.ai.chat.push(ChatItem::Ai { text, notes: vec![], proposal, footer });
            }
        }
    }

    /// Replaces the script with an AI proposal (the previous text goes on the undo stack).
    fn ai_apply_script(&mut self, text: &str) {
        if let Some(ws) = &mut self.ws {
            let idx = ws.script;
            self.ai.undo.push(ws.docs[idx].text.clone());
            ws.docs[idx].text = text.to_string();
            ws.active = idx;
            ws.recompile();
            self.vp.invalidate();
            self.status = "Applied the AI's script (not saved yet - Ctrl+S saves, Undo AI change reverts)".into();
        }
    }

    fn ai_undo(&mut self) {
        if let (Some(prev), Some(ws)) = (self.ai.undo.pop(), &mut self.ws) {
            let idx = ws.script;
            ws.docs[idx].text = prev;
            ws.recompile();
            self.vp.invalidate();
            self.status = "Reverted the last AI change".into();
        }
    }

    /// "Fix with AI" from the problems list.
    pub fn ai_fix_problems(&mut self, ctx: &egui::Context) {
        let Some(ws) = &self.ws else { return };
        let problems: Vec<String> = ws.diags.iter().filter(|d| d.is_error()).map(|d| d.render()).collect();
        if problems.is_empty() {
            return;
        }
        self.right_tab = RightTab::Ai;
        self.ai.mode = AiMode::Scene;
        let req = format!("The script has these problems. Fix them and change nothing else:\n{}", problems.join("\n"));
        self.ai_send(ctx, req);
    }

    pub fn ui_ai(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        self.ai_poll();
        if self.ai_busy() {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }

        // ------------------------------------------------------------ provider and settings
        ui.horizontal(|ui| {
            let current = self.ai.cfg.kind;
            egui::ComboBox::from_id_salt("ai_provider").width(300.0).selected_text(current.label()).show_ui(ui, |ui| {
                for k in ProviderKind::ALL {
                    ui.selectable_value(&mut self.ai.cfg.kind, k, k.label());
                }
            });
            if ui.button(if self.ai.show_settings { "Hide settings" } else { "Settings" }).clicked() {
                self.ai.show_settings = !self.ai.show_settings;
            }
            if ui.add_enabled(!self.ai_busy(), egui::Button::new("Test")).on_hover_text("Send a tiny request to check the connection").clicked() {
                self.ai_test_connection(&ctx);
            }
        });
        if self.ai.show_settings {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let kind = self.ai.cfg.kind;
                match kind {
                    ProviderKind::ClaudeCli | ProviderKind::CodexCli => {
                        ui.label(RichText::new("Uses the CLI you already have installed and logged in (`claude` / `codex`), with your own subscription. Oneiro never sees your credentials.").weak().small());
                        ui.horizontal(|ui| {
                            ui.label("model (optional)");
                            ui.text_edit_singleline(&mut self.ai.cfg.model);
                        });
                    }
                    ProviderKind::AnthropicApi => {
                        ui.horizontal(|ui| {
                            ui.label("model");
                            ui.add(egui::TextEdit::singleline(&mut self.ai.cfg.model).hint_text("claude-sonnet-5-5"));
                        });
                        ui.horizontal(|ui| {
                            ui.label("key env var");
                            ui.add(egui::TextEdit::singleline(&mut self.ai.cfg.api_key_env).hint_text("ANTHROPIC_API_KEY"));
                        });
                        ui.horizontal(|ui| {
                            ui.label("or key (this session only)");
                            ui.add(egui::TextEdit::singleline(&mut self.ai.session_key).password(true).desired_width(160.0));
                        });
                        ui.label(RichText::new("Keys are never saved to disk.").weak().small());
                    }
                    ProviderKind::OpenAiCompatible => {
                        ui.horizontal(|ui| {
                            ui.label("base URL");
                            ui.text_edit_singleline(&mut self.ai.cfg.base_url);
                        });
                        ui.horizontal(|ui| {
                            ui.label("model");
                            ui.add(egui::TextEdit::singleline(&mut self.ai.cfg.model).hint_text("llama3.1"));
                        });
                        ui.horizontal(|ui| {
                            ui.label("key env var (optional)");
                            ui.text_edit_singleline(&mut self.ai.cfg.api_key_env);
                        });
                        ui.checkbox(&mut self.ai.cfg.vision, "model can see images (enables visual self-check)");
                        ui.label(RichText::new("Ollama: http://localhost:11434/v1   LM Studio: http://localhost:1234/v1").weak().small());
                    }
                    ProviderKind::CustomCli => {
                        ui.horizontal(|ui| {
                            ui.label("command");
                            ui.add(egui::TextEdit::singleline(&mut self.ai.cfg.custom_command).hint_text("my-agent --flag"));
                        });
                        ui.label(RichText::new("The prompt is sent on stdin; the reply is read from stdout.").weak().small());
                    }
                }
                ui.checkbox(&mut self.ai.cfg.self_check, "Visual self-check (the model looks at a contact sheet of the shots; needs image support)");
                ui.add(egui::Slider::new(&mut self.ai.cfg.max_rounds, 1..=8).text("max correction rounds"));
                ui.horizontal(|ui| {
                    if ui.button("Save settings").clicked() {
                        match self.ai.cfg.save() {
                            Ok(_) => self.status = format!("Saved {}", oneiro_ai::config::config_path().display()),
                            Err(e) => self.status = e,
                        }
                    }
                    if ui.button("Copy MCP setup command").on_hover_text("Lets Claude Code / Codex drive this project through the Oneiro MCP server").clicked() {
                        let cmd = self.mcp_setup_command();
                        ctx.copy_text(cmd);
                        self.status = "MCP setup command copied to the clipboard".into();
                    }
                });
            });
        }
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.ai.mode, AiMode::Scene, "Write / edit scenes");
            ui.selectable_value(&mut self.ai.mode, AiMode::Character, "Design a character");
        });
        if self.ai.mode == AiMode::Character {
            ui.checkbox(&mut self.ai.parts, "Draw it as vector parts (SVG) instead of library blocks")
                .on_hover_text("The AI draws the head, body, arm and leg as SVG; the engine checks them and Oneiro animates and lip-syncs the result. Needs a capable model.");
        }
        ui.separator();

        // ------------------------------------------------------------ input (bottom) and transcript
        let mut send = false;
        egui::Panel::bottom("ai_input").resizable(false).show(ui, |ui| {
            if let Some(job) = &self.ai.job {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(format!("{}  ({:.0}s)", job.status, job.started.elapsed().as_secs_f32()));
                });
            }
            let hint = match self.ai.mode {
                AiMode::Scene => "What should happen? (Ctrl+Enter to send)",
                AiMode::Character => "Describe the character: look, age, personality... (Ctrl+Enter to send)",
            };
            let out = egui::TextEdit::multiline(&mut self.ai.input).desired_rows(3).desired_width(f32::INFINITY).hint_text(hint).show(ui);
            let ctrl_enter = out.response.has_focus() && ctx.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
            ui.horizontal(|ui| {
                if ui.add_enabled(!self.ai_busy() && !self.ai.input.trim().is_empty(), egui::Button::new(RichText::new("Send").strong())).clicked() || (ctrl_enter && !self.ai_busy()) {
                    send = true;
                }
                if !self.ai.undo.is_empty() && ui.button("Undo AI change").clicked() {
                    self.ai_undo();
                }
                if ui.button("Clear chat").clicked() {
                    self.ai.chat.truncate(1);
                    self.ai.history.clear();
                }
            });
            if self.ai.mode == AiMode::Scene {
                ui.horizontal_wrapped(|ui| {
                    for s in ["Add a new scene", "Make it funnier", "Shorten to about 20 seconds", "Improve the camera work", "Add a dramatic ending"] {
                        if ui.small_button(s).clicked() {
                            self.ai.input = s.to_string();
                        }
                    }
                });
            }
        });
        if send {
            let text = std::mem::take(&mut self.ai.input);
            self.ai_send(&ctx, text);
        }

        let mut apply_script: Option<(usize, String)> = None;
        let mut reject: Option<usize> = None;
        let mut apply_char: Option<(usize, String, String, Vec<(String, String)>)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
            for (i, item) in self.ai.chat.iter().enumerate() {
                match item {
                    ChatItem::User(t) => {
                        egui::Frame::group(ui.style()).fill(Color32::from_rgb(40, 38, 62)).show(ui, |ui| {
                            ui.label(RichText::new("You").strong().small());
                            ui.label(t);
                        });
                    }
                    ChatItem::Info(t) => {
                        ui.label(RichText::new(t).weak());
                    }
                    ChatItem::Error(t) => {
                        egui::Frame::group(ui.style()).fill(Color32::from_rgb(70, 30, 30)).show(ui, |ui| {
                            ui.label(RichText::new("Problem").strong().small());
                            ui.label(t);
                        });
                    }
                    ChatItem::Ai { text, notes, proposal, footer } => {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.label(RichText::new("Director").strong().small().color(Color32::from_rgb(170, 130, 255)));
                            if !text.is_empty() {
                                ui.label(text);
                            }
                            for n in notes {
                                ui.label(RichText::new(n).weak().small());
                            }
                            match proposal {
                                Proposal::None => {}
                                Proposal::Script { text: new_text, valid, problems, diff, state } => {
                                    let d = diff;
                                    let (add, rem) = diff_stats(&d);
                                    ui.horizontal(|ui| {
                                        if *valid {
                                            ui.label(RichText::new("valid").color(Color32::from_rgb(120, 220, 150)));
                                        } else {
                                            ui.label(RichText::new("has errors").color(Color32::from_rgb(240, 110, 100)));
                                        }
                                        ui.label(format!("+{} -{} lines", add, rem));
                                    });
                                    for p in problems {
                                        ui.label(RichText::new(p).small().color(Color32::from_rgb(240, 150, 140)));
                                    }
                                    egui::CollapsingHeader::new("Show changes").id_salt(("diff", i)).show(ui, |ui| {
                                        for (tag, line) in d.iter().filter(|(t, _)| *t != DiffTag::Same) {
                                            let (prefix, col) = if *tag == DiffTag::Added { ("+ ", Color32::from_rgb(130, 220, 150)) } else { ("- ", Color32::from_rgb(240, 130, 130)) };
                                            ui.label(RichText::new(format!("{}{}", prefix, line)).monospace().color(col));
                                        }
                                        if add + rem == 0 {
                                            ui.label("No changes.");
                                        }
                                    });
                                    match state {
                                        ProposalState::Pending => {
                                            ui.horizontal(|ui| {
                                                if ui.add_enabled(*valid, egui::Button::new(RichText::new("Apply to script").strong())).clicked() {
                                                    apply_script = Some((i, new_text.clone()));
                                                }
                                                if ui.button("Reject").clicked() {
                                                    reject = Some(i);
                                                }
                                            });
                                        }
                                        ProposalState::Applied => {
                                            ui.label(RichText::new("applied").weak());
                                        }
                                        ProposalState::Rejected => {
                                            ui.label(RichText::new("rejected").weak());
                                        }
                                    }
                                }
                                Proposal::Character { source, name, svgs, state } => {
                                    ui.label(RichText::new(format!("character {}{}", name, if svgs.is_empty() { "" } else { " (vector parts)" })).strong());
                                    ui.add(egui::Label::new(RichText::new(source).monospace().small()).wrap());
                                    match state {
                                        ProposalState::Pending => {
                                            ui.horizontal(|ui| {
                                                let label = if svgs.is_empty() { "Preview in the builder" } else { "Save to the library and open" };
                                                if ui.button(label).clicked() {
                                                    apply_char = Some((i, name.clone(), source.clone(), svgs.clone()));
                                                }
                                                if ui.button("Reject").clicked() {
                                                    reject = Some(i);
                                                }
                                            });
                                        }
                                        ProposalState::Applied => {
                                            ui.label(RichText::new("opened in the builder (save it there)").weak());
                                        }
                                        ProposalState::Rejected => {
                                            ui.label(RichText::new("rejected").weak());
                                        }
                                    }
                                }
                            }
                            ui.label(RichText::new(footer).weak().small());
                        });
                    }
                }
                ui.add_space(4.0);
            }
        });
        if let Some((i, text)) = apply_script {
            self.ai_apply_script(&text);
            if let Some(ChatItem::Ai { proposal: Proposal::Script { state, .. }, .. }) = self.ai.chat.get_mut(i) {
                *state = ProposalState::Applied;
            }
        }
        if let Some((i, name, source, svgs)) = apply_char {
            if !svgs.is_empty() {
                // Vector character: the SVG files must exist for the character to load, so it is saved right away.
                let root = self.ws.as_ref().map(|w| w.root.clone());
                if let Some(root) = root {
                    match oneiro_ai::parts::save_files(&root, &name, &source, &svgs) {
                        Ok(path) => {
                            if let Some(ws) = &mut self.ws {
                                ws.reload_project();
                                ws.recompile();
                            }
                            self.thumbs.clear();
                            self.vp.invalidate();
                            self.builder.select(self.ws.as_ref(), &name);
                            self.right_tab = RightTab::Character;
                            self.status = format!("Saved {} and its {} SVG parts", path.display(), svgs.len());
                        }
                        Err(e) => self.status = format!("Could not save the character: {}", e),
                    }
                }
            } else {
                let (file, _) = oneiro_lang::parse(&source, "ai");
                if let Some(b) = file.blocks.first() {
                    let mut d = vec![];
                    let def = oneiro_core::CharacterDef::from_block(b, "ai", &mut d);
                    self.builder.load_draft(def, true);
                    self.right_tab = RightTab::Character;
                }
            }
            if let Some(ChatItem::Ai { proposal: Proposal::Character { state, .. }, .. }) = self.ai.chat.get_mut(i) {
                *state = ProposalState::Applied;
            }
            let _ = name;
        }
        if let Some(i) = reject {
            match self.ai.chat.get_mut(i) {
                Some(ChatItem::Ai { proposal: Proposal::Script { state, .. }, .. }) | Some(ChatItem::Ai { proposal: Proposal::Character { state, .. }, .. }) => {
                    *state = ProposalState::Rejected
                }
                _ => {}
            }
        }
    }

    /// `claude mcp add ...` line that registers this project's MCP server.
    pub fn mcp_setup_command(&self) -> String {
        let exe = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join(if cfg!(windows) { "oneiro-mcp.exe" } else { "oneiro-mcp" })));
        let exe = exe.filter(|p| p.exists()).map(|p| p.display().to_string()).unwrap_or_else(|| "oneiro-mcp".into());
        let root = self.ws.as_ref().map(|w| w.root.display().to_string()).unwrap_or_else(|| ".".into());
        format!("claude mcp add oneiro -- \"{}\" \"{}\"", exe, root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_finds_added_and_removed_lines() {
        let old = "a\nb\nc\nd";
        let new = "a\nb2\nc\nd\ne";
        let d = diff_lines(old, new);
        let (add, rem) = diff_stats(&d);
        assert_eq!((add, rem), (2, 1));
        assert!(d.contains(&(DiffTag::Removed, "b".into())) && d.contains(&(DiffTag::Added, "b2".into())) && d.contains(&(DiffTag::Added, "e".into())));
        assert_eq!(diff_stats(&diff_lines(old, old)), (0, 0));
        assert_eq!(diff_stats(&diff_lines("", "x\ny")), (2, 0));
    }

    #[test]
    fn diff_reassembles_the_new_text() {
        let old = "scene {\n  a\n  b\n}";
        let new = "scene {\n  a\n  x\n  y\n  b\n}";
        let d = diff_lines(old, new);
        let rebuilt: Vec<&str> = d.iter().filter(|(t, _)| *t != DiffTag::Removed).map(|(_, l)| l.as_str()).collect();
        assert_eq!(rebuilt.join("\n"), new);
        let original: Vec<&str> = d.iter().filter(|(t, _)| *t != DiffTag::Added).map(|(_, l)| l.as_str()).collect();
        assert_eq!(original.join("\n"), old);
    }
}

impl App {
    /// Applies the newest pending, valid script proposal (used by the scripted startup mode).
    pub fn ai_apply_latest_pending(&mut self) {
        let found = self.ai.chat.iter().enumerate().rev().find_map(|(i, c)| match c {
            ChatItem::Ai { proposal: Proposal::Script { text, valid: true, state: ProposalState::Pending, .. }, .. } => Some((i, text.clone())),
            _ => None,
        });
        if let Some((i, text)) = found {
            self.ai_apply_script(&text);
            if let Some(ChatItem::Ai { proposal: Proposal::Script { state, .. }, .. }) = self.ai.chat.get_mut(i) {
                *state = ProposalState::Applied;
            }
        }
    }
}
