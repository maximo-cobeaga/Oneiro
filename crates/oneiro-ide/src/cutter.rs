//! The Cutter tab (hybrid cutter, tier C): open an illustration of a character, let Oneiro (or an AI
//! that can see) propose where the head, body, arm and leg are, drag the boxes to fix them, watch the
//! validator and the assembled puppet update, and create a normal character.

use crate::app::{App, RightTab};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke, TextureOptions};
use oneiro_ai::cutter_ai::{propose_with_ai, CutProposal};
use oneiro_ai::DirectorOpts;
use oneiro_core::raster::Raster;
use oneiro_cutter::{cut, extract_mask, overlay::COLORS, propose, validate, write_character, Image, Issue, Layout, Mask, RectN, WriteOpts, PARTS};
use oneiro_render::preview::{character_portrait, PortraitOpts};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Instant;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Drag {
    Move,
    Corner(bool, bool),
}

pub struct CutterState {
    pub path: Option<PathBuf>,
    img: Option<Arc<Image>>,
    mask: Option<Arc<Mask>>,
    pub layout: Option<Layout>,
    issues: Vec<Issue>,
    tex: Option<egui::TextureHandle>,
    preview: Option<egui::TextureHandle>,
    /// Layout the preview was made from (re-rendered when it changes and the mouse is released).
    preview_of: Option<String>,
    pub selected: usize,
    drag: Option<(usize, Drag)>,
    pub name: String,
    pub height_m: f64,
    pub style: String,
    pub msg: String,
    pub tolerance: f32,
    job: Option<(Receiver<Result<CutProposal, String>>, Instant)>,
}

impl Default for CutterState {
    fn default() -> Self {
        CutterState {
            path: None,
            img: None,
            mask: None,
            layout: None,
            issues: vec![],
            tex: None,
            preview: None,
            preview_of: None,
            selected: 0,
            drag: None,
            name: String::new(),
            height_m: 1.15,
            style: String::new(),
            msg: String::new(),
            tolerance: 0.0,
            job: None,
        }
    }
}

fn to_color_image(pm: &tiny_skia::Pixmap) -> egui::ColorImage {
    egui::ColorImage::from_rgba_premultiplied([pm.width() as usize, pm.height() as usize], pm.data())
}

impl CutterState {
    fn center_x(&self) -> Option<f32> {
        self.mask.as_ref().map(|m| m.axis())
    }

    fn revalidate(&mut self) {
        self.issues = match (&self.layout, &self.mask) {
            (Some(l), Some(m)) => validate(l, m),
            _ => vec![],
        };
    }

    fn errors(&self) -> usize {
        self.issues.iter().filter(|i| i.is_error()).count()
    }

    /// Loads an illustration and proposes a layout.
    pub fn open(&mut self, ctx: &egui::Context, path: PathBuf) {
        self.msg.clear();
        let loaded = Image::load(&path).and_then(|img| {
            let mask = extract_mask(&img, if self.tolerance > 0.0 { Some(self.tolerance) } else { None })?;
            Ok((img, mask))
        });
        match loaded {
            Err(e) => {
                self.msg = e;
                self.img = None;
                self.mask = None;
                self.layout = None;
                self.issues.clear();
            }
            Ok((img, mask)) => {
                self.tex = Some(ctx.load_texture("cutter_image", to_color_image(&img.pm), TextureOptions::LINEAR));
                self.layout = propose(&mask).ok();
                if self.name.is_empty() {
                    let stem = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let clean: String = stem.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
                    let mut cs = clean.chars();
                    self.name = cs.next().map(|f| f.to_ascii_uppercase().to_string() + cs.as_str()).unwrap_or_default();
                }
                self.img = Some(Arc::new(img));
                self.mask = Some(Arc::new(mask));
                self.path = Some(path);
                self.preview_of = None;
                self.revalidate();
            }
        }
    }
}

fn box_in(rect: Rect, r: RectN) -> Rect {
    Rect::from_min_max(Pos2::new(rect.left() + r.x0 * rect.width(), rect.top() + r.y0 * rect.height()), Pos2::new(rect.left() + r.x1 * rect.width(), rect.top() + r.y1 * rect.height()))
}

