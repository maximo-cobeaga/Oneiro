//! The start screen (new project, open project, recent projects, examples) and the small "new set" /
//! "new episode" dialogs, so nothing needs a terminal.

use crate::i18n::tr;
use crate::app::{App, RightTab};
use crate::theme;
use eframe::egui::{self, RichText};
use oneiro_core::scaffold::{self, Template};
use std::path::{Path, PathBuf};

const MAX_RECENT: usize = 8;

// ------------------------------------------------------------------ remembered projects

pub fn config_dir() -> Option<PathBuf> {
    if let Ok(d) = std::env::var("ONEIRO_CONFIG_DIR") {
        return Some(PathBuf::from(d));
    }
    let base = std::env::var("APPDATA").ok().or_else(|| std::env::var("XDG_CONFIG_HOME").ok()).or_else(|| std::env::var("HOME").ok().map(|h| format!("{}/.config", h)))?;
    Some(PathBuf::from(base).join("Oneiro"))
}

pub fn load_recent() -> Vec<PathBuf> {
    let Some(dir) = config_dir() else { return vec![] };
    let text = std::fs::read_to_string(dir.join("recent.txt")).unwrap_or_default();
    text.lines().map(PathBuf::from).filter(|p| p.join("oneiro.toml").exists()).take(MAX_RECENT).collect()
}

