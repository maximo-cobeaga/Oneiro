//! The timeline: script statements, camera shots and one lane group per character, with a
//! scrubbable playhead. Clicking a block jumps to the script line that produced it.

use crate::i18n::tr;
use crate::app::{fmt_time, App};
use crate::theme;
use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke};
use oneiro_core::catalog::Gesture;

#[derive(Default)]
pub struct TimelineView {
    pub view_start: f64,
    /// 0 means "fit the whole episode".
    pub view_len: f64,
}

enum Hit {
    Script { line: u32, t0: f64 },
    Shot { scene: u32, index: u32, t0: f64 },
    Time(f64),
}

struct Block {
    rect: Rect,
    hit: Hit,
    tip: String,
}

fn gesture_name(g: Gesture) -> &'static str {
    match g {
        Gesture::Wave => "wave",
        Gesture::Point => "point",
        Gesture::Shrug => "shrug",
        Gesture::Nod => "nod",
        Gesture::ShakeHead => "shake head",
        Gesture::CrossArms => "cross arms",
        Gesture::FacePalm => "facepalm",
    }
}

const ROW_H: f32 = 30.0;
const RULER_H: f32 = 22.0;
const LABEL_W: f32 = 108.0;

impl App {
    pub fn ui_timeline(&mut self, ui: &mut egui::Ui) {
        let Some(ep) = self.ws.as_ref().map(|w| w.episode.clone()) else {
            ui.label(tr("No project open"));
            return;
        };
        if ep.scenes.is_empty() {
            ui.label(tr("The timeline appears once the script has a scene."));
            return;
        }
        let total = ep.duration.max(0.5);
        if self.tl.view_len <= 0.0 {
            self.tl.view_len = total;
            self.tl.view_start = 0.0;
        }
        ui.horizontal(|ui| {
            if ui.small_button(tr("Fit")).clicked() {
                self.tl.view_start = 0.0;
                self.tl.view_len = total;
            }
            ui.label(egui::RichText::new(tr("wheel: zoom   shift+wheel / middle-drag: pan   click: seek / jump to script line")).weak().small());
        });

        // Rows: script, camera, one per distinct actor name.
        let mut names: Vec<String> = Vec::new();
        for sc in &ep.scenes {
            for a in &sc.actors {
                if !names.contains(&a.name) {
                    names.push(a.name.clone());
                }
            }
        }
        let rows = 2 + names.len();
        let height = RULER_H + rows as f32 * ROW_H + 4.0;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let width = ui.available_width();
            let (resp, painter) = ui.allocate_painter(egui::vec2(width, height), Sense::click_and_drag());
            let rect = resp.rect;
            let x0 = rect.left() + LABEL_W;
            let area_w = (rect.width() - LABEL_W).max(50.0);
            let pps = area_w as f64 / self.tl.view_len.max(0.05);

            // Zoom / pan
            if resp.hovered() {
                let (scroll, shift) = ui.ctx().input(|i| (i.smooth_scroll_delta, i.modifiers.shift));
                if scroll.y != 0.0 || scroll.x != 0.0 {
                    if shift || scroll.x != 0.0 {
                        let d = if scroll.x != 0.0 { scroll.x } else { scroll.y } as f64;
                        self.tl.view_start -= d / pps;
                    } else if let Some(p) = resp.hover_pos() {
                        let anchor = self.tl.view_start + ((p.x - x0) as f64 / pps);
                        let factor = (-(scroll.y as f64) * 0.004).exp();
                        let new_len = (self.tl.view_len * factor).clamp(0.5, total * 1.2);
                        let frac = ((p.x - x0) as f64 / area_w as f64).clamp(0.0, 1.0);
                        self.tl.view_start = anchor - frac * new_len;
                        self.tl.view_len = new_len;
                    }
                }
            }
            if resp.dragged_by(egui::PointerButton::Middle) {
                self.tl.view_start -= resp.drag_delta().x as f64 / pps;
            }
            // Keep the playhead visible while playing.
            if self.playing && (self.t < self.tl.view_start || self.t > self.tl.view_start + self.tl.view_len) {
                self.tl.view_start = (self.t - self.tl.view_len * 0.1).max(0.0);
            }
            self.tl.view_start = self.tl.view_start.clamp(-0.5, (total - self.tl.view_len * 0.5).max(0.0));
            let (vs, vl) = (self.tl.view_start, self.tl.view_len);
            let pps = area_w as f64 / vl.max(0.05);
            let x_of = |t: f64| -> f32 { x0 + ((t - vs) * pps) as f32 };
            let t_of = |x: f32| -> f64 { vs + (x - x0) as f64 / pps };

            // Backgrounds
            painter.rect_filled(rect, 0.0, theme::PANEL_DARK);
            let row_top = |r: usize| rect.top() + RULER_H + r as f32 * ROW_H;
            for r in 0..rows {
                let c = if r % 2 == 0 { Color32::from_rgb(30, 28, 40) } else { Color32::from_rgb(24, 22, 32) };
                painter.rect_filled(Rect::from_min_max(Pos2::new(rect.left(), row_top(r)), Pos2::new(rect.right(), row_top(r) + ROW_H)), 0.0, c);
            }
            let label_row = |r: usize, text: &str, color: Color32| {
                painter.text(Pos2::new(rect.left() + 8.0, row_top(r) + ROW_H / 2.0), egui::Align2::LEFT_CENTER, text, egui::FontId::proportional(12.0), color);
            };
            label_row(0, crate::i18n::tr("Script"), Color32::from_gray(190));
            label_row(1, crate::i18n::tr("Camera"), Color32::from_rgb(190, 150, 255));
            for (i, n) in names.iter().enumerate() {
                label_row(2 + i, n, Color32::from_rgb(250, 220, 120));
            }
            painter.line_segment([Pos2::new(x0, rect.top()), Pos2::new(x0, rect.bottom())], Stroke::new(1.0, Color32::from_gray(60)));

            // Ruler
            let steps = [0.1, 0.25, 0.5, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0];
            let step = steps.iter().copied().find(|s| s * pps >= 56.0).unwrap_or(120.0);
            let mut t = (vs / step).floor() * step;
            while t <= vs + vl {
                let x = x_of(t);
                if x >= x0 - 1.0 && x <= rect.right() {
                    painter.line_segment([Pos2::new(x, rect.top() + RULER_H - 7.0), Pos2::new(x, rect.top() + RULER_H)], Stroke::new(1.0, Color32::from_gray(110)));
                    painter.text(Pos2::new(x + 3.0, rect.top() + 3.0), egui::Align2::LEFT_TOP, fmt_time(t), egui::FontId::monospace(10.0), Color32::from_gray(150));
                    painter.line_segment([Pos2::new(x, rect.top() + RULER_H), Pos2::new(x, rect.bottom())], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 10)));
                }
                t += step;
            }

            let clip = Rect::from_min_max(Pos2::new(x0, rect.top()), rect.max);
            let lane = painter.with_clip_rect(clip);
            let mut blocks: Vec<Block> = Vec::new();
            let sel = self.sel_shot;

            for sc in &ep.scenes {
                // Scene boundary
                let xs = x_of(sc.t0);
                lane.line_segment([Pos2::new(xs, rect.top()), Pos2::new(xs, rect.bottom())], Stroke::new(1.5, Color32::from_rgb(120, 90, 200)));
                lane.text(Pos2::new(xs + 4.0, rect.top() + RULER_H + 2.0 + ROW_H * (rows as f32 - 0.3)), egui::Align2::LEFT_BOTTOM, format!("scene {}", sc.number), egui::FontId::proportional(10.0), Color32::from_rgb(150, 120, 230));

                // Script row
                for s in &sc.spans {
                    let (a, b) = (x_of(sc.t0 + s.t0), x_of(sc.t0 + s.t1));
                    let r = Rect::from_min_max(Pos2::new(a, row_top(0) + 4.0), Pos2::new(b.max(a + 4.0), row_top(0) + ROW_H - 4.0));
                    let zero = s.t1 - s.t0 < 1e-6;
                    let col = if zero { Color32::from_rgb(110, 110, 130) } else { Color32::from_rgb(70, 74, 110) };
                    lane.rect_filled(r, 3.0, col);
                    if !zero && r.width() > 30.0 {
                        lane.with_clip_rect(r).text(r.left_center() + egui::vec2(4.0, 0.0), egui::Align2::LEFT_CENTER, &s.label, egui::FontId::proportional(10.5), Color32::from_gray(230));
                    }
                    blocks.push(Block { rect: r, hit: Hit::Script { line: s.line, t0: sc.t0 + s.t0 }, tip: format!("line {}: {}\n{} - {}", s.line, s.label, fmt_time(sc.t0 + s.t0), fmt_time(sc.t0 + s.t1)) });
                }

                // Camera row
                for c in &sc.cameras {
                    let (a, b) = (x_of(sc.t0 + c.t0), x_of(sc.t0 + c.t1));
                    let r = Rect::from_min_max(Pos2::new(a, row_top(1) + 3.0), Pos2::new(b, row_top(1) + ROW_H - 3.0));
                    let selected = sel == Some((sc.number, c.index));
                    let pinned = c.index > 0 && self.ws.as_ref().map_or(false, |w| w.is_shot_pinned(sc.number, c.index));
                    lane.rect_filled(r.shrink2(egui::vec2(0.5, 0.0)), 3.0, Color32::from_rgb(96, 64, 170));
                    if selected {
                        lane.rect_stroke(r, 3.0, Stroke::new(2.0, Color32::WHITE), egui::StrokeKind::Inside);
                    }
                    let label = format!("{}{}", if pinned { "pin " } else { "" }, c.source.trim_start_matches("camera "));
                    lane.with_clip_rect(r).text(r.left_center() + egui::vec2(5.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(10.5), Color32::from_gray(240));
                    blocks.push(Block { rect: r, hit: Hit::Shot { scene: sc.number, index: c.index, t0: sc.t0 + c.t0 }, tip: format!("shot {}: {}{}", c.index, c.source, if pinned { "  (pinned)" } else { "" }) });
                }

                // Actor rows
                for (ri, name) in names.iter().enumerate() {
                    let Some(a) = sc.actors.iter().find(|a| &a.name == name) else { continue };
                    let top = row_top(2 + ri);
                    let bar = |t0: f64, t1: f64, lane_i: f32, col: Color32, label: &str, blocks: &mut Vec<Block>| {
                        let (xa, xb) = (x_of(sc.t0 + t0), x_of(sc.t0 + t1.min(sc.dur)));
                        let y0 = top + 3.0 + lane_i * 12.0;
                        let r = Rect::from_min_max(Pos2::new(xa, y0), Pos2::new(xb.max(xa + 3.0), y0 + 11.0));
                        lane.rect_filled(r, 2.0, col);
                        if r.width() > 26.0 {
                            lane.with_clip_rect(r).text(r.left_center() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(9.5), Color32::from_gray(20));
                        }
                        blocks.push(Block { rect: r, hit: Hit::Time(sc.t0 + t0), tip: format!("{}: {}\n{} - {}", name, label, fmt_time(sc.t0 + t0), fmt_time(sc.t0 + t1.min(sc.dur))) });
                    };
                    for m in &a.moves {
                        bar(m.t0, m.t1, 0.0, Color32::from_rgb(100, 205, 130), if m.run { "run" } else { "walk" }, &mut blocks);
                    }
                    for s in &a.sits {
                        bar(s.t0, s.t1, 0.0, Color32::from_rgb(205, 150, 90), if s.to > 0.5 { "sit" } else { "stand" }, &mut blocks);
                    }
                    for g in &a.gestures {
                        bar(g.t0, g.t1, 1.0, Color32::from_rgb(110, 160, 245), gesture_name(g.kind), &mut blocks);
                    }
                    for s in &a.speech {
                        bar(s.t0, s.t1, 1.0, Color32::from_rgb(245, 175, 85), "speech", &mut blocks);
                    }
                    for e in &a.exprs {
                        let x = x_of(sc.t0 + e.t);
                        let c = Pos2::new(x, top + ROW_H - 5.0);
                        lane.add(egui::Shape::convex_polygon(
                            vec![c + egui::vec2(0.0, -4.0), c + egui::vec2(4.0, 0.0), c + egui::vec2(0.0, 4.0), c + egui::vec2(-4.0, 0.0)],
                            Color32::from_rgb(240, 130, 190),
                            Stroke::NONE,
                        ));
                    }
                }
            }

            // Playhead
            let px = x_of(self.t);
            if px >= x0 - 1.0 && px <= rect.right() {
                painter.line_segment([Pos2::new(px, rect.top()), Pos2::new(px, rect.bottom())], Stroke::new(2.0, Color32::from_rgb(255, 80, 90)));
                painter.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(px - 6.0, rect.top()), Pos2::new(px + 6.0, rect.top()), Pos2::new(px, rect.top() + 9.0)],
                    Color32::from_rgb(255, 80, 90),
                    Stroke::NONE,
                ));
            }

            // Interaction
            let hover = resp.hover_pos();
            if let Some(h) = hover {
                if h.x >= x0 {
                    if let Some(b) = blocks.iter().rev().find(|b| b.rect.contains(h)) {
                        let tip = b.tip.clone();
                        resp.clone().on_hover_text(tip);
                    }
                }
            }
            let pointer = resp.interact_pointer_pos();
            if resp.dragged_by(egui::PointerButton::Primary) {
                if let Some(p) = pointer {
                    if p.x >= x0 {
                        self.playing = false;
                        self.seek(t_of(p.x));
                    }
                }
            }
            if resp.clicked() {
                if let Some(p) = pointer {
                    if p.x >= x0 {
                        let hit = if p.y > rect.top() + RULER_H { blocks.iter().rev().find(|b| b.rect.contains(p)).map(|b| &b.hit) } else { None };
                        match hit {
                            Some(Hit::Script { line, t0 }) => {
                                self.jump_to = Some((*line, 1));
                                self.right_tab = crate::app::RightTab::Script;
                                self.playing = false;
                                self.seek(*t0);
                            }
                            Some(Hit::Shot { scene, index, t0 }) => {
                                if *index > 0 {
                                    self.sel_shot = Some((*scene, *index));
                                    self.right_tab = crate::app::RightTab::Inspector;
                                }
                                self.playing = false;
                                self.seek(*t0 + 0.001);
                            }
                            Some(Hit::Time(t)) => {
                                self.playing = false;
                                self.seek(*t);
                            }
                            None => {
                                self.playing = false;
                                self.seek(t_of(p.x));
                            }
                        }
                    }
                }
            }
        });
    }
}
