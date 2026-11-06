//! Block-based character builder: pick parts and wardrobe from the library, tweak proportions
//! and colors, preview the puppet turning, walking and talking, then save a `.onchar`.

use crate::app::App;
use crate::workspace::Workspace;
use eframe::egui::{self, Color32, RichText, TextureOptions};
use oneiro_audio::Shape;
use oneiro_core::catalog::*;
use oneiro_core::{CharacterDef, Rgb};
use oneiro_render::preview::{character_portrait, PortraitOpts};

const MOUTHS_UI: [(&str, Option<Shape>); 7] = [
    ("closed", None),
    ("A (ah)", Some(Shape::A)),
    ("E (ee)", Some(Shape::E)),
    ("O (oh)", Some(Shape::O)),
    ("U (oo)", Some(Shape::U)),
    ("S (ss)", Some(Shape::S)),
    ("FV (ff)", Some(Shape::FV)),
];

pub struct BuilderState {
    pub draft: Option<CharacterDef>,
    pub original: String,
    pub is_new: bool,
    pub turn: f64,
    pub expression: String,
    pub mouth: usize,
    pub animate: bool,
    pub walking: bool,
    pub tex: Option<egui::TextureHandle>,
    pub tex_key: u64,
    pub new_name: String,
    pub traits_text: String,
    pub msg: String,
    pub voice_preview: bool,
}

impl Default for BuilderState {
    fn default() -> Self {
        BuilderState {
            draft: None,
            original: String::new(),
            is_new: false,
            turn: 0.0,
            expression: "neutral".into(),
            mouth: 0,
            animate: false,
            walking: false,
            tex: None,
            tex_key: 0,
            new_name: String::new(),
            traits_text: String::new(),
            msg: String::new(),
            voice_preview: false,
        }
    }
}

const TEMPLATE: &str = "character NAME {
  rig: library.rigs.biped_cutout
  build {
    head:  library.heads.round    { size: 1.0, skin_tone: #F2C9A0 }
    eyes:  library.eyes.ovals     { spacing: 0.4 }
    mouth: library.mouths.visemes_basic
    body:  library.bodies.kid     { height: 1.0 }
  }
  wardrobe {
    default: [green_sweater, jeans, black_shoes]
  }
  voice { provider: file, pitch: 180 }
  sheet { traits: [curious] }
  styles { cutout_2d: auto }
}
";

pub fn valid_name(n: &str) -> bool {
    let mut cs = n.chars();
    matches!(cs.next(), Some(c) if c.is_ascii_alphabetic()) && n.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

impl BuilderState {
    pub fn select(&mut self, ws: Option<&Workspace>, name: &str) {
        if let Some(def) = ws.and_then(|w| w.project.characters.get(name)) {
            self.traits_text = def.traits.join(", ");
            self.original = def.to_source();
            self.draft = Some(def.clone());
            self.is_new = false;
            self.tex_key = 0;
            self.msg.clear();
        }
    }

    pub fn start_new(&mut self, name: &str) {
        let src = TEMPLATE.replace("NAME", name);
        let (file, _) = oneiro_lang::parse(&src, "template");
        let mut d = Vec::new();
        if let Some(b) = file.blocks.first() {
            let def = CharacterDef::from_block(b, "template", &mut d);
            self.traits_text = def.traits.join(", ");
            self.original = String::new();
            self.draft = Some(def);
            self.is_new = true;
            self.tex_key = 0;
        }
    }
}

fn slot_label(s: Slot) -> &'static str {
    match s {
        Slot::Hat => "Hat",
        Slot::Top => "Top",
        Slot::Bottom => "Bottom",
        Slot::Shoes => "Shoes",
        Slot::Accessory => "Accessory",
    }
}

fn rgb_to_arr(c: Rgb) -> [u8; 3] {
    [c.0, c.1, c.2]
}

fn hash_str(s: &str) -> u64 {
    oneiro_core::math::hash_str(s)
}