/// Puts `path` first in the remembered list (and saves it).
pub fn remember(list: &mut Vec<PathBuf>, path: &Path) {
    let path = &std::fs::canonicalize(path).map(|p| PathBuf::from(p.display().to_string().trim_start_matches(r"\\?\"))).unwrap_or_else(|_| path.to_path_buf());
    list.retain(|p| p != path);
    list.insert(0, path.to_path_buf());
    list.truncate(MAX_RECENT);
    if let Some(dir) = config_dir() {
        let _ = std::fs::create_dir_all(&dir);
        let text = list.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join("\n");
        let _ = std::fs::write(dir.join("recent.txt"), text);
    }
}

/// Where new projects go by default: Documents/Oneiro.
pub fn default_parent() -> PathBuf {
    let home = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).unwrap_or_else(|_| ".".into());
    PathBuf::from(home).join("Documents").join("Oneiro")
}

/// Example projects that ship next to the program (or in the source tree when developing).
pub fn find_examples() -> Vec<PathBuf> {
    let mut roots = vec![];
    if let Ok(exe) = std::env::current_exe() {
        for anc in exe.ancestors().skip(1).take(4) {
            roots.push(anc.join("examples"));
        }
    }
    roots.push(PathBuf::from("examples"));
    for r in roots {
        if let Ok(rd) = std::fs::read_dir(&r) {
            let mut out: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.join("oneiro.toml").exists()).collect();
            if !out.is_empty() {
                out.sort();
                return out;
            }
        }
    }
    vec![]
}

// ------------------------------------------------------------------ dialogs

pub struct NewProject {
    pub name: String,
    pub parent: PathBuf,
    pub template: Template,
    pub error: Option<String>,
}

impl NewProject {
    pub fn new() -> Self {
        NewProject { name: "My Series".into(), parent: default_parent(), template: Template::Starter, error: None }
    }
    pub fn folder(&self) -> PathBuf {
        self.parent.join(scaffold::folder_name(&self.name))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum QuickKind {
    Set,
    Episode,
}

pub struct Quick {
    pub kind: QuickKind,
    pub name: String,
    /// 0 bare room, 1 diner, 2 open field, 3 village.
    pub template: usize,
    pub set: String,
    pub cast: Vec<(String, bool)>,
    pub error: Option<String>,
}

impl App {
    pub fn remember_project(&mut self, path: &Path) {
        remember(&mut self.recent, path);
    }

    // -------------------------------------------------------------- start screen

    pub fn ui_welcome(&mut self, ui: &mut egui::Ui) {
        let mut open: Option<PathBuf> = None;
        let mut want_new = false;
        let mut want_open = false;
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.08);
            if self.logo.is_none() {
                let pm = crate::logo::render(160);
                let img = egui::ColorImage::from_rgba_premultiplied([160, 160], pm.data());
                self.logo = Some(ui.ctx().load_texture("logo", img, egui::TextureOptions::LINEAR));
            }
            if let Some(t) = &self.logo {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(t.id(), egui::vec2(110.0, 110.0))));
            }
            ui.label(RichText::new(tr("Oneiro Studio")).size(38.0).strong().color(theme::ACCENT));
            ui.label(RichText::new(tr("write, direct and render animated series")).size(15.0).weak());
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.add_space((ui.available_width() - 360.0).max(0.0) / 2.0);
                if ui.add_sized([170.0, 40.0], egui::Button::new(RichText::new(tr("New project...")).size(16.0))).clicked() {
                    want_new = true;
                }
                if ui.add_sized([170.0, 40.0], egui::Button::new(RichText::new(tr("Open project...")).size(16.0))).clicked() {
                    want_open = true;
                }
            });
            ui.add_space(18.0);
            let recent = self.recent.clone();
            let examples = find_examples();
            ui.allocate_ui(egui::vec2(520.0, 0.0), |ui| {
                if !recent.is_empty() {
                    ui.label(RichText::new(tr("Recent")).strong());
                    for p in &recent {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.link(format!("{}    {}", name, p.display())).clicked() {
                            open = Some(p.clone());
                        }
                    }
                    ui.add_space(12.0);
                }
                if !examples.is_empty() {
                    ui.label(RichText::new(tr("Examples (changes are saved in place, copy first if you want to keep them)")).strong());
                    for p in &examples {
                        let name = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                        if ui.link(name).clicked() {
                            open = Some(p.clone());
                        }
                    }
                }
            });
            if let Some(e) = &self.error {
                ui.add_space(14.0);
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), e);
            }
        });
        if want_new {
            self.new_project = Some(NewProject::new());
        }
        if want_open {
            self.pick_and_open();
        }
        if let Some(p) = open {
            self.open_project(&p);
        }
    }

    pub fn pick_and_open(&mut self) {
        if let Some(p) = rfd::FileDialog::new().set_title("Open an Oneiro project folder").pick_folder() {
            self.open_project(&p);
        }
    }

    pub fn new_project_window(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.new_project.take() else { return };
        let mut keep = true;
        let mut create = false;
        egui::Window::new(tr("New project")).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Title"));
                ui.add(egui::TextEdit::singleline(&mut d.name).desired_width(300.0));
            });
            ui.horizontal(|ui| {
                ui.label(tr("Folder"));
                ui.label(RichText::new(d.parent.display().to_string()).weak());
                if ui.button(tr("Browse...")).clicked() {
                    let _ = std::fs::create_dir_all(&d.parent);
                    if let Some(p) = rfd::FileDialog::new().set_title("Where should the project folder go?").set_directory(&d.parent).pick_folder() {
                        d.parent = p;
                    }
                }
            });
            ui.add_space(4.0);
            for t in Template::ALL {
                ui.radio_value(&mut d.template, t, t.label());
            }
            ui.add_space(4.0);
            let folder = d.folder();
            ui.label(RichText::new(format!("Will create {}", folder.display())).weak().small());
            if let Some(e) = &d.error {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), e);
            }
            ui.horizontal(|ui| {
                let ok = !scaffold::folder_name(&d.name).is_empty();
                if ui.add_enabled(ok, egui::Button::new(tr("Create"))).clicked() {
                    create = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    keep = false;
                }
            });
        });
        if create {
            let folder = d.folder();
            match scaffold::create_project(&folder, d.name.trim(), d.template) {
                Ok(_) => {
                    self.open_project(&folder);
                    return;
                }
                Err(e) => d.error = Some(e),
            }
        }
        if keep {
            self.new_project = Some(d);
        }
    }

    // -------------------------------------------------------------- new set / new episode

    pub fn start_quick(&mut self, kind: QuickKind) {
        let Some(ws) = &self.ws else { return };
        let sets: Vec<String> = ws.project.sets.keys().cloned().collect();
        let cast: Vec<(String, bool)> = ws.project.characters.keys().map(|k| (k.clone(), true)).collect();
        let n = match kind {
            QuickKind::Set => ws.project.sets.len() + 1,
            QuickKind::Episode => ws.scripts().len() + 1,
        };
        self.quick = Some(Quick {
            kind,
            name: match kind {
                QuickKind::Set => format!("set_{}", n),
                QuickKind::Episode => format!("episode_{}", n),
            },
            template: 0,
            set: sets.first().cloned().unwrap_or_default(),
            cast,
            error: None,
        });
    }

    pub fn quick_window(&mut self, ctx: &egui::Context) {
        let Some(mut q) = self.quick.take() else { return };
        let sets: Vec<String> = self.ws.as_ref().map(|w| w.project.sets.keys().cloned().collect()).unwrap_or_default();
        let (mut keep, mut create) = (true, false);
        let title = if q.kind == QuickKind::Set { "New set" } else { "New episode" };
        egui::Window::new(title).collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr("Name"));
                ui.add(egui::TextEdit::singleline(&mut q.name).desired_width(220.0));
            });
            ui.label(RichText::new(tr("letters, digits and _ (it is also the file name)")).weak().small());
            match q.kind {
                QuickKind::Set => {
                    for (i, label) in [tr("Bare room (8 x 6 m, a window, a door and a table)"), tr("Diner (counter, stools, clock, pie case)"), tr("Open field (sky and grass, no walls, build your own 3D world)"), tr("Village (houses, trees, a well and hills in 3D)")].into_iter().enumerate() {
                        ui.radio_value(&mut q.template, i, label);
                    }
                }
                QuickKind::Episode => {
                    egui::ComboBox::from_label(tr("Set")).selected_text(q.set.clone()).show_ui(ui, |ui| {
                        for s in &sets {
                            ui.selectable_value(&mut q.set, s.clone(), s);
                        }
                    });
                    ui.label(tr("Characters in the cast:"));
                    for (n, on) in q.cast.iter_mut() {
                        ui.checkbox(on, n.as_str());
                    }
                }
            }
            if let Some(e) = &q.error {
                ui.colored_label(egui::Color32::from_rgb(255, 120, 120), e);
            }
            ui.horizontal(|ui| {
                if ui.button(tr("Create")).clicked() {
                    create = true;
                }
                if ui.button(tr("Cancel")).clicked() {
                    keep = false;
                }
            });
        });
        if create {
            match self.create_quick(&q) {
                Ok(()) => return,
                Err(e) => q.error = Some(e),
            }
        }
        if keep {
            self.quick = Some(q);
        }
    }

    fn create_quick(&mut self, q: &Quick) -> Result<(), String> {
        if !scaffold::valid_name(&q.name) {
            return Err("use letters, digits and _, starting with a letter".into());
        }
        let ws = self.ws.as_mut().ok_or("no project open")?;
        match q.kind {
            QuickKind::Set => {
                let path = ws.root.join("library").join("sets").join(format!("{}.onset", q.name));
                if path.exists() || ws.project.sets.contains_key(&q.name) {
                    return Err(format!("a set called '{}' already exists", q.name));
                }
                let text = match q.template {
                    1 => scaffold::diner_source(&q.name),
                    2 => scaffold::field_source(&q.name),
                    3 => scaffold::village_source(&q.name),
                    _ => scaffold::room_source(&q.name),
                };
                std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
                std::fs::write(&path, text).map_err(|e| e.to_string())?;
                ws.reload_project();
                let i = ws.open_doc(&path)?;
                ws.active = i;
                self.status = format!("Created set {} (library/sets/{}.onset)", q.name, q.name);
            }
            QuickKind::Episode => {
                if q.set.is_empty() {
                    return Err("the project has no set yet: create one first".into());
                }
                let dir = ws.root.join("episodes").join(&q.name);
                let path = dir.join("script.onr");
                if path.exists() {
                    return Err(format!("an episode called '{}' already exists", q.name));
                }
                let cast: Vec<String> = q.cast.iter().filter(|(_, on)| *on).map(|(n, _)| n.clone()).collect();
                let text = scaffold::episode_source(&q.name.replace('_', " "), &ws.project.manifest.name, &cast, &q.set);
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                std::fs::write(&path, text).map_err(|e| e.to_string())?;
                let i = ws.open_doc(&path)?;
                ws.active = i;
                ws.set_script(i);
                self.t = 0.0;
                self.vp.invalidate();
                self.status = format!("Created episode {} (episodes/{}/script.onr)", q.name, q.name);
            }
        }
        self.right_tab = RightTab::Script;
        Ok(())
    }
}

pub static TEST_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_projects_are_remembered_most_recent_first_without_duplicates() {
        let _g = TEST_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let dir = std::env::temp_dir().join(format!("oneiro_recent_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mk = |n: &str| {
            let p = dir.join(n);
            scaffold::create_project(&p, n, Template::Empty).unwrap();
            p
        };
        let (a, b) = (mk("a"), mk("b"));
        std::env::set_var("ONEIRO_CONFIG_DIR", dir.join("cfg"));
        let mut list = load_recent();
        remember(&mut list, &a);
        remember(&mut list, &b);
        remember(&mut list, &a);
        assert_eq!(list.len(), 2);
        let again = load_recent();
        assert_eq!(again.len(), 2);
        assert!(again[0].ends_with("a") && again[1].ends_with("b"));
        std::fs::remove_dir_all(&b).unwrap();
        assert_eq!(load_recent().len(), 1, "deleted projects drop out of the list");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