impl App {
    pub fn ui_cutter(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.ui_cutter_inner(ui));
    }

    fn ui_cutter_inner(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        if self.ws.is_none() {
            ui.label("No project open.");
            return;
        }
        // Result of a running AI proposal.
        let mut done: Option<Result<CutProposal, String>> = None;
        if let Some((rx, _)) = &self.cutter.job {
            if let Ok(r) = rx.try_recv() {
                done = Some(r);
            }
        }
        if let Some(r) = done {
            self.cutter.job = None;
            match r {
                Ok(p) => {
                    self.cutter.msg = format!("The model answered in {} round(s){}.", p.rounds, if p.valid { "" } else { " but its boxes still have problems: fix them by dragging" });
                    self.cutter.layout = Some(p.layout);
                    self.cutter.preview_of = None;
                    self.cutter.revalidate();
                }
                Err(e) => self.cutter.msg = e,
            }
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button("Open illustration...").clicked() {
                if let Some(p) = rfd::FileDialog::new().set_title("Illustration of one character").add_filter("Images", &["png", "jpg", "jpeg", "webp", "bmp", "gif"]).pick_file() {
                    self.cutter.open(&ctx, p);
                }
            }
            let have = self.cutter.mask.is_some();
            if ui.add_enabled(have, egui::Button::new("Propose automatically")).on_hover_text("Finds the neck, hips and torso from the silhouette").clicked() {
                if let Some(m) = self.cutter.mask.clone() {
                    match propose(&m) {
                        Ok(l) => {
                            self.cutter.layout = Some(l);
                            self.cutter.preview_of = None;
                            self.cutter.revalidate();
                        }
                        Err(e) => self.cutter.msg = e,
                    }
                }
            }
            let busy = self.cutter.job.is_some();
            if ui.add_enabled(have && !busy, egui::Button::new("Propose with AI")).on_hover_text("A vision model looks at the picture; the engine checks its boxes and sends it back to fix them. Uses the provider of the AI tab.").clicked() {
                self.cutter_ai_propose(&ctx);
            }
        });
        if let Some((_, t)) = &self.cutter.job {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("the AI is looking at the picture ({:.0}s)", t.elapsed().as_secs_f32()));
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(200));
        }
        if !self.cutter.msg.is_empty() {
            ui.label(RichText::new(self.cutter.msg.clone()).small().color(Color32::from_rgb(230, 200, 120)));
        }
        let (Some(img), Some(_)) = (self.cutter.img.clone(), self.cutter.layout.as_ref()) else {
            ui.add_space(8.0);
            ui.label("Cut a character out of an illustration:");
            ui.label(RichText::new("1. Open a picture of ONE character seen from the front, standing, arms at the sides, on a plain background (or a PNG with transparency).\n2. Oneiro proposes where the head, body, arm and leg are. Drag the boxes if needed: the arm and the leg are the ones on the LEFT of the picture; the right ones are mirrored.\n3. Fix what the checker reports, name it and create the character: it is animated, lip-synced to your audio only if its face is drawn by Oneiro, and works in any style.").weak().small());
            return;
        };

        // ------------------------------------------------------------ image with boxes
        ui.horizontal(|ui| {
            for (i, p) in PARTS.iter().enumerate() {
                let c = COLORS[i];
                let label = RichText::new(format!("{} {}", i + 1, p)).color(Color32::from_rgb(c.0, c.1, c.2)).strong();
                ui.selectable_value(&mut self.cutter.selected, i, label);
            }
        });
        let avail = ui.available_width().min(520.0);
        let aspect = img.height() as f32 / img.width() as f32;
        let size = egui::vec2(avail, (avail * aspect).min(560.0));
        let size = if size.y >= 560.0 { egui::vec2(560.0 / aspect, 560.0) } else { size };
        let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
        let rect = resp.rect;
        if let Some(tex) = &self.cutter.tex {
            painter.image(tex.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
        let cx = self.cutter.center_x();
        let layout = self.cutter.layout.clone().unwrap();
        for (i, (_, r)) in layout.rects().iter().enumerate() {
            let c = COLORS[i];
            let col = Color32::from_rgb(c.0, c.1, c.2);
            let sel = i == self.cutter.selected;
            painter.rect_stroke(box_in(rect, *r), 0.0, Stroke::new(if sel { 2.5 } else { 1.5 }, col), egui::StrokeKind::Inside);
            if sel {
                painter.rect_filled(box_in(rect, *r), 0.0, Color32::from_rgba_unmultiplied(c.0, c.1, c.2, 28));
            }
            if let (Some(cx), true) = (cx, i >= 2) {
                painter.rect_stroke(box_in(rect, r.mirrored(cx)), 0.0, Stroke::new(1.0, Color32::from_rgba_unmultiplied(c.0, c.1, c.2, 120)), egui::StrokeKind::Inside);
            }
            let b = box_in(rect, *r);
            painter.text(b.left_top() + egui::vec2(3.0, 1.0), egui::Align2::LEFT_TOP, format!("{}", i + 1), egui::FontId::monospace(13.0), col);
            if sel {
                for (hx, hy) in [(b.left(), b.top()), (b.right(), b.top()), (b.left(), b.bottom()), (b.right(), b.bottom())] {
                    painter.rect_filled(Rect::from_center_size(Pos2::new(hx, hy), egui::vec2(8.0, 8.0)), 1.0, col);
                }
            }
        }
        // Dragging: corners resize, the inside moves; clicking inside another box selects it.
        let to_n = |p: Pos2| ((p.x - rect.left()) / rect.width(), (p.y - rect.top()) / rect.height());
        let mut changed = false;
        if resp.drag_started() {
            if let Some(p) = resp.interact_pointer_pos() {
                let sel = self.cutter.selected;
                let b = box_in(rect, layout.rects()[sel].1);
                let near = |a: f32, b: f32| (a - b).abs() < 10.0;
                let kind = if near(p.x, b.left()) && near(p.y, b.top()) {
                    Some(Drag::Corner(false, false))
                } else if near(p.x, b.right()) && near(p.y, b.top()) {
                    Some(Drag::Corner(true, false))
                } else if near(p.x, b.left()) && near(p.y, b.bottom()) {
                    Some(Drag::Corner(false, true))
                } else if near(p.x, b.right()) && near(p.y, b.bottom()) {
                    Some(Drag::Corner(true, true))
                } else if b.contains(p) {
                    Some(Drag::Move)
                } else {
                    None
                };
                self.cutter.drag = kind.map(|k| (sel, k));
            }
        }
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                let mut best: Option<(usize, f32)> = None;
                for (i, (_, r)) in layout.rects().iter().enumerate() {
                    let b = box_in(rect, *r);
                    if b.contains(p) && best.map_or(true, |(_, a)| b.area() < a) {
                        best = Some((i, b.area()));
                    }
                }
                if let Some((i, _)) = best {
                    self.cutter.selected = i;
                }
            }
        }
        if resp.dragged() {
            if let (Some((part, kind)), Some(p)) = (self.cutter.drag, resp.interact_pointer_pos()) {
                let d = resp.drag_delta();
                let (dx, dy) = (d.x / rect.width(), d.y / rect.height());
                let (nx, ny) = to_n(p);
                if let Some(l) = self.cutter.layout.as_mut() {
                    if let Some(r) = l.get_mut(PARTS[part]) {
                        drag_rect(r, kind, (dx, dy), (nx, ny));
                        changed = true;
                    }
                }
            }
        }
        if resp.drag_stopped() {
            self.cutter.drag = None;
            self.cutter.preview_of = None;
        }
        if changed {
            self.cutter.revalidate();
        }

        // ------------------------------------------------------------ validator
        ui.add_space(4.0);
        if self.cutter.issues.is_empty() {
            ui.label(RichText::new("The layout looks good.").color(Color32::from_rgb(120, 220, 150)));
        }
        for i in &self.cutter.issues {
            let col = if i.is_error() { Color32::from_rgb(240, 120, 110) } else { Color32::from_rgb(240, 200, 90) };
            ui.label(RichText::new(i.render()).small().color(col));
        }

        // ------------------------------------------------------------ preview of the assembled puppet
        self.cutter_preview(&ctx);
        if let Some(tex) = &self.cutter.preview {
            ui.horizontal(|ui| {
                ui.add(egui::Image::new(egui::load::SizedTexture::new(tex.id(), egui::vec2(150.0, 190.0))));
                ui.label(RichText::new("The puppet assembled from the pieces (it turns, walks and gestures like any character).").weak().small());
            });
        }

        // ------------------------------------------------------------ create
        ui.separator();
        let mut create = false;
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut self.cutter.name).hint_text("Capitalized").desired_width(110.0));
            ui.label("height");
            ui.add(egui::DragValue::new(&mut self.cutter.height_m).range(0.3..=2.5).speed(0.01).suffix(" m"));
            egui::ComboBox::from_id_salt("cut_style").selected_text(if self.cutter.style.is_empty() { "(default style)".to_string() } else { self.cutter.style.clone() }).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.cutter.style, String::new(), "(default style)");
                for s in oneiro_core::catalog::STYLES {
                    ui.selectable_value(&mut self.cutter.style, s.to_string(), *s);
                }
            });
        });
        let name_ok = oneiro_cutter::valid_name(&self.cutter.name);
        let can = self.cutter.errors() == 0 && name_ok;
        ui.horizontal(|ui| {
            if ui.add_enabled(can, egui::Button::new(RichText::new("Create character").strong())).clicked() {
                create = true;
            }
            if !name_ok {
                ui.label(RichText::new("the name must start with a capital letter (letters, digits, _)").weak().small());
            } else if self.cutter.errors() > 0 {
                ui.label(RichText::new("fix the errors above first").weak().small());
            }
        });
        if create {
            self.cutter_create();
        }
    }

    fn cutter_ai_propose(&mut self, ctx: &egui::Context) {
        let (Some(img), Some(mask), Some(start)) = (self.cutter.img.clone(), self.cutter.mask.clone(), self.cutter.layout.clone()) else { return };
        let cfg = self.ai.cfg.clone();
        let key = self.ai.session_key.clone();
        let (tx, rx) = channel();
        self.cutter.job = Some((rx, Instant::now()));
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let r = cfg.build_provider(Some(&key)).and_then(|p| {
                let opts = DirectorOpts { max_rounds: cfg.max_rounds, ..Default::default() };
                propose_with_ai(p.as_ref(), &img, &mask, Some(&start), &opts, &mut |_| {})
            });
            let _ = tx.send(r);
            c2.request_repaint();
        });
    }

    /// Cuts the pieces and renders the assembled puppet (only when the layout changed).
    fn cutter_preview(&mut self, ctx: &egui::Context) {
        let key = self.cutter.layout.as_ref().map(|l| l.to_json());
        if self.cutter.preview_of == key || self.cutter.drag.is_some() {
            return;
        }
        self.cutter.preview_of = key;
        self.cutter.preview = None;
        if self.cutter.errors() > 0 {
            return;
        }
        let (Some(img), Some(mask), Some(layout)) = (self.cutter.img.clone(), self.cutter.mask.clone(), self.cutter.layout.clone()) else { return };
        let Ok(c) = cut(&img, &mask, &layout) else { return };
        let Ok(mut def) = oneiro_cutter::character_of("Preview", &c, &WriteOpts { height_m: self.cutter.height_m, style: self.cutter.style.clone(), source: String::new() }) else { return };
        if let Some(rig) = def.svg.as_mut() {
            for p in &c.pieces {
                if let Some(part) = rig.parts.get_mut(p.name) {
                    part.raster = Some(Arc::new(Raster { pm: p.pm.clone() }));
                }
            }
        }
        let pm = character_portrait(&def, &PortraitOpts::default(), 150 * 2, 190 * 2);
        self.cutter.preview = Some(ctx.load_texture("cutter_preview", to_color_image(&pm), TextureOptions::LINEAR));
    }

    fn cutter_create(&mut self) {
        let (Some(img), Some(mask), Some(layout)) = (self.cutter.img.clone(), self.cutter.mask.clone(), self.cutter.layout.clone()) else { return };
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else { return };
        let name = self.cutter.name.clone();
        let opts = WriteOpts {
            height_m: self.cutter.height_m,
            style: self.cutter.style.clone(),
            source: self.cutter.path.as_ref().and_then(|p| p.file_name()).map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
        };
        match cut(&img, &mask, &layout).and_then(|c| write_character(&root, &name, &c, &opts)) {
            Ok(path) => {
                if let Some(ws) = &mut self.ws {
                    ws.reload_project();
                    ws.recompile();
                }
                self.thumbs.clear();
                self.vp.invalidate();
                self.builder.select(self.ws.as_ref(), &name);
                self.right_tab = RightTab::Character;
                self.status = format!("Created {} from the illustration. Use it with: use character {}", path.display(), name);
            }
            Err(e) => self.cutter.msg = e,
        }
    }
}

