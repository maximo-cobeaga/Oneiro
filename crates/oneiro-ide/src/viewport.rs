//! The 3D viewport: renders the current frame with the real renderer (what you see is what the
//! export produces), plus a free orbit camera, actor picking/dragging and motion recording.

use crate::i18n::tr;
use crate::app::{App, Take};
use crate::record::{emit, step_puppet, Puppet, Recorder, Sample};
use crate::textops::statement_end_line;
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Shape, Stroke, TextureOptions};
use oneiro_core::math::*;
use oneiro_core::CameraPose;
use oneiro_render::draw::Cam;
use oneiro_render::Renderer;

#[derive(Clone, Copy, Debug)]
pub struct FreeCam {
    pub yaw: f64,
    pub pitch: f64,
    pub dist: f64,
    pub target: V3,
    pub fov: f64,
}

impl FreeCam {
    pub fn pose(&self) -> CameraPose {
        let d = [
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            -self.yaw.cos() * self.pitch.cos(),
        ];
        CameraPose {
            pos: [self.target[0] + d[0] * self.dist, self.target[1] + d[1] * self.dist, self.target[2] + d[2] * self.dist],
            target: self.target,
            fov: self.fov,
            roll: 0.0,
        }
    }

    pub fn from_pose(p: &CameraPose) -> FreeCam {
        let v = sub3(p.pos, p.target);
        let dist = len3(v).max(0.3);
        let pitch = (v[1] / dist).clamp(-1.0, 1.0).asin();
        let yaw = v[0].atan2(-v[2]);
        FreeCam { yaw, pitch, dist, target: p.target, fov: p.fov }
    }

    pub fn overview(set_depth: f64) -> FreeCam {
        FreeCam { yaw: 0.0, pitch: 0.95, dist: (set_depth * 1.9).max(7.0), target: [0.0, 0.6, 0.0], fov: 42.0 }
    }
}

pub struct DragState {
    pub name: String,
    pub pos: V2,
}

pub struct Viewport {
    pub free: bool,
    pub cam: FreeCam,
    pub show_labels: bool,
    pub tex: Option<egui::TextureHandle>,
    pub key: u64,
    pub drag: Option<DragState>,
}

impl Default for Viewport {
    fn default() -> Self {
        Viewport { free: false, cam: FreeCam::overview(6.0), show_labels: true, tex: None, key: 0, drag: None }
    }
}

impl Viewport {
    pub fn invalidate(&mut self) {
        self.key = 0;
    }
}

fn mix_key(h: &mut u64, v: u64) {
    *h ^= v.wrapping_add(0x9e3779b97f4a7c15).wrapping_add(*h << 6).wrapping_add(*h >> 2);
}

impl App {
    pub fn ui_viewport(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(ep) = self.ws.as_ref().map(|w| w.episode.clone()) else {
            ui.vertical_centered(|ui| {
                ui.add_space(ui.available_height() * 0.35);
                ui.heading(tr("Oneiro Studio"));
                ui.label(tr("Open a project folder to start."));
                if ui.button(tr("Open project...")).clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_title("Open an Oneiro project folder").pick_folder() {
                        self.open_project(&p);
                    }
                }
                if let Some(e) = &self.error {
                    ui.colored_label(Color32::from_rgb(240, 120, 110), e);
                }
            });
            return;
        };
        let valid = self.ws.as_ref().map_or(true, |w| w.valid);

