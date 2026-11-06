//! Inspector tab: selected character placement, selected shot (camera) and story state.
//! Editing a value here pins it in the `.onlock`, so the solver never overwrites it.

use crate::app::{App, RightTab};
use crate::viewport::FreeCam;
use eframe::egui::{self, Color32, DragValue, RichText};
use oneiro_core::CameraPose;

impl App {
    pub fn ui_inspector(&mut self, ui: &mut egui::Ui) {
        let Some(ep) = self.ws.as_ref().map(|w| w.episode.clone()) else {
            ui.label("No project open.");
            return;
        };
        let Some((si, lt)) = ep.locate(self.t) else {
            ui.label("The inspector appears once the script has a scene.");
            return;
        };
        let sc = &ep.scenes[si];
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(format!("Scene {} - {}", sc.number, if sc.title.is_empty() { "(untitled)" } else { &sc.title })).strong());
            ui.label(RichText::new(format!("set: {}   time of day: {}   playhead {:.2}s", sc.set.name, sc.time_of_day, lt)).weak());
            ui.separator();

            // ------------------------------------------------------------ character
            ui.label(RichText::new("Character").strong());
            let names: Vec<String> = sc.actors.iter().map(|a| a.name.clone()).collect();
            let mut sel = self.sel_actor.clone();
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("insp_actor").selected_text(sel.clone().unwrap_or("(none)".into())).show_ui(ui, |ui| {
                    ui.selectable_value(&mut sel, None, "(none)");
                    for n in &names {
                        ui.selectable_value(&mut sel, Some(n.clone()), n);
                    }
                });
                if let Some(n) = &self.sel_actor {
                    if ui.button("Edit in builder").clicked() {
                        let n = n.clone();
                        self.builder.select(self.ws.as_ref(), &n);
                        self.right_tab = RightTab::Character;
                    }
                }
            });
            self.sel_actor = sel;
            if let Some(name) = self.sel_actor.clone() {
                if let Some(a) = sc.actors.iter().find(|a| a.name == name) {
                    let pinned = self.ws.as_ref().map_or(false, |w| w.is_place_pinned(sc.number, &name));
                    let mut pos = a.init_pos;
                    let mut yaw = a.init_yaw.to_degrees();
                    let mut changed = false;
                    ui.horizontal(|ui| {
                        ui.label("start x");
                        changed |= ui.add(DragValue::new(&mut pos[0]).speed(0.02).suffix(" m")).changed();
                        ui.label("z");
                        changed |= ui.add(DragValue::new(&mut pos[1]).speed(0.02).suffix(" m")).changed();
                        ui.label("facing");
                        changed |= ui.add(DragValue::new(&mut yaw).speed(1.0).suffix(" deg")).changed();
                    });
                    ui.horizontal(|ui| {
                        if pinned {
                            ui.colored_label(Color32::from_rgb(150, 110, 255), "pinned in .onlock");
                            if ui.button("Unpin (back to the script's placement)").clicked() {
                                if let Some(ws) = &mut self.ws {
                                    ws.unpin_place(sc.number, &name);
                                }
                                self.vp.invalidate();
                            }
                        } else {
                            ui.label(RichText::new("placed by the script (edit a value or drag in the viewport to pin)").weak());
                        }
                    });
                    if changed {
                        if let Some(ws) = &mut self.ws {
                            ws.pin_place(sc.number, &name, pos, yaw.to_radians());
                        }
                        self.vp.invalidate();
                    }
                    let k = a.kin(lt);
                    ui.label(RichText::new(format!("now: ({:.2}, {:.2})  facing {:.0} deg  {}", k.pos[0], k.pos[1], k.yaw.to_degrees(), if k.sit > 0.5 { "seated" } else { "standing" })).weak());
                }
            }
            ui.separator();

            // ------------------------------------------------------------ shot
            ui.label(RichText::new("Shot").strong());
            let current = sc.cameras.iter().rev().find(|c| c.t0 <= lt).or(sc.cameras.first());
            let key = self.sel_shot.filter(|(s, _)| *s == sc.number).or_else(|| current.filter(|c| c.index > 0).map(|c| (sc.number, c.index)));
            let seg = key.and_then(|(_, i)| sc.cameras.iter().find(|c| c.index == i));
            match seg {
                None => {
                    ui.label(RichText::new("No editable shot at the playhead. Add a `camera` or `cut to` line, or click a block in the Camera row.").weak());
                }
                Some(seg) => {
                    let (scene_no, index) = (sc.number, seg.index);
                    let pinned = self.ws.as_ref().map_or(false, |w| w.is_shot_pinned(scene_no, index));
                    ui.label(RichText::new(format!("shot {}: {}", index, seg.source)).monospace());
                    let mut pose = seg.base;
                    let mut changed = false;
                    egui::Grid::new("shot_grid").num_columns(4).show(ui, |ui| {
                        ui.label("position");
                        for i in 0..3 {
                            changed |= ui.add(DragValue::new(&mut pose.pos[i]).speed(0.02)).changed();
                        }
                        ui.end_row();
                        ui.label("looks at");
                        for i in 0..3 {
                            changed |= ui.add(DragValue::new(&mut pose.target[i]).speed(0.02)).changed();
                        }
                        ui.end_row();
                        ui.label("lens fov");
                        changed |= ui.add(DragValue::new(&mut pose.fov).speed(0.2).range(8.0..=110.0).suffix(" deg")).changed();
                        ui.label("roll");
                        changed |= ui.add(DragValue::new(&mut pose.roll).speed(0.2).range(-45.0..=45.0).suffix(" deg")).changed();
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if pinned {
                            ui.colored_label(Color32::from_rgb(150, 110, 255), "pinned in .onlock");
                        } else {
                            ui.label(RichText::new("solved from the script").weak());
                        }
                    });
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Look through this shot").clicked() {
                            self.vp.free = false;
                            self.seek(sc.t0 + seg.t0 + 0.001);
                        }
                        if ui.button("Pin the free-camera view as this shot").on_hover_text("Orbit in the viewport, then store that view here").clicked() {
                            let p = self.vp.cam.pose();
                            let p = CameraPose { fov: seg.base.fov, ..p };
                            if let Some(ws) = &mut self.ws {
                                ws.pin_shot(scene_no, index, p, seg.source.clone());
                            }
                            self.vp.invalidate();
                        }
                        if ui.button("Free camera from this shot").clicked() {
                            self.vp.cam = FreeCam::from_pose(&seg.base);
                            self.vp.free = true;
                        }
                        if pinned && ui.button("Unpin (recompute from script)").clicked() {
                            if let Some(ws) = &mut self.ws {
                                ws.unpin_shot(scene_no, index);
                            }
                            self.vp.invalidate();
                        }
                    });
                    if changed {
                        if let Some(ws) = &mut self.ws {
                            ws.pin_shot(scene_no, index, pose, seg.source.clone());
                        }
                        self.sel_shot = Some((scene_no, index));
                        self.vp.invalidate();
                    }
                }
            }
            ui.separator();

            // ------------------------------------------------------------ story state
            ui.label(RichText::new("Story state (end of episode)").strong());
            let st = &ep.state;
            if st.is_empty() {
                ui.label(RichText::new("Nothing recorded yet. Use give / wound / relation / remember or `Marta.mood = angry` in the script.").weak());
            } else {
                for (k, v) in &st.fields {
                    ui.label(format!("{} = {}", k, v));
                }
                for (who, items) in &st.inventory {
                    ui.label(format!("{} carries: {}", who, items.join(", ")));
                }
                for (who, ws_) in &st.wounds {
                    for (part, s) in ws_ {
                        ui.label(format!("{}: {} {}", who, part, s));
                    }
                }
                for (a, b, v) in &st.relations {
                    ui.label(format!("{} -> {}: {}", a, b, v));
                }
                for (who, ms) in &st.memories {
                    for m in ms {
                        ui.label(format!("{} remembers: {}", who, m));
                    }
                }
            }
        });
    }
}
