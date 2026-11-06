//! Application state and top-level layout of Oneiro Studio.

use crate::ai::AiState;
use crate::builder::BuilderState;
use crate::record::Recorder;
use crate::studio::AudioState;
use crate::timeline::TimelineView;
use crate::viewport::Viewport;
use crate::workspace::Workspace;
use eframe::egui::{self, Color32, RichText};
use oneiro_render::encode::{encode_episode, EncodeOptions};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RightTab {
    Script,
    Character,
    Inspector,
    Audio,
    Ai,
}

pub struct Take {
    pub actor: String,
    pub lines: Vec<String>,
    pub insert_after_line: u32,
    pub seek_to: f64,
}

pub struct ExportJob {
    pub progress: Arc<AtomicUsize>,
    pub total: usize,
    pub cancel: Arc<AtomicBool>,
    pub result: Arc<Mutex<Option<Result<u64, String>>>>,
    pub out: PathBuf,
    pub started: Instant,
}

/// Command line switches used to script the IDE (screenshots for documentation and tests).
#[derive(Default, Clone)]
pub struct StartupOpts {
    pub project: Option<PathBuf>,
    pub screenshot: Option<PathBuf>,
    pub after_frames: u64,
    pub time: Option<f64>,
    pub tab: Option<String>,
    pub select: Option<String>,
    pub free_cam: bool,
    pub exit_after: bool,
    pub width: Option<f32>,
    pub height: Option<f32>,
    pub demo_record: bool,
    pub demo_insert: Option<String>,
    pub export: Option<PathBuf>,
    pub ai_provider: Option<String>,
    pub ai_request: Option<String>,
    pub ai_apply: bool,
    /// Plays the timeline for a moment and checks that the speakers follow the playhead.
    pub audio_selftest: bool,
    /// Generates the missing voices with this provider, prints the result and exits.
    pub demo_generate: Option<String>,
}

pub struct App {
    pub ws: Option<Workspace>,
    pub error: Option<String>,
    pub status: String,

    // playback
    pub t: f64,
    pub playing: bool,
    pub looping: bool,
    pub last_tick: Option<Instant>,

    pub vp: Viewport,
    pub tl: TimelineView,
    pub sel_actor: Option<String>,
    pub sel_shot: Option<(u32, u32)>,
    pub right_tab: RightTab,
    pub builder: BuilderState,
    pub ai: AiState,
    pub audio: AudioState,
    pub thumbs: HashMap<String, (u64, egui::TextureHandle)>,
    pub set_thumbs: HashMap<String, (u64, egui::TextureHandle)>,

    // editor interaction
    pub jump_to: Option<(u32, u32)>,
    pub insert_request: Option<String>,
    pub cursor_char: usize,
    pub follow_cursor: bool,
    pub last_cursor_line: u32,

    // recording
    pub recorder: Option<Recorder>,
    pub take: Option<Take>,

    // export
    pub export: Option<ExportJob>,
    pub export_res: usize,
    pub show_help: bool,

    pub startup: StartupOpts,
    pub frame_counter: u64,
    pub screenshot_requested: bool,
    pub ai_phase: u8,
}

