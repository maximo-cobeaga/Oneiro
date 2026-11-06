//! Left panel: the project's library (episodes, characters, sets, sounds).

use crate::i18n::tr;
use crate::app::{App, RightTab};
use eframe::egui::{self, RichText, TextureOptions};
use oneiro_audio::read_wav;
use oneiro_render::preview::{character_portrait, set_preview, PortraitOpts};

impl App {
    pub fn ui_library(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        ui.heading(tr("Library"));
        let Some(ws) = self.ws.as_ref() else {
            ui.label(tr("No project open."));
            return;
        };
        ui.label(RichText::new(ws.project.manifest.name.clone()).strong());
        ui.label(RichText::new(ws.root.display().to_string()).weak().small());
        ui.separator();

        let scripts = ws.scripts();
        let active_script = ws.script_doc().path.clone();
        let root = ws.root.clone();
        let characters: Vec<(String, String)> = ws.project.characters.iter().map(|(k, v)| (k.clone(), v.to_source())).collect();
        let sets: Vec<(String, oneiro_core::SetDef)> = ws.project.sets.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
        let sounds = ws.sounds();

        let mut open_script: Option<std::path::PathBuf> = None;
        let mut open_doc: Option<std::path::PathBuf> = None;
        let mut pick_char: Option<String> = None;
        let mut new_char = false;
        let mut quick: Option<crate::welcome::QuickKind> = None;
        let mut open_studio = false;

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::CollapsingHeader::new(RichText::new(tr("Episodes")).strong()).default_open(true).show(ui, |ui| {
                for s in &scripts {
                    let label = s.strip_prefix(&root).unwrap_or(s).to_string_lossy().replace('\\', "/");
                    if ui.selectable_label(*s == active_script, label).clicked() {
                        open_script = Some(s.clone());
                    }
                }
                if ui.button(tr("+ New episode")).clicked() {
                    quick = Some(crate::welcome::QuickKind::Episode);
                }
            });
            egui::CollapsingHeader::new(RichText::new(tr("Characters")).strong()).default_open(true).show(ui, |ui| {
                for (name, src) in &characters {
                    let key = oneiro_core::math::hash_str(src);
                    let stale = self.thumbs.get(name).map_or(true, |(k, _)| *k != key);
                    if stale {
                        if let Some(def) = self.ws.as_ref().and_then(|w| w.project.characters.get(name)) {
                            let pm = character_portrait(def, &PortraitOpts::default(), 72, 92);
                            let img = egui::ColorImage::from_rgba_premultiplied([72, 92], pm.data());
                            let tex = ctx.load_texture(format!("thumb_{}", name), img, TextureOptions::LINEAR);
                            self.thumbs.insert(name.clone(), (key, tex));
                        }
                    }
                    ui.horizontal(|ui| {
                        if let Some((_, tex)) = self.thumbs.get(name) {
                            ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(48.0, 61.0))));
                        }
                        ui.vertical(|ui| {
                            if ui.selectable_label(self.builder.draft.as_ref().map_or(false, |d| &d.name == name), name).clicked() {
                                pick_char = Some(name.clone());
                            }
                            if ui.small_button(tr("source")).on_hover_text(tr("Edit the .onchar text")).clicked() {
                                open_doc = Some(root.join("library").join("characters").join(format!("{}.onchar", name)));
                            }
                        });
                    });
                }
                if ui.button(tr("+ New character")).clicked() {
                    new_char = true;
                }
            });
            egui::CollapsingHeader::new(RichText::new(tr("Sets")).strong()).default_open(true).show(ui, |ui| {
                for (name, set) in &sets {
                    let key = oneiro_core::math::hash_str(&format!("{:?}", set));
                    let stale = self.set_thumbs.get(name).map_or(true, |(k, _)| *k != key);
                    if stale {
                        let pm = set_preview(set, 192, 108);
                        let img = egui::ColorImage::from_rgba_premultiplied([192, 108], pm.data());
                        let tex = ctx.load_texture(format!("set_{}", name), img, TextureOptions::LINEAR);
                        self.set_thumbs.insert(name.clone(), (key, tex));
                    }
                    if let Some((_, tex)) = self.set_thumbs.get(name) {
                        ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(160.0, 90.0))));
                    }
                    ui.horizontal(|ui| {
                        ui.label(name);
                        ui.label(RichText::new(format!("{} x {} m, {} anchors", set.width, set.depth, set.anchors.len())).weak().small());
                    });
                    if ui.small_button(tr("edit source")).clicked() {
                        open_doc = Some(root.join("library").join("sets").join(format!("{}.onset", name)));
                    }
                    ui.add_space(4.0);
                }
                if ui.button(tr("+ New set")).clicked() {
                    quick = Some(crate::welcome::QuickKind::Set);
                }
            });
            egui::CollapsingHeader::new(RichText::new(tr("Sounds")).strong()).default_open(false).show(ui, |ui| {
                if sounds.is_empty() {
                    ui.label(RichText::new(tr("No sounds yet. Create some in the Audio tab.")).weak().small());
                }
                for s in &sounds {
                    let label = s.strip_prefix(&root).unwrap_or(s).to_string_lossy().replace('\\', "/");
                    let dur = read_wav(s).map(|a| a.duration()).unwrap_or(0.0);
                    ui.label(format!("{}  ({:.1}s)", label, dur));
                }
                if ui.button(tr("Open the sound studio")).clicked() {
                    open_studio = true;
                }
            });
        });

        if let Some(p) = open_script {
            if let Some(ws) = &mut self.ws {
                if let Ok(i) = ws.open_doc(&p) {
                    ws.active = i;
                    ws.set_script(i);
                    self.t = 0.0;
                    self.playing = false;
                    self.sel_shot = None;
                    self.vp.invalidate();
                    self.right_tab = RightTab::Script;
                }
            }
        }
        if let Some(p) = open_doc {
            if let Some(ws) = &mut self.ws {
                match ws.open_doc(&p) {
                    Ok(i) => {
                        ws.active = i;
                        self.right_tab = RightTab::Script;
                    }
                    Err(e) => self.status = e,
                }
            }
        }
        if let Some(n) = pick_char {
            self.builder.select(self.ws.as_ref(), &n);
            self.sel_actor = Some(n);
            self.right_tab = RightTab::Character;
        }
        if let Some(k) = quick {
            self.start_quick(k);
        }
        if open_studio {
            self.right_tab = RightTab::Audio;
        }
        if new_char {
            self.right_tab = RightTab::Character;
            self.status = "Type a name and press '+ New character' in the builder".into();
        }
    }
}