        // ---------------------------------------------------------------- toolbar
        let actor_names: Vec<String> = ep.locate(self.t).map(|(si, _)| ep.scenes[si].actors.iter().map(|a| a.name.clone()).collect()).unwrap_or_default();
        let mut start_rec = false;
        let mut stop_rec = false;
        let mut cancel_rec = false;
        ui.horizontal(|ui| {
            let was_free = self.vp.free;
            ui.selectable_value(&mut self.vp.free, false, tr("Shot camera"));
            ui.selectable_value(&mut self.vp.free, true, tr("Free camera"));
            if self.vp.free && !was_free {
                if let Some((si, lt)) = ep.locate(self.t) {
                    self.vp.cam = FreeCam::from_pose(&ep.scenes[si].camera_at(lt));
                }
            }
            ui.checkbox(&mut self.vp.show_labels, tr("Labels"));
            ui.separator();
            if self.recorder.is_none() {
                let mut sel = self.sel_actor.clone().filter(|s| actor_names.contains(s)).or_else(|| actor_names.first().cloned());
                egui::ComboBox::from_id_salt("rec_actor").selected_text(sel.clone().unwrap_or("-".into())).width(90.0).show_ui(ui, |ui| {
                    for n in &actor_names {
                        ui.selectable_value(&mut sel, Some(n.clone()), n);
                    }
                });
                if sel != self.sel_actor && sel.is_some() {
                    self.sel_actor = sel;
                }
                if ui.add_enabled(!actor_names.is_empty() && valid, egui::Button::new(RichText::new(tr("● Record motion")).color(Color32::from_rgb(240, 100, 100)))).on_hover_text(tr("Drive the selected character with WASD / arrows and turn it into script lines")).clicked() {
                    start_rec = true;
                }
            } else {
                let rec = self.recorder.as_ref().unwrap();
                ui.label(RichText::new(format!("● REC {}  {:.1}s", rec.actor, rec.clock)).color(Color32::from_rgb(255, 90, 90)).strong());
                if ui.button(tr("Stop")).clicked() {
                    stop_rec = true;
                }
                if ui.button(tr("Cancel (Esc)")).clicked() {
                    cancel_rec = true;
                }
            }
        });
        if start_rec {
            self.start_recording();
        }
        if stop_rec {
            self.stop_recording();
        }
        if cancel_rec {
            self.recorder = None;
            self.status = "Recording cancelled".into();
        }