const RES: [(&str, u32, u32); 3] = [("Preview 640x360", 640, 360), ("HD 1280x720", 1280, 720), ("Full HD 1920x1080", 1920, 1080)];

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, startup: StartupOpts) -> App {
        crate::theme::apply(&cc.egui_ctx);
        let mut app = App {
            ws: None,
            error: None,
            status: "Open a project to start (File > Open project...)".into(),
            t: 0.0,
            playing: false,
            looping: true,
            last_tick: None,
            vp: Viewport::default(),
            tl: TimelineView::default(),
            sel_actor: None,
            sel_shot: None,
            right_tab: RightTab::Script,
            builder: BuilderState::default(),
            ai: AiState::default(),
            audio: AudioState::default(),
            thumbs: HashMap::new(),
            set_thumbs: HashMap::new(),
            jump_to: None,
            insert_request: None,
            cursor_char: 0,
            follow_cursor: false,
            last_cursor_line: 0,
            recorder: None,
            take: None,
            export: None,
            export_res: 1,
            show_help: false,
            startup: startup.clone(),
            frame_counter: 0,
            screenshot_requested: false,
            ai_phase: 0,
        };
        if let Some(p) = &startup.project {
            app.open_project(p);
        }
        if let Some(t) = startup.time {
            app.t = t;
        }
        if let Some(tab) = &startup.tab {
            app.right_tab = match tab.as_str() {
                "character" => RightTab::Character,
                "inspector" => RightTab::Inspector,
                "audio" => RightTab::Audio,
                "ai" => RightTab::Ai,
                _ => RightTab::Script,
            };
        }
        if let Some(name) = &startup.select {
            app.sel_actor = Some(name.clone());
            if app.right_tab == RightTab::Character {
                app.builder.select(app.ws.as_ref(), name);
            }
        }
        app.vp.free = startup.free_cam;
        app
    }

    pub fn open_project(&mut self, path: &std::path::Path) {
        match Workspace::open(path) {
            Ok(ws) => {
                self.status = format!("Opened {}", ws.root.display());
                self.error = None;
                self.t = 0.0;
                self.playing = false;
                self.sel_actor = None;
                self.sel_shot = None;
                self.recorder = None;
                self.take = None;
                self.thumbs.clear();
                self.set_thumbs.clear();
                self.builder = BuilderState::default();
                self.audio.on_project_change();
                self.vp.invalidate();
                self.ws = Some(ws);
            }
            Err(e) => {
                self.error = Some(e.clone());
                self.status = e;
            }
        }
    }

    pub fn duration(&self) -> f64 {
        self.ws.as_ref().map_or(0.0, |w| w.episode.duration)
    }

    pub fn fps(&self) -> f64 {
        self.ws.as_ref().map_or(24.0, |w| w.episode.fps as f64)
    }

    pub fn seek(&mut self, t: f64) {
        self.t = t.clamp(0.0, self.duration());
    }

    fn tick(&mut self, ctx: &egui::Context) {
        let now = Instant::now();
        if self.playing && self.recorder.is_none() {
            if let Some(last) = self.last_tick {
                self.t += (now - last).as_secs_f64();
            }
            let dur = self.duration();
            if self.t >= dur {
                if self.looping && dur > 0.0 {
                    self.t = 0.0;
                } else {
                    self.t = dur;
                    self.playing = false;
                }
            }
            ctx.request_repaint();
        }
        self.last_tick = Some(now);
        if let Some(ws) = &mut self.ws {
            if let Some(msg) = ws.check_external(false) {
                self.status = msg;
                self.vp.invalidate();
            }
            ctx.request_repaint_after(Duration::from_millis(1000));
        }
        if let Some(ws) = &mut self.ws {
            if let Some(le) = ws.last_edit {
                if le.elapsed() > Duration::from_millis(350) {
                    ws.recompile();
                    self.vp.invalidate();
                } else {
                    ctx.request_repaint_after(Duration::from_millis(120));
                }
            }
        }
        // Export job completion
        if let Some(job) = &self.export {
            if job.result.lock().map(|r| r.is_some()).unwrap_or(false) {
                ctx.request_repaint();
            } else {
                ctx.request_repaint_after(Duration::from_millis(100));
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        let typing = ctx.egui_wants_keyboard_input();
        let (save, space, left, right, home, end, esc) = ctx.input(|i| {
            (
                i.modifiers.command && i.key_pressed(egui::Key::S),
                i.key_pressed(egui::Key::Space),
                i.key_pressed(egui::Key::ArrowLeft),
                i.key_pressed(egui::Key::ArrowRight),
                i.key_pressed(egui::Key::Home),
                i.key_pressed(egui::Key::End),
                i.key_pressed(egui::Key::Escape),
            )
        });
        if save {
            self.save_all();
        }
        if esc && self.recorder.is_some() {
            self.recorder = None;
            self.status = "Recording cancelled".into();
        }
        if typing || self.recorder.is_some() {
            return;
        }
        let frame = 1.0 / self.fps();
        if space {
            self.toggle_play();
        }
        if left {
            self.playing = false;
            self.seek(self.t - frame);
        }
        if right {
            self.playing = false;
            self.seek(self.t + frame);
        }
        if home {
            self.seek(0.0);
        }
        if end {
            self.seek(self.duration());
        }
    }

    pub fn toggle_play(&mut self) {
        if self.playing {
            self.playing = false;
        } else {
            if self.t >= self.duration() - 1e-6 {
                self.t = 0.0;
            }
            self.playing = true;
            self.last_tick = Some(Instant::now());
        }
    }

    pub fn save_all(&mut self) {
        if let Some(ws) = &mut self.ws {
            match ws.save_all() {
                Ok(0) => self.status = "Nothing to save".into(),
                Ok(n) => {
                    self.status = format!("Saved {} file(s)", n);
                    self.vp.invalidate();
                    self.thumbs.clear();
                    self.set_thumbs.clear();
                }
                Err(e) => self.status = format!("Save failed: {}", e),
            }
        }
    }

    pub fn start_export(&mut self, out: PathBuf, w: u32, h: u32, preview: bool) {
        let Some(ws) = &self.ws else { return };
        if !ws.valid {
            self.status = "Fix the errors in the script before rendering".into();
            return;
        }
        let ep = ws.episode.clone();
        let progress = Arc::new(AtomicUsize::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        let result = Arc::new(Mutex::new(None));
        let total = ep.frame_count();
        let (p2, c2, r2, out2) = (progress.clone(), cancel.clone(), result.clone(), out.clone());
        std::thread::spawn(move || {
            let opts = EncodeOptions { width: w, height: h, preview, audio: true };
            let res = encode_episode(&ep, &out2, &opts, &mut |done, _| {
                p2.store(done, Ordering::Relaxed);
                !c2.load(Ordering::Relaxed)
            });
            *r2.lock().unwrap() = Some(res);
        });
        self.export = Some(ExportJob { progress, total, cancel, result, out, started: Instant::now() });
    }

    fn menu(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("Open project...").clicked() {
                    if let Some(p) = rfd::FileDialog::new().set_title("Open an Oneiro project folder").pick_folder() {
                        self.open_project(&p);
                    }
                    ui.close();
                }
                if ui.add_enabled(self.ws.is_some(), egui::Button::new("Save all      Ctrl+S")).clicked() {
                    self.save_all();
                    ui.close();
                }
                ui.separator();
                let can_render = self.ws.as_ref().map_or(false, |w| w.valid) && self.export.is_none();
                for (i, (label, w, h)) in RES.iter().enumerate() {
                    if ui.add_enabled(can_render, egui::Button::new(format!("Render mp4 - {}...", label))).clicked() {
                        self.export_res = i;
                        let default_name =
                            self.ws.as_ref().map(|w| w.script_doc().path.file_stem().unwrap().to_string_lossy().to_string()).unwrap_or("out".into());
                        let dir = self.ws.as_ref().map(|w| w.root.join("out")).unwrap_or_default();
                        let _ = std::fs::create_dir_all(&dir);
                        if let Some(p) = rfd::FileDialog::new()
                            .set_title("Save video")
                            .set_directory(&dir)
                            .set_file_name(format!("{}.mp4", default_name))
                            .add_filter("MP4 video", &["mp4"])
                            .save_file()
                        {
                            self.start_export(p, *w, *h, i == 0);
                        }
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Quit").clicked() {
                    ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
            ui.menu_button("View", |ui| {
                ui.checkbox(&mut self.vp.free, "Free camera (right-drag to orbit)");
                ui.checkbox(&mut self.vp.show_labels, "Actor labels");
                ui.checkbox(&mut self.looping, "Loop playback");
                ui.checkbox(&mut self.follow_cursor, "Playhead follows editor cursor");
            });
            ui.menu_button("Help", |ui| {
                if ui.button("Shortcuts and tips").clicked() {
                    self.show_help = true;
                    ui.close();
                }
            });
        });
    }

    fn transport(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if ui.button("|<").on_hover_text("Go to start (Home)").clicked() {
                self.seek(0.0);
            }
            if ui.button("<").on_hover_text("Previous frame (Left)").clicked() {
                self.playing = false;
                let f = 1.0 / self.fps();
                self.seek(self.t - f);
            }
            let label = if self.playing { "Pause" } else { "Play" };
            if ui.add(egui::Button::new(RichText::new(label).strong()).min_size(egui::vec2(58.0, 0.0))).on_hover_text("Space").clicked() {
                self.toggle_play();
            }
            if ui.button(">").on_hover_text("Next frame (Right)").clicked() {
                self.playing = false;
                let f = 1.0 / self.fps();
                self.seek(self.t + f);
            }
            if ui.button(">|").on_hover_text("Go to end (End)").clicked() {
                self.seek(self.duration());
            }
            ui.separator();
            let dur = self.duration();
            ui.monospace(format!("{}  /  {}   frame {}", fmt_time(self.t), fmt_time(dur), (self.t * self.fps()).round() as i64));
            ui.separator();
            let valid = self.ws.as_ref().map_or(true, |w| w.valid);
            if !valid {
                ui.label(RichText::new("script has errors - showing last valid version").color(Color32::from_rgb(240, 120, 110)));
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new(&self.status).weak());
            if let Some(ws) = &self.ws {
                let errors = ws.diags.iter().filter(|d| d.is_error()).count();
                let warns = ws.diags.len() - errors;
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(format!("{} scene(s) | {:.1}s | {} fps", ws.episode.scenes.len(), ws.episode.duration, ws.episode.fps));
                    ui.separator();
                    let c = if errors > 0 { Color32::from_rgb(240, 110, 100) } else { Color32::from_rgb(120, 220, 150) };
                    ui.label(RichText::new(format!("{} error(s), {} warning(s)", errors, warns)).color(c));
                });
            }
        });
    }

    fn export_window(&mut self, ctx: &egui::Context) {
        let mut done: Option<Result<u64, String>> = None;
        if let Some(job) = &self.export {
            let finished = job.result.lock().unwrap().clone();
            if let Some(r) = finished {
                done = Some(r);
            } else {
                egui::Window::new("Rendering video").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                    let p = job.progress.load(Ordering::Relaxed);
                    let frac = p as f32 / job.total.max(1) as f32;
                    ui.label(format!("{}", job.out.display()));
                    ui.add(egui::ProgressBar::new(frac).show_percentage().desired_width(360.0));
                    ui.label(format!("frame {} / {}   {:.0}s elapsed", p, job.total, job.started.elapsed().as_secs_f32()));
                    if ui.button("Cancel").clicked() {
                        job.cancel.store(true, Ordering::Relaxed);
                    }
                });
            }
        }
        if let Some(r) = done {
            let out = self.export.take().map(|j| j.out).unwrap_or_default();
            match r {
                Ok(_) => {
                    self.status = format!("Rendered {}", out.display());
                    if self.startup.export.is_some() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                    #[cfg(windows)]
                    if self.startup.export.is_none()
                    {
                        let _ = std::process::Command::new("explorer").arg(format!("/select,{}", out.display())).spawn();
                    }
                }
                Err(e) if e == "cancelled" => self.status = "Render cancelled".into(),
                Err(e) => self.status = format!("Render failed: {}", e),
            }
        }
    }

    fn help_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_help;
        egui::Window::new("Oneiro Studio - shortcuts and tips").open(&mut open).show(ctx, |ui| {
            ui.label("Playback");
            ui.monospace("Space        play / pause\nLeft / Right step one frame\nHome / End   jump to start / end");
            ui.separator();
            ui.label("Viewport");
            ui.monospace("Click an actor to select it; drag it to move its starting position (pinned in the .onlock).\nFree camera: right-drag orbit, middle-drag pan, wheel zoom.");
            ui.separator();
            ui.label("Record motion");
            ui.monospace("Pick a character, press Record, drive it with WASD / arrows (Shift = run),\npress Stop and insert the generated lines into the script. Esc cancels.");
            ui.separator();
            ui.label("Editing");
            ui.monospace("Ctrl+S       save everything\nClick a diagnostic to jump to it. Click timeline blocks to jump to their script line.");
        });
        self.show_help = open;
    }

    fn take_window(&mut self, ctx: &egui::Context) {
        let mut action: Option<bool> = None;
        if let Some(take) = &self.take {
            egui::Window::new("Recorded motion").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
                if take.lines.is_empty() {
                    ui.label("The character barely moved - nothing to insert.");
                } else {
                    ui.label(format!("{} move(s) for {} will be inserted after line {}:", take.lines.len(), take.actor, take.insert_after_line));
                    for l in &take.lines {
                        ui.monospace(l);
                    }
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(!take.lines.is_empty(), egui::Button::new("Insert into script")).clicked() {
                        action = Some(true);
                    }
                    if ui.button("Discard").clicked() {
                        action = Some(false);
                    }
                });
            });
        }
        if let Some(insert) = action {
            if insert {
                self.apply_take();
            } else {
                self.take = None;
            }
        }
    }

    /// Inserts the recorded take into the script after the statement it was recorded against.
    pub fn apply_take(&mut self) {
        let Some(take) = self.take.take() else { return };
        if let Some(ws) = &mut self.ws {
            let idx = ws.script;
            let (text, cursor) = crate::textops::insert_lines_after(&ws.docs[idx].text, take.insert_after_line, &take.lines);
            ws.docs[idx].text = text;
            ws.active = idx;
            ws.recompile();
            ws.write_lock();
            self.cursor_char = cursor;
            self.right_tab = RightTab::Script;
            self.seek(take.seek_to);
            self.vp.invalidate();
            self.status = format!("Inserted {} line(s) for {}", take.lines.len(), take.actor);
        }
    }

    fn handle_startup_screenshot(&mut self, ctx: &egui::Context) {
        let Some(path) = self.startup.screenshot.clone() else { return };
        if self.frame_counter == self.startup.after_frames && !self.screenshot_requested {
            self.screenshot_requested = true;
            let exit = self.startup.exit_after;
            let c2 = ctx.clone();
            ctx.request_screenshot(move |img| {
                let (w, h) = (img.width() as u32, img.height() as u32);
                let mut bytes = Vec::with_capacity(img.pixels.len() * 4);
                for p in &img.pixels {
                    bytes.extend_from_slice(&[p.r(), p.g(), p.b(), p.a()]);
                }
                if let Some(size) = tiny_skia::IntSize::from_wh(w, h) {
                    if let Some(pm) = tiny_skia::Pixmap::from_vec(bytes, size) {
                        let _ = pm.save_png(&path);
                    }
                }
                if exit {
                    c2.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            });
        }
        ctx.request_repaint();
    }
}

