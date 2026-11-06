//! Right-hand panel: tabs (Script / Character / Inspector) and the script editor with syntax
//! highlighting, diagnostics, a directing palette and live recompilation.

use crate::app::{App, RightTab};
use crate::highlight::layout_job;
use crate::textops::{insert_at_cursor, line_col_to_char, line_of_char, line_start};
use crate::theme;
use eframe::egui::{self, text::CCursor, text::CCursorRange, Color32, FontId, RichText};
use oneiro_core::catalog::*;
use std::collections::HashSet;
use std::time::Instant;

impl App {
    pub fn ui_right(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.right_tab, RightTab::Script, "Script");
            ui.selectable_value(&mut self.right_tab, RightTab::Character, "Character builder");
            ui.selectable_value(&mut self.right_tab, RightTab::Inspector, "Inspector");
            ui.selectable_value(&mut self.right_tab, RightTab::Audio, "Audio");
            ui.selectable_value(&mut self.right_tab, RightTab::Ai, "AI");
        });
        ui.separator();
        match self.right_tab {
            RightTab::Script => self.ui_script(ui),
            RightTab::Character => self.ui_builder(ui),
            RightTab::Inspector => self.ui_inspector(ui),
            RightTab::Audio => self.ui_audio(ui),
            RightTab::Ai => self.ui_ai(ui),
        }
    }

    /// The actor the palette inserts lines for.
    fn palette_actor(&self) -> String {
        let ep = self.ws.as_ref().map(|w| w.episode.clone());
        if let Some(sel) = &self.sel_actor {
            return sel.clone();
        }
        ep.and_then(|e| e.locate(self.t).and_then(|(si, _)| e.scenes[si].actors.first().map(|a| a.name.clone()))).unwrap_or_else(|| "Marta".into())
    }

    fn palette(&mut self, ui: &mut egui::Ui) {
        let who = self.palette_actor();
        let Some(ep) = self.ws.as_ref().map(|w| w.episode.clone()) else { return };
        let (set_name, anchors, others): (String, Vec<String>, Vec<String>) = match ep.locate(self.t) {
            Some((si, _)) => {
                let sc = &ep.scenes[si];
                (
                    sc.set.name.clone(),
                    sc.set.anchors.keys().cloned().collect(),
                    sc.actors.iter().filter(|a| a.name != who).map(|a| a.name.clone()).collect(),
                )
            }
            None => ("set".into(), vec![], vec![]),
        };
        let anchor = anchors.first().map(|a| format!("{}.{}", set_name, a)).unwrap_or_else(|| "(0, 0)".into());
        let target = others.first().cloned().unwrap_or_else(|| anchor.clone());
        let mut insert: Option<String> = None;

        egui::CollapsingHeader::new(RichText::new(format!("Direct: lines for {}", who)).strong()).default_open(false).show(ui, |ui| {
            ui.label(RichText::new("Click to insert a line after the cursor.").weak().small());
            ui.label("Actions");
            ui.horizontal_wrapped(|ui| {
                for v in IMPLEMENTED_VERBS {
                    if *v == "emote" || *v == "stops" {
                        continue;
                    }
                    if ui.small_button(*v).clicked() {
                        let tail = match *v {
                            "walks_to" | "runs_to" | "sits_on" => format!(" {}", anchor),
                            "turns_to" | "looks_at" | "points_at" => format!(" {}", target),
                            _ => String::new(),
                        };
                        insert = Some(format!("{} {}{}", who, v, tail));
                    }
                }
            });
            ui.label("Faces and dialogue");
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("dialogue").clicked() {
                    insert = Some(format!("{} (neutral): \"...\"", who));
                }
                for e in EXPRESSIONS {
                    if ui.small_button(*e).clicked() {
                        insert = Some(format!("{} emote {}", who, e));
                    }
                }
            });
            ui.label("Camera");
            ui.horizontal_wrapped(|ui| {
                for s in SHOTS {
                    if ui.small_button(*s).clicked() {
                        insert = Some(if *s == "over_shoulder" {
                            format!("camera over_shoulder {} -> {}", who, others.first().cloned().unwrap_or_else(|| "Other".into()))
                        } else if *s == "wide" || *s == "extreme_wide" || *s == "establishing" {
                            format!("camera {}", s)
                        } else {
                            format!("cut to {} on {}", s, who)
                        });
                    }
                }
            });
            ui.horizontal_wrapped(|ui| {
                ui.label("move:");
                for m in MOVES {
                    if *m != "static" && ui.small_button(*m).clicked() {
                        insert = Some(format!("camera medium on {} move {} for 3s", who, m));
                    }
                }
            });
            ui.label("Timing and sound");
            ui.horizontal_wrapped(|ui| {
                if ui.small_button("wait 1s").clicked() {
                    insert = Some("wait 1s".into());
                }
                if ui.small_button("together { }").clicked() {
                    insert = Some(format!("together {{\n    {} nods\n  }}", who));
                }
                if ui.small_button("sfx").clicked() {
                    insert = Some("sfx door_bell".into());
                }
                if ui.small_button("time: night").clicked() {
                    insert = Some("time: night".into());
                }
            });
        });
        if let Some(s) = insert {
            self.insert_request = Some(s);
        }
    }

    pub fn ui_script(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.ws.is_none() {
            ui.label("No project open.");
            return;
        }
        // ------------------------------------------------------------ document tabs
        let mut switch_to = None;
        let mut close = None;
        let mut save = false;
        ui.horizontal_wrapped(|ui| {
            let ws = self.ws.as_ref().unwrap();
            for (i, d) in ws.docs.iter().enumerate() {
                let label = format!("{}{}", d.title(), if d.dirty() { " *" } else { "" });
                if ui.selectable_label(ws.active == i, label).clicked() {
                    switch_to = Some(i);
                }
                if i != ws.script && ui.small_button("x").clicked() {
                    close = Some(i);
                }
            }
            ui.separator();
            if ui.button("Save (Ctrl+S)").clicked() {
                save = true;
            }
        });
        if let Some(i) = switch_to {
            self.ws.as_mut().unwrap().active = i;
        }
        if let Some(i) = close {
            self.ws.as_mut().unwrap().close_doc(i);
        }
        if save {
            self.save_all();
        }
        let is_script = {
            let ws = self.ws.as_ref().unwrap();
            ws.docs[ws.active].kind == crate::workspace::DocKind::Script
        };
        if is_script {
            self.palette(ui);
        } else {
            ui.label(RichText::new("Library file: save with Ctrl+S to reload the project.").weak().small());
        }

        // ------------------------------------------------------------ diagnostics for the active doc
        let (idx, doc_label, diags) = {
            let ws = self.ws.as_ref().unwrap();
            let idx = ws.active;
            let label = ws.docs[idx].path.strip_prefix(&ws.root).unwrap_or(&ws.docs[idx].path).to_string_lossy().replace('\\', "/");
            let diags: Vec<_> = ws.diags.iter().filter(|d| d.file == label).cloned().collect();
            (idx, label, diags)
        };
        let mut jump: Option<(u32, u32)> = None;
        let mut fix_with_ai = false;
        egui::Panel::bottom("diag_panel").resizable(true).default_size(120.0).min_size(46.0).show(ui, |ui| {
            let errors = diags.iter().filter(|d| d.is_error()).count();
            ui.horizontal(|ui| {
                ui.label(RichText::new("Problems").strong());
                let c = if errors > 0 { Color32::from_rgb(240, 110, 100) } else { Color32::from_rgb(120, 220, 150) };
                ui.label(RichText::new(if diags.is_empty() { "none - the script is valid".to_string() } else { format!("{} error(s), {} warning(s)", errors, diags.len() - errors) }).color(c));
                if errors > 0 && ui.small_button("Fix with AI").on_hover_text("Ask the AI to fix these problems (you review the change before applying)").clicked() {
                    fix_with_ai = true;
                }
            });
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for d in &diags {
                    let col = if d.is_error() { Color32::from_rgb(240, 110, 100) } else { Color32::from_rgb(230, 190, 70) };
                    let r = ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(format!("{} {}:{}", d.code, d.line, d.col)).color(col).monospace());
                        ui.label(&d.message);
                    });
                    if let Some(s) = &d.suggestion {
                        ui.label(RichText::new(format!("    {}", s)).weak().small());
                    }
                    if r.response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        jump = Some((d.line, d.col));
                    }
                }
            });
        });
        if fix_with_ai {
            let ctx2 = ctx.clone();
            self.ai_fix_problems(&ctx2);
        }
        if jump.is_some() {
            self.jump_to = jump;
        }

        // ------------------------------------------------------------ editor
        let errs: HashSet<u32> = diags.iter().filter(|d| d.is_error()).map(|d| d.line).collect();
        let warns: HashSet<u32> = diags.iter().filter(|d| !d.is_error()).map(|d| d.line).collect();
        let id = ui.make_persistent_id(("doc_editor", idx));

        // Pending snippet insertion
        if let Some(snippet) = self.insert_request.take() {
            let ws = self.ws.as_mut().unwrap();
            let raw = egui::TextEdit::load_state(&ctx, id).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index.0).unwrap_or(self.cursor_char);
            let is_script_doc = ws.docs[idx].kind == crate::workspace::DocKind::Script;
            let scene_hint = ws.episode.locate(self.t).map(|(si, _)| si);
            let cur = if is_script_doc { crate::textops::insertion_cursor(&ws.docs[idx].text, raw, scene_hint) } else { raw };
            let (text, new_cursor) = insert_at_cursor(&ws.docs[idx].text, cur, &snippet);
            ws.docs[idx].text = text;
            ws.last_edit = Some(Instant::now());
            set_cursor(&ctx, id, new_cursor);
            self.cursor_char = new_cursor;
            ctx.memory_mut(|m| m.request_focus(id));
        }
        // Pending jump
        let mut scroll_line: Option<u32> = None;
        if let Some((line, col)) = self.jump_to.take() {
            let ws = self.ws.as_ref().unwrap();
            let target_doc = ws.script;
            if ws.active != target_doc && ws.docs[ws.active].kind != crate::workspace::DocKind::Script {
                // jumps always refer to the script
            }
            let text = &ws.docs[idx].text;
            let ci = line_col_to_char(text, line, col);
            set_cursor(&ctx, id, ci);
            self.cursor_char = ci;
            scroll_line = Some(line);
            ctx.memory_mut(|m| m.request_focus(id));
        }

        let font = FontId::monospace(13.0);
        let ws = self.ws.as_mut().unwrap();
        let mut changed = false;
        let mut cursor_idx: Option<usize> = None;
        egui::ScrollArea::both().id_salt(("editor_scroll", idx)).auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal_top(|ui| {
                let (_, grect) = ui.allocate_space(egui::vec2(40.0, 1.0));
                let mut layouter = |ui: &egui::Ui, text: &dyn egui::TextBuffer, _wrap: f32| {
                    let job = layout_job(text.as_str(), &errs, &warns, font.clone());
                    ui.fonts_mut(|f| f.layout_job(job))
                };
                let out = egui::TextEdit::multiline(&mut ws.docs[idx].text)
                    .id(id)
                    .font(font.clone())
                    .desired_width(f32::INFINITY)
                    .desired_rows(32)
                    .lock_focus(true)
                    .layouter(&mut layouter)
                    .show(ui);
                if out.response.changed() {
                    changed = true;
                }
                cursor_idx = out.cursor_range.map(|r| r.primary.index.0);

                // line numbers
                let painter = ui.painter();
                for (i, row) in out.galley.rows.iter().enumerate() {
                    let y = out.galley_pos.y + row.pos.y + row.height() / 2.0;
                    let lineno = i as u32 + 1;
                    let col = if errs.contains(&lineno) { Color32::from_rgb(240, 110, 100) } else { Color32::from_gray(100) };
                    painter.text(egui::pos2(grect.left() + 34.0, y), egui::Align2::RIGHT_CENTER, lineno.to_string(), FontId::monospace(11.0), col);
                }
                if let Some(l) = scroll_line {
                    if let Some(row) = out.galley.rows.get(l as usize - 1) {
                        let y = out.galley_pos.y + row.pos.y;
                        ui.scroll_to_rect(egui::Rect::from_min_size(egui::pos2(out.galley_pos.x, y), egui::vec2(10.0, row.height())), Some(egui::Align::Center));
                    }
                }
            });
        });
        if changed {
            ws.last_edit = Some(Instant::now());
        }
        if let Some(ci) = cursor_idx {
            self.cursor_char = ci;
            let line = line_of_char(&ws.docs[idx].text, ci);
            if line != self.last_cursor_line {
                self.last_cursor_line = line;
                if self.follow_cursor && idx == ws.script {
                    let ep = ws.episode.clone();
                    if let Some(span) = ep.scenes.iter().flat_map(|s| s.spans.iter().map(move |sp| (s.t0, sp))).find(|(_, sp)| sp.line == line) {
                        self.t = span.0 + span.1.t0;
                        self.playing = false;
                    }
                }
            }
        }
        let _ = (line_start, theme::ACCENT, &doc_label);
    }
}

fn set_cursor(ctx: &egui::Context, id: egui::Id, idx: usize) {
    let mut state = egui::TextEdit::load_state(ctx, id).unwrap_or_default();
    state.cursor.set_char_range(Some(CCursorRange::one(CCursor::new(idx))));
    state.store(ctx, id);
}