        // ---------------------------------------------------------------- layout
        let avail = ui.available_size_before_wrap();
        let (rect, resp) = ui.allocate_exact_size(avail, Sense::click_and_drag());
        ui.painter().rect_filled(rect, 0.0, theme::PANEL_DARK);
        let Some((si, lt)) = ep.locate(self.t) else {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "No scene yet - write a `scene` in the script", egui::FontId::proportional(16.0), Color32::GRAY);
            return;
        };
        let sc = &ep.scenes[si];
        let aspect = ep.width as f32 / ep.height as f32;
        let (mut iw, mut ih) = (rect.width(), rect.width() / aspect);
        if ih > rect.height() {
            ih = rect.height();
            iw = ih * aspect;
        }
        let img_rect = Rect::from_center_size(rect.center(), egui::vec2(iw, ih));
        let ppp = ctx.pixels_per_point();
        let rw = (((iw * ppp).round() as u32).clamp(160, 1280)) & !1;
        let rh = (((rw as f32 / aspect).round() as u32).max(90)) & !1;

        // ---------------------------------------------------------------- state for this frame
        let shot_pose = sc.camera_at(lt);
        let pose = if self.vp.free { self.vp.cam.pose() } else { shot_pose };
        let cam = Cam::new(&pose, rw, rh);
        let mut frames = sc.frames_at(lt);

        // Recording: puppeteer the character, other actors stay frozen.
        let demo_drive = self.startup.demo_record;
        if let Some(rec) = &mut self.recorder {
            let dt = ctx.input(|i| i.stable_dt as f64).min(0.05);
            let (inp, run) = ctx.input(|i| {
                let mut x = 0.0;
                let mut z = 0.0;
                if i.key_down(egui::Key::D) || i.key_down(egui::Key::ArrowRight) {
                    x += 1.0;
                }
                if i.key_down(egui::Key::A) || i.key_down(egui::Key::ArrowLeft) {
                    x -= 1.0;
                }
                if i.key_down(egui::Key::W) || i.key_down(egui::Key::ArrowUp) {
                    z += 1.0;
                }
                if i.key_down(egui::Key::S) || i.key_down(egui::Key::ArrowDown) {
                    z -= 1.0;
                }
                ([x, z], i.modifiers.shift)
            });
            let (inp, run) = if demo_drive {
                if rec.clock < 1.6 { ([0.0, 1.0], false) } else if rec.clock < 3.2 { ([1.0, 0.0], true) } else { ([0.0, 0.0], false) }
            } else {
                (inp, run)
            };
            let bounds = (sc.set.width / 2.0 - 0.3, sc.set.depth / 2.0 - 0.3);
            step_puppet(&mut rec.puppet, inp, run, [cam.fwd[0], cam.fwd[2]], [cam.right[0], cam.right[2]], dt, bounds);
            rec.clock += dt;
            if rec.clock - rec.last_sample >= 1.0 / 30.0 {
                rec.last_sample = rec.clock;
                rec.samples.push(Sample { t: rec.clock, pos: rec.puppet.pos, running: rec.puppet.running });
            }
            if let Some(fr) = frames.get_mut(rec.actor_idx) {
                fr.kin.pos = rec.puppet.pos;
                fr.kin.yaw = rec.puppet.yaw;
                fr.kin.odo = rec.puppet.odo;
                fr.kin.speed = rec.puppet.speed;
                fr.kin.running = rec.puppet.running;
                fr.kin.sit = 0.0;
            }
            ctx.request_repaint();
        }
        if let Some(d) = &self.vp.drag {
            if let Some(i) = sc.actors.iter().position(|a| a.name == d.name) {
                frames[i].kin.pos = d.pos;
            }
        }

        // ---------------------------------------------------------------- render (only when something changed)
        let mut key = 0xcbf29ce484222325u64;
        mix_key(&mut key, self.t.to_bits());
        mix_key(&mut key, self.vp.free as u64);
        for v in [pose.pos[0], pose.pos[1], pose.pos[2], pose.target[0], pose.target[1], pose.target[2], pose.fov, pose.roll] {
            mix_key(&mut key, v.to_bits());
        }
        mix_key(&mut key, self.ws.as_ref().map_or(0, |w| w.generation));
        mix_key(&mut key, ((rw as u64) << 32) | rh as u64);
        for fr in &frames {
            mix_key(&mut key, fr.kin.pos[0].to_bits() ^ fr.kin.pos[1].to_bits().rotate_left(7) ^ fr.kin.odo.to_bits().rotate_left(13));
        }
        if self.vp.tex.is_none() || key != self.vp.key {
            self.vp.key = key;
            let renderer = Renderer::new(&ep, rw, rh);
            let pm = renderer.render_scene(sc, pose, frames.clone());
            let img = egui::ColorImage::from_rgba_premultiplied([rw as usize, rh as usize], pm.data());
            match &mut self.vp.tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => self.vp.tex = Some(ctx.load_texture("viewport", img, TextureOptions::LINEAR)),
            }
        }
        let painter = ui.painter_at(rect);
        if let Some(tex) = &self.vp.tex {
            painter.image(tex.id(), img_rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }

        // ---------------------------------------------------------------- helpers: screen <-> world
        let to_ui = |sx: f32, sy: f32| -> Pos2 { Pos2::new(img_rect.min.x + sx / rw as f32 * iw, img_rect.min.y + sy / rh as f32 * ih) };
        let project = |p: V3| -> Option<Pos2> { cam.project(p).map(|(x, y, _)| to_ui(x, y)) };        let ground = |pos: Pos2| -> Option<V2> {
            let px = (pos.x - img_rect.min.x) / iw * rw as f32;
            let py = (pos.y - img_rect.min.y) / ih * rh as f32;
            ground_hit(&cam, px as f64, py as f64)
        };

        // ---------------------------------------------------------------- overlays
        if self.vp.show_labels {
            for fr in &frames {
                let a = &sc.actors[fr.idx];
                let h = a.def.height();
                let lift = fr.kin.sit * (fr.kin.seat_h + 0.03 - a.def.dims().leg);
                if let Some(p) = project([fr.kin.pos[0], h * 1.12 + lift + 0.1, fr.kin.pos[1]]) {
                    let selected = self.sel_actor.as_deref() == Some(a.name.as_str());
                    let galley = painter.layout_no_wrap(a.name.clone(), egui::FontId::proportional(12.0), Color32::WHITE);
                    let r = Rect::from_center_size(p, galley.size() + egui::vec2(10.0, 4.0));
                    let bg = if selected { theme::ACCENT } else { Color32::from_black_alpha(150) };
                    painter.rect_filled(r, 4.0, bg);
                    painter.galley(r.center() - galley.size() / 2.0, galley, Color32::WHITE);
                }
            }
        }
        if let Some(sel) = self.sel_actor.clone() {
            if let Some(fr) = frames.iter().find(|f| sc.actors[f.idx].name == sel) {
                let ring: Vec<Pos2> = (0..32)
                    .filter_map(|i| {
                        let a = i as f64 / 32.0 * std::f64::consts::TAU;
                        project([fr.kin.pos[0] + a.cos() * 0.5, 0.01, fr.kin.pos[1] + a.sin() * 0.5])
                    })
                    .collect();
                if ring.len() > 3 {
                    painter.add(Shape::closed_line(ring, Stroke::new(2.0, theme::ACCENT)));
                }
            }
        }
        // Shot label
        {
            let seg = sc.cameras.iter().rev().find(|c| c.t0 <= lt).or(sc.cameras.first());
            let mut text = format!("scene {}  |  {}", sc.number, fmt_pin(seg, self.ws.as_ref().unwrap(), sc.number));
            if self.vp.free {
                text.push_str("   [free camera]");
            }
            let g = painter.layout_no_wrap(text, egui::FontId::monospace(12.0), Color32::from_gray(225));
            let r = Rect::from_min_size(img_rect.left_bottom() + egui::vec2(8.0, -g.size().y - 12.0), g.size() + egui::vec2(10.0, 4.0));
            painter.rect_filled(r, 4.0, Color32::from_black_alpha(150));
            painter.galley(r.min + egui::vec2(5.0, 2.0), g, Color32::WHITE);
        }
        if let Some(rec) = &self.recorder {
            let text = format!("● REC {}  -  WASD / arrows to move, Shift to run  -  after: {}", rec.actor, rec.after_label);
            let g = painter.layout_no_wrap(text, egui::FontId::proportional(13.0), Color32::WHITE);
            let r = Rect::from_min_size(img_rect.left_top() + egui::vec2(8.0, 8.0), g.size() + egui::vec2(12.0, 6.0));
            painter.rect_filled(r, 4.0, Color32::from_rgba_unmultiplied(190, 30, 30, 220));
            painter.galley(r.min + egui::vec2(6.0, 3.0), g, Color32::WHITE);
        }
        if !valid {
            let g = painter.layout_no_wrap("script has errors - showing last valid version".into(), egui::FontId::proportional(12.0), Color32::WHITE);
            let r = Rect::from_min_size(img_rect.right_top() + egui::vec2(-g.size().x - 20.0, 8.0), g.size() + egui::vec2(12.0, 6.0));
            painter.rect_filled(r, 4.0, Color32::from_rgba_unmultiplied(180, 90, 30, 220));
            painter.galley(r.min + egui::vec2(6.0, 3.0), g, Color32::WHITE);
        }

        // ---------------------------------------------------------------- interaction
        if self.recorder.is_some() {
            return;
        }
        // Free camera navigation
        if self.vp.free {
            let c = &mut self.vp.cam;
            if resp.dragged_by(egui::PointerButton::Secondary) {
                let d = resp.drag_delta();
                c.yaw -= d.x as f64 * 0.008;
                c.pitch = (c.pitch + d.y as f64 * 0.008).clamp(-0.2, 1.5);
            }
            if resp.dragged_by(egui::PointerButton::Middle) {
                let d = resp.drag_delta();
                let k = c.dist * 0.0015;
                let right = [cam.right[0], 0.0, cam.right[2]];
                let fwd = norm3([cam.fwd[0], 0.0, cam.fwd[2]]);
                c.target = add3(c.target, add3(mul3(right, -d.x as f64 * k), mul3(fwd, d.y as f64 * k)));
            }
            if resp.hovered() {
                let scroll = ctx.input(|i| i.smooth_scroll_delta.y) as f64;
                if scroll != 0.0 {
                    c.dist = (c.dist * (-scroll * 0.0015).exp()).clamp(0.8, 40.0);
                }
            }
        }
        // Picking & dragging actors
        let pick = |pos: Pos2| -> Option<String> {
            let mut best: Option<(f32, String)> = None;
            for fr in &frames {
                let a = &sc.actors[fr.idx];
                let h = a.def.height();
                let lift = fr.kin.sit * (fr.kin.seat_h + 0.03 - a.def.dims().leg);
                let (Some(c), Some(f)) = (project([fr.kin.pos[0], h * 0.5 + lift, fr.kin.pos[1]]), project([fr.kin.pos[0], 0.0, fr.kin.pos[1]])) else { continue };
                let radius = ((c.y - f.y).abs() * 1.1).max(24.0);
                let d = c.distance(pos);
                if d < radius && best.as_ref().map_or(true, |(bd, _)| d < *bd) {
                    best = Some((d, a.name.clone()));
                }
            }
            best.map(|(_, n)| n)
        };
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = resp.interact_pointer_pos() {
                if let Some(name) = pick(pos) {
                    self.sel_actor = Some(name.clone());
                    if lt > 0.05 {
                        self.status = "Editing the starting position: jumped to the start of the scene".into();
                        let t0 = sc.t0;
                        self.t = t0;
                    }
                    let start = sc.actors.iter().find(|a| a.name == name).map(|a| a.init_pos).unwrap_or([0.0, 0.0]);
                    self.vp.drag = Some(DragState { name, pos: start });
                }
            }
        }
        if resp.dragged_by(egui::PointerButton::Primary) {
            if let (Some(d), Some(p)) = (&mut self.vp.drag, resp.interact_pointer_pos()) {
                if let Some(g) = ground(p) {
                    let hw = sc.set.width / 2.0 - 0.3;
                    let hd = sc.set.depth / 2.0 - 0.3;
                    d.pos = [g[0].clamp(-hw, hw), g[1].clamp(-hd, hd)];
                }
            }
        }
        if resp.drag_stopped() {
            if let Some(d) = self.vp.drag.take() {
                let yaw = sc.actors.iter().find(|a| a.name == d.name).map(|a| a.init_yaw).unwrap_or(std::f64::consts::PI);
                let number = sc.number;
                if let Some(ws) = &mut self.ws {
                    ws.pin_place(number, &d.name, d.pos, yaw);
                }
                self.status = format!("{} placed at ({:.2}, {:.2}) and pinned in the .onlock", d.name, d.pos[0], d.pos[1]);
                self.vp.invalidate();
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                self.sel_actor = pick(pos);
            }
        }
    }

    // ------------------------------------------------------------------ recording

    pub fn start_recording(&mut self) {
        let Some(ws) = &self.ws else { return };
        let ep = ws.episode.clone();
        let Some((si, lt)) = ep.locate(self.t) else { return };
        let sc = &ep.scenes[si];
        let name = match self.sel_actor.clone().filter(|n| sc.actors.iter().any(|a| &a.name == n)).or_else(|| sc.actors.first().map(|a| a.name.clone())) {
            Some(n) => n,
            None => {
                self.status = "Place a character first (place Marta at ...)".into();
                return;
            }
        };
        let Some(span) = sc.spans.iter().rev().find(|s| s.t0 <= lt + 1e-6) else {
            self.status = "Add at least one statement to the scene before recording".into();
            return;
        };
        let actor_idx = sc.actors.iter().position(|a| a.name == name).unwrap();
        let end_line = statement_end_line(&ws.docs[ws.script].text, span.line);
        let t_local = span.t1;
        let kin = sc.actors[actor_idx].kin(t_local);
        let puppet = Puppet { pos: kin.pos, yaw: kin.yaw, odo: kin.odo, speed: 0.0, running: false };
        let label = format!("line {} ({})", end_line, span.label);
        self.recorder = Some(Recorder {
            actor: name,
            actor_idx,
            scene_idx: si,
            t_start_local: t_local,
            t_start_global: sc.t0 + t_local,
            start_pos: kin.pos,
            puppet,
            samples: vec![],
            clock: 0.0,
            last_sample: 0.0,
            insert_after_line: end_line,
            after_label: label,
        });
        self.t = sc.t0 + t_local;
        self.playing = false;
        self.vp.free = true;
        self.vp.cam = FreeCam::overview(sc.set.depth);
        self.status = "Recording: drive the character, then press Stop".into();
    }

    pub fn stop_recording(&mut self) {
        let Some(rec) = self.recorder.take() else { return };
        let mut samples = vec![Sample { t: 0.0, pos: rec.start_pos, running: false }];
        samples.extend(rec.samples.iter().copied());
        let lines = emit(&rec.actor, &samples);
        self.take = Some(Take { actor: rec.actor, lines, insert_after_line: rec.insert_after_line, seek_to: rec.t_start_global });
    }
}