impl App {
    pub fn ui_builder(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(ws) = self.ws.as_ref() else {
            ui.label("No project open.");
            return;
        };
        let names: Vec<String> = ws.project.characters.keys().cloned().collect();
        let mut pick: Option<String> = None;
        let mut start_new = false;
        ui.horizontal(|ui| {
            let current = self.builder.draft.as_ref().map(|d| d.name.clone()).unwrap_or("(choose)".into());
            egui::ComboBox::from_id_salt("builder_pick").selected_text(current).show_ui(ui, |ui| {
                for n in &names {
                    if ui.selectable_label(false, n).clicked() {
                        pick = Some(n.clone());
                    }
                }
            });
            ui.label("or");
            ui.add(egui::TextEdit::singleline(&mut self.builder.new_name).hint_text("name").desired_width(90.0));
            let ok = valid_name(&self.builder.new_name) && !names.contains(&capitalize(&self.builder.new_name));
            if ui.add_enabled(ok, egui::Button::new("+ New character")).on_hover_text("Letters, digits and _ (starting with a letter). The first letter becomes a capital.").clicked() {
                start_new = true;
            }
        });
        if let Some(n) = pick {
            self.builder.select(self.ws.as_ref(), &n);
        }
        if start_new {
            let n = capitalize(self.builder.new_name.trim());
            self.builder.start_new(&n);
        }
        ui.separator();
        if self.builder.draft.is_none() {
            ui.label("Pick a character to edit, or create a new one. Everything is built from library blocks - no drawing needed.");
            return;
        }

        // ------------------------------------------------------------ preview
        let b = &mut self.builder;
        if b.animate {
            ctx.request_repaint();
        }
        let time = if b.animate { ctx.input(|i| i.time) } else { 0.0 };
        let mouth = if b.animate {
            let shapes = [Shape::A, Shape::E, Shape::O, Shape::U, Shape::S, Shape::A, Shape::FV];
            let k = ((time * 7.0) as usize) % shapes.len();
            Some((shapes[k], 0.55 + 0.4 * ((time * 11.0).sin() as f32).abs()))
        } else {
            MOUTHS_UI[b.mouth].1.map(|s| (s, 0.7))
        };
        let opts = PortraitOpts { turn_deg: b.turn, expression: b.expression.clone(), mouth, time, walking: b.walking && b.animate };
        let mut key = hash_str(&b.draft.as_ref().unwrap().to_source());
        key ^= hash_str(&format!("{:?}{}{:?}{}{}", b.turn.to_bits(), b.expression, mouth.map(|m| (m.0 as u8, m.1.to_bits())), (time * 24.0) as i64, opts.walking));
        if b.tex.is_none() || key != b.tex_key {
            b.tex_key = key;
            let pm = character_portrait(b.draft.as_ref().unwrap(), &opts, 300, 340);
            let img = egui::ColorImage::from_rgba_premultiplied([300, 340], pm.data());
            match &mut b.tex {
                Some(t) => t.set(img, TextureOptions::LINEAR),
                None => b.tex = Some(ctx.load_texture("builder_preview", img, TextureOptions::LINEAR)),
            }
        }

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            let b = &mut self.builder;
            ui.horizontal(|ui| {
                if let Some(t) = &b.tex {
                    ui.add(egui::Image::new(egui::load::SizedTexture::new(t.id(), egui::vec2(220.0, 250.0))));
                }
                ui.vertical(|ui| {
                    ui.label(RichText::new("Preview").strong());
                    ui.add(egui::Slider::new(&mut b.turn, 0.0..=180.0).text("turn").suffix(" deg"));
                    egui::ComboBox::from_label("expression").selected_text(b.expression.clone()).show_ui(ui, |ui| {
                        for e in EXPRESSIONS {
                            ui.selectable_value(&mut b.expression, e.to_string(), *e);
                        }
                    });
                    egui::ComboBox::from_label("mouth").selected_text(MOUTHS_UI[b.mouth].0).show_ui(ui, |ui| {
                        for (i, (n, _)) in MOUTHS_UI.iter().enumerate() {
                            ui.selectable_value(&mut b.mouth, i, *n);
                        }
                    });
                    ui.checkbox(&mut b.animate, "Animate (talk)");
                    ui.checkbox(&mut b.walking, "Walk in place");
                });
            });
            ui.separator();

            let d = b.draft.as_mut().unwrap();
            ui.horizontal(|ui| {
                ui.label("Name");
                ui.label(RichText::new(&d.name).strong());
                if b.is_new {
                    ui.label(RichText::new("(new, not saved yet)").weak());
                }
            });

            ui.horizontal(|ui| {
                ui.label("Style");
                let shown = if d.style.is_empty() { "(episode default)".to_string() } else { d.style.clone() };
                egui::ComboBox::from_id_salt("char_style").selected_text(shown).show_ui(ui, |ui| {
                    ui.selectable_value(&mut d.style, String::new(), "(episode default)");
                    for s in STYLES {
                        ui.selectable_value(&mut d.style, s.to_string(), *s);
                    }
                });
            });
            ui.label(RichText::new("cutout_2d: paper cutout - rubber_hose: 1930s noodle limbs - clay: stop-motion plasticine - ink: pen drawing - toon_3d: real 3D. A script can override it with `use character X style Y`.").weak().small());
            if d.style == "toon_3d" {
                for (key, label, default) in [("cape", "cape (cloth physics)", Rgb(192, 32, 48)), ("tail", "tail (spring physics)", Rgb(224, 120, 42))] {
                    let slot = if key == "cape" { &mut d.toon3d.cape } else { &mut d.toon3d.tail };
                    let mut on = slot.is_some();
                    let mut arr = rgb_to_arr(slot.unwrap_or(default));
                    ui.horizontal(|ui| {
                        ui.checkbox(&mut on, label);
                        if on {
                            egui::color_picker::color_edit_button_srgb(ui, &mut arr);
                        }
                    });
                    *slot = on.then(|| Rgb(arr[0], arr[1], arr[2]));
                }
                ui.label(RichText::new("3D characters are real geometry with cel shading and ink outlines; the cape and tail are simulated in every scene.").weak().small());
            }
            if d.has_svg() {
                ui.label(RichText::new("This character is drawn from parts (library/parts): its head, body and clothes are those drawings, so they have no block controls here. Ask the AI tab to redraw it, or edit the SVG files. Eyes, voice, style and personality still apply.").color(Color32::from_rgb(120, 190, 240)).small());
            }
            if !d.has_svg() {
            egui::CollapsingHeader::new(RichText::new("Head").strong()).default_open(true).show(ui, |ui| {
                egui::ComboBox::from_label("shape").selected_text(d.head.kind.clone()).show_ui(ui, |ui| {
                    for k in HEADS {
                        ui.selectable_value(&mut d.head.kind, k.to_string(), *k);
                    }
                });
                let mut size = d.head.num("size", 1.0);
                if ui.add(egui::Slider::new(&mut size, 0.6..=1.5).text("size")).changed() {
                    d.head.nums.insert("size".into(), size);
                }
                let skin = d.head.color("skin_tone", Rgb(242, 201, 160));
                let mut arr = rgb_to_arr(skin);
                ui.horizontal(|ui| {
                    ui.label("skin");
                    if egui::color_picker::color_edit_button_srgb(ui, &mut arr).changed() {
                        d.head.colors.insert("skin_tone".into(), Rgb(arr[0], arr[1], arr[2]));
                    }
                    let hair = d.head.color("hair", Rgb(92, 60, 38));
                    let mut harr = rgb_to_arr(hair);
                    ui.label("hair");
                    if egui::color_picker::color_edit_button_srgb(ui, &mut harr).changed() {
                        d.head.colors.insert("hair".into(), Rgb(harr[0], harr[1], harr[2]));
                    }
                    let mut bald = d.head.num("bald", 0.0) > 0.5;
                    if ui.checkbox(&mut bald, "bald").changed() {
                        d.head.nums.insert("bald".into(), if bald { 1.0 } else { 0.0 });
                    }
                });
            });
            }
            egui::CollapsingHeader::new(RichText::new("Eyes").strong()).default_open(true).show(ui, |ui| {
                egui::ComboBox::from_label("style").selected_text(d.eyes.kind.clone()).show_ui(ui, |ui| {
                    for k in EYES {
                        ui.selectable_value(&mut d.eyes.kind, k.to_string(), *k);
                    }
                });
                let mut sp = d.eyes.num("spacing", 0.4);
                if ui.add(egui::Slider::new(&mut sp, 0.2..=0.7).text("spacing")).changed() {
                    d.eyes.nums.insert("spacing".into(), sp);
                }
            });
            if !d.has_svg() {
            egui::CollapsingHeader::new(RichText::new("Body").strong()).default_open(true).show(ui, |ui| {
                egui::ComboBox::from_label("build").selected_text(d.body.kind.clone()).show_ui(ui, |ui| {
                    for k in BODIES {
                        ui.selectable_value(&mut d.body.kind, k.to_string(), *k);
                    }
                });
                let mut h = d.body.num("height", 1.0);
                if ui.add(egui::Slider::new(&mut h, 0.6..=1.6).text("height")).changed() {
                    d.body.nums.insert("height".into(), h);
                }
                let mut w = d.body.num("width", 1.0);
                if ui.add(egui::Slider::new(&mut w, 0.7..=1.5).text("width")).changed() {
                    d.body.nums.insert("width".into(), w);
                }
            });
            egui::CollapsingHeader::new(RichText::new("Wardrobe").strong()).default_open(true).show(ui, |ui| {
                for slot in [Slot::Hat, Slot::Top, Slot::Bottom, Slot::Shoes, Slot::Accessory] {
                    let current = d.wardrobe.iter().find(|w| wardrobe_item(w).map(|i| i.slot) == Some(slot)).cloned();
                    let mut choice = current.clone();
                    egui::ComboBox::from_label(slot_label(slot)).selected_text(choice.clone().unwrap_or("(none)".into())).show_ui(ui, |ui| {
                        ui.selectable_value(&mut choice, None, "(none)");
                        for it in WARDROBE.iter().filter(|i| i.slot == slot) {
                            ui.selectable_value(&mut choice, Some(it.name.to_string()), it.name);
                        }
                    });
                    if choice != current {
                        d.wardrobe.retain(|w| wardrobe_item(w).map(|i| i.slot) != Some(slot));
                        if let Some(c) = choice {
                            d.wardrobe.push(c);
                        }
                    }
                }
            });
            }
            egui::CollapsingHeader::new(RichText::new("Voice and personality").strong()).default_open(false).show(ui, |ui| {
                egui::ComboBox::from_label("voice engine").selected_text(d.voice_provider.clone()).show_ui(ui, |ui| {
                    for (id, label) in [
                        ("file", "file: recorded or imported audio"),
                        ("demo", "demo: babble, preview only"),
                        ("sapi", "sapi: Windows speech (offline)"),
                        ("elevenlabs", "elevenlabs: cloud voices"),
                        ("openai", "openai: OpenAI-compatible / local server"),
                        ("command", "command: Piper, espeak..."),
                    ] {
                        ui.selectable_value(&mut d.voice_provider, id.to_string(), label);
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("voice");
                    ui.add(egui::TextEdit::singleline(&mut d.voice_id).hint_text("voice id or name (empty: engine default)").desired_width(250.0));
                });
                ui.add(egui::Slider::new(&mut d.voice_speed, 0.5..=2.0).text("speaking speed"));
                ui.add(egui::Slider::new(&mut d.voice_pitch, 70.0..=400.0).text("pitch (Hz, demo engine)"));
                ui.horizontal(|ui| {
                    if ui.button("Preview voice").clicked() {
                        b.voice_preview = true;
                    }
                    ui.label(RichText::new("Engines and keys: Audio tab > Voice settings.").weak().small());
                });
                ui.horizontal(|ui| {
                    ui.label("traits");
                    if ui.text_edit_singleline(&mut b.traits_text).changed() {
                        d.traits = b.traits_text.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect();
                    }
                });
                ui.label("notes for the AI");
                ui.text_edit_multiline(&mut d.notes);
            });
            ui.separator();

            let dirty = d.to_source() != b.original;
            let mut save = false;
            let mut revert = false;
            ui.horizontal(|ui| {
                if ui.add_enabled(dirty, egui::Button::new(RichText::new("Save to library").strong())).clicked() {
                    save = true;
                }
                if ui.add_enabled(dirty && !b.is_new, egui::Button::new("Revert")).clicked() {
                    revert = true;
                }
                if dirty {
                    ui.label(RichText::new("unsaved changes").color(Color32::from_rgb(230, 190, 70)));
                }
            });
            if !b.msg.is_empty() {
                ui.label(RichText::new(&b.msg).weak());
            }
            if save {
                let def = b.draft.clone().unwrap();
                let src = def.to_source();
                self.save_character(&def.name, &src);
            }
            if revert {
                let n = self.builder.draft.as_ref().map(|d| d.name.clone()).unwrap_or_default();
                self.builder.select(self.ws.as_ref(), &n);
            }
            if self.builder.voice_preview {
                self.builder.voice_preview = false;
                if let Some(def) = self.builder.draft.clone() {
                    let c = ui.ctx().clone();
                    self.preview_voice(&c, &def);
                }
            }
        });
    }

    fn save_character(&mut self, name: &str, source: &str) {
        let Some(ws) = &mut self.ws else { return };
        let path = ws.character_path(name);
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        match std::fs::write(&path, source) {
            Ok(_) => {
                // Keep an open editor tab in sync.
                if let Some(d) = ws.docs.iter_mut().find(|d| d.path == path) {
                    d.text = source.to_string();
                    d.saved = source.to_string();
                }
                ws.reload_project();
                ws.recompile();
                self.thumbs.clear();
                self.vp.invalidate();
                self.builder.original = source.to_string();
                self.builder.is_new = false;
                self.builder.msg = format!("Saved library/characters/{}.onchar", name);
                self.status = format!("Saved character {}", name);
            }
            Err(e) => self.builder.msg = format!("Could not save: {}", e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_must_start_with_a_capital() {
        assert!(valid_name("Luna") && valid_name("Dr_Who2"));
        assert!(valid_name("luna") && !valid_name("") && !valid_name("A B") && !valid_name("1Luna"));
    }

    #[test]
    fn a_new_character_is_valid_and_serializes_cleanly() {
        let mut b = BuilderState::default();
        b.start_new("Luna");
        let def = b.draft.clone().unwrap();
        assert_eq!(def.name, "Luna");
        assert!(b.is_new);
        let src = def.to_source();
        let (file, d) = oneiro_lang::parse(&src, "x");
        assert!(d.is_empty(), "{:?}", d);
        let mut diags = vec![];
        let back = CharacterDef::from_block(&file.blocks[0], "x", &mut diags);
        assert!(diags.is_empty(), "{:?}", diags);
        assert_eq!(back.to_source(), src);
        // Editing a wardrobe slot changes the source (so the builder knows it is dirty).
        let mut changed = def.clone();
        changed.wardrobe.clear();
        assert_ne!(changed.to_source(), src);
    }
}

impl BuilderState {
    /// Loads a character (for example one proposed by the AI) into the builder as an unsaved draft.
    pub fn load_draft(&mut self, def: CharacterDef, is_new: bool) {
        self.traits_text = def.traits.join(", ");
        self.original = if is_new { String::new() } else { def.to_source() };
        self.draft = Some(def);
        self.is_new = is_new;
        self.tex_key = 0;
        self.msg.clear();
    }
}

/// "luna" -> "Luna" (character names are capitalized so scripts can tell them from keywords).
pub fn capitalize(n: &str) -> String {
    let mut cs = n.chars();
    match cs.next() {
        Some(f) => f.to_uppercase().collect::<String>() + cs.as_str(),
        None => String::new(),
    }
}