pub fn fmt_time(t: f64) -> String {
    let t = t.max(0.0);
    let m = (t / 60.0).floor() as u32;
    let s = t - m as f64 * 60.0;
    format!("{:02}:{:05.2}", m, s)
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.frame_counter += 1;
        self.tick(&ctx);
        self.shortcuts(&ctx);
        self.poll_voice();
        self.sync_audio();

        egui::Panel::top("menu").show(ui, |ui| self.menu(ui));
        egui::Panel::top("transport").show(ui, |ui| self.transport(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::bottom("timeline").resizable(true).default_size(210.0).min_size(120.0).show(ui, |ui| self.ui_timeline(ui));
        egui::Panel::left("library").resizable(true).default_size(230.0).min_size(160.0).show(ui, |ui| self.ui_library(ui));
        egui::Panel::right("right").resizable(true).default_size(470.0).min_size(320.0).show(ui, |ui| self.ui_right(ui));
        egui::CentralPanel::default().show(ui, |ui| self.ui_viewport(ui));

        self.export_window(&ctx);
        self.take_window(&ctx);
        self.help_window(&ctx);
        self.run_startup_script(&ctx);
        self.run_ai_startup(&ctx);
        self.handle_startup_screenshot(&ctx);
    }
}

impl App {
    /// Scripted behaviour for `--demo-*` / `--export` startup switches (used for docs and tests).
    pub fn run_startup_script(&mut self, ctx: &egui::Context) {
        let f = self.frame_counter;
        if f == 5 {
            if let Some(text) = self.startup.demo_insert.clone() {
                self.right_tab = RightTab::Script;
                self.insert_request = Some(text);
            }
        }
        if f == 10 && self.startup.demo_record {
            self.sel_actor = Some("Blob".into());
            self.start_recording();
        }
        if f == 3 {
            if let Some(out) = self.startup.export.clone() {
                self.start_export(out, 640, 360, true);
            }
        }
        // Scripted driving while recording: forward, then right, then stop and insert.
        if self.startup.demo_record {
            if let Some(rec) = &self.recorder {
                if rec.clock > 3.4 {
                    self.stop_recording();
                    self.apply_take();
                }
            }
        }
        self.run_audio_startup(ctx);
        if self.startup.demo_record || self.startup.demo_insert.is_some() || self.startup.export.is_some() || self.startup.audio_selftest || self.startup.demo_generate.is_some() {
            ctx.request_repaint();
        }
    }
}

impl App {
    /// `--ai-request`: sends a request through the real AI tab and (optionally) applies the result,
    /// then schedules the screenshot shortly after the answer arrives.
    pub fn run_ai_startup(&mut self, ctx: &egui::Context) {
        let Some(req) = self.startup.ai_request.clone() else { return };
        match self.ai_phase {
            0 => {
                // hold the screenshot until the answer is in
                self.startup.after_frames = u64::MAX;
                if self.frame_counter >= 12 {
                    self.right_tab = RightTab::Ai;
                    if let Some(k) = self.startup.ai_provider.as_deref().and_then(oneiro_ai::ProviderKind::from_id) {
                        self.ai.cfg.kind = k;
                    }
                    self.ai_send(ctx, req);
                    self.ai_phase = 1;
                }
            }
            1 => {
                if !self.ai_busy() {
                    if self.startup.ai_apply {
self.ai_apply_latest_pending();
                    }
                    self.startup.after_frames = self.frame_counter + 20;
                    self.ai_phase = 2;
                }
            }
            _ => {}
        }
        ctx.request_repaint();
    }
}