/// Moves or resizes a box. `delta` is the pointer movement and `pos` the pointer position, both normalized.
fn drag_rect(r: &mut RectN, kind: Drag, delta: (f32, f32), pos: (f32, f32)) {
    const MIN: f32 = 0.03;
    match kind {
        Drag::Move => {
            let (w, h) = (r.w(), r.h());
            r.x0 = (r.x0 + delta.0).clamp(0.0, 1.0 - w);
            r.y0 = (r.y0 + delta.1).clamp(0.0, 1.0 - h);
            r.x1 = r.x0 + w;
            r.y1 = r.y0 + h;
        }
        Drag::Corner(right, bottom) => {
            if right {
                r.x1 = pos.0.clamp(r.x0 + MIN, 1.0);
            } else {
                r.x0 = pos.0.clamp(0.0, r.x1 - MIN);
            }
            if bottom {
                r.y1 = pos.1.clamp(r.y0 + MIN, 1.0);
            } else {
                r.y0 = pos.1.clamp(0.0, r.y1 - MIN);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moving_a_box_keeps_its_size_and_stays_inside_the_picture() {
        let mut r = RectN::new(0.2, 0.2, 0.4, 0.5);
        drag_rect(&mut r, Drag::Move, (0.1, -0.05), (0.0, 0.0));
        assert!((r.x0 - 0.3).abs() < 1e-6 && (r.y0 - 0.15).abs() < 1e-6 && (r.w() - 0.2).abs() < 1e-6 && (r.h() - 0.3).abs() < 1e-6);
        drag_rect(&mut r, Drag::Move, (5.0, 5.0), (0.0, 0.0));
        assert!((r.x1 - 1.0).abs() < 1e-6 && (r.y1 - 1.0).abs() < 1e-6 && (r.w() - 0.2).abs() < 1e-6);
        drag_rect(&mut r, Drag::Move, (-5.0, -5.0), (0.0, 0.0));
        assert!(r.x0 == 0.0 && r.y0 == 0.0);
    }

    #[test]
    fn corners_resize_and_never_invert_or_vanish() {
        let mut r = RectN::new(0.2, 0.2, 0.6, 0.6);
        drag_rect(&mut r, Drag::Corner(true, true), (0.0, 0.0), (0.8, 0.9));
        assert_eq!((r.x1, r.y1), (0.8, 0.9));
        drag_rect(&mut r, Drag::Corner(false, false), (0.0, 0.0), (0.1, 0.05));
        assert_eq!((r.x0, r.y0), (0.1, 0.05));
        // Dragging past the opposite side leaves the minimum size, not a negative box.
        drag_rect(&mut r, Drag::Corner(true, true), (0.0, 0.0), (0.0, 0.0));
        assert!(r.w() >= 0.03 - 1e-6 && r.h() >= 0.03 - 1e-6);
        drag_rect(&mut r, Drag::Corner(false, false), (0.0, 0.0), (2.0, 2.0));
        assert!(r.x0 < r.x1 && r.y0 < r.y1);
    }
}