fn fmt_pin(seg: Option<&oneiro_core::CameraSeg>, ws: &crate::workspace::Workspace, scene: u32) -> String {
    match seg {
        Some(s) => {
            let pinned = s.index > 0 && ws.is_shot_pinned(scene, s.index);
            format!("shot {}: {}{}", s.index, s.source, if pinned { "  [pinned]" } else { "" })
        }
        None => "no camera".into(),
    }
}

/// Intersects the camera ray through render-space pixel (px, py) with the floor (y = 0).
pub fn ground_hit(cam: &Cam, px: f64, py: f64) -> Option<V2> {
    let xc = (px - cam.cx) / cam.f;
    let yc = -(py - cam.cy) / cam.f;
    let dir = add3(cam.fwd, add3(mul3(cam.right, xc), mul3(cam.up, yc)));
    if dir[1] > -1e-4 {
        return None;
    }
    let t = -cam.pos[1] / dir[1];
    let hit = add3(cam.pos, mul3(dir, t));
    Some([hit[0], hit[2]])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clicking_a_projected_floor_point_returns_that_point() {
        let pose = CameraPose { pos: [1.0, 3.0, -5.0], target: [0.0, 0.5, 1.0], fov: 45.0, roll: 0.0 };
        let cam = Cam::new(&pose, 1280, 720);
        for p in [[0.0, 0.0], [1.5, 2.0], [-2.0, 1.0], [3.0, -1.0]] {
            let (x, y, _) = cam.project([p[0], 0.0, p[1]]).unwrap();
            let hit = ground_hit(&cam, x as f64, y as f64).unwrap();
            assert!(dist2(hit, p) < 1e-3, "{:?} -> {:?}", p, hit);
        }
        // A ray above the horizon never reaches the floor (flat camera).
        let flat = Cam::new(&CameraPose { pos: [0.0, 1.5, -5.0], target: [0.0, 1.5, 1.0], fov: 45.0, roll: 0.0 }, 1280, 720);
        assert!(ground_hit(&flat, 640.0, 5.0).is_none());
        assert!(ground_hit(&flat, 640.0, 700.0).is_some());
    }

    #[test]
    fn free_camera_round_trips_through_a_pose() {
        let cam = FreeCam { yaw: 0.7, pitch: 0.4, dist: 6.0, target: [1.0, 0.5, 0.0], fov: 40.0 };
        let back = FreeCam::from_pose(&cam.pose());
        assert!((back.yaw - cam.yaw).abs() < 1e-9 && (back.pitch - cam.pitch).abs() < 1e-9 && (back.dist - cam.dist).abs() < 1e-9);
    }
}
