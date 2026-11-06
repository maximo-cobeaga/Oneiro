//! Dark theme with a violet accent.

use eframe::egui::{self, Color32, CornerRadius, Stroke};

pub const ACCENT: Color32 = Color32::from_rgb(150, 100, 255);
pub const PANEL: Color32 = Color32::from_rgb(26, 24, 34);
pub const PANEL_DARK: Color32 = Color32::from_rgb(18, 17, 24);

pub fn apply(ctx: &egui::Context) {
    let mut v = egui::Visuals::dark();
    v.panel_fill = PANEL;
    v.window_fill = Color32::from_rgb(32, 30, 42);
    v.extreme_bg_color = PANEL_DARK;
    v.faint_bg_color = Color32::from_rgb(34, 32, 44);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(150, 100, 255, 90);
    v.selection.stroke = Stroke::new(1.0, ACCENT);
    v.hyperlink_color = Color32::from_rgb(150, 190, 255);
    let r = CornerRadius::same(4);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.corner_radius = r;
    v.widgets.active.corner_radius = r;
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(46, 43, 60);
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(64, 58, 88);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(90, 70, 150);
    ctx.set_visuals(v);
    ctx.global_style_mut(|s| {
        s.spacing.item_spacing = egui::vec2(6.0, 5.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}
