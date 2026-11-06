//! The Audio tab (sound studio): dialogue lines with record / import / generate, a sound library
//! with generators and waveforms, a mini mix timeline, synchronized playback and voice settings.

use crate::i18n::tr;
use crate::app::{fmt_time, App, RightTab};
use eframe::egui::{self, Color32, Pos2, Rect, RichText, Sense, Stroke};
use oneiro_audio::device::{Player, Recorder};
use oneiro_audio::import::{self, ImportKind};
use oneiro_audio::mix::MixReport;
use oneiro_audio::musicgen::{self, Mood, MusicSpec};
use oneiro_audio::{dsp, sfxgen, write_wav, Audio, SR};
use oneiro_core::AudioKind;
use oneiro_voice::{DialogueLine, GenOptions, LineReport, ProviderKind, VoiceConfig, VoiceInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Instant, SystemTime};

const VOICE_COL: Color32 = Color32::from_rgb(100, 170, 255);
const SFX_COL: Color32 = Color32::from_rgb(255, 190, 90);
const MUSIC_COL: Color32 = Color32::from_rgb(130, 220, 160);

#[derive(Clone, Debug)]
pub enum RecTarget {
    /// A dialogue line of the script (`rel` is the file the take is saved to).
    Line { who: String, line: u32, rel: String, had_audio: bool },
    /// A sound effect for the library.
    Sound { name: String },
}

pub struct Recording {
    pub rec: Recorder,
    pub target: RecTarget,
}

/// A finished take waiting for Keep / Discard.
pub struct Review {
    pub audio: Audio,
    pub target: RecTarget,
    pub loud: f32,
}

/// What to do when automatic voice generation ends.
#[derive(Clone, Debug)]
pub enum AfterVoices {
    Play,
    Export(PathBuf, u32, u32, bool),
}

pub enum VoiceMsg {
    Progress { done: usize, total: usize, label: String },
    Script { res: Result<(String, Vec<LineReport>), String>, base: String },
    Line { res: Result<f64, String>, line: u32, rel: String, had_audio: bool, who: String },
    Voices(ProviderKind, Result<Vec<VoiceInfo>, String>),
    Clip(Result<Audio, String>),
    Cloned { res: Result<String, String>, who: String },
}

pub struct VoiceJob {
    pub rx: Receiver<VoiceMsg>,
    pub started: Instant,
    pub done: usize,
    pub total: usize,
    pub label: String,
}

pub struct WaveInfo {
    key: (SystemTime, u64),
    pub peaks: Vec<f32>,
    pub dur: f64,
    pub loud: f32,
}

pub struct AudioState {
    pub player: Option<Player>,
    pub player_err: Option<String>,
    /// Generation of the episode whose mix the player currently holds.
    loaded_gen: Option<(u64, u64)>,
    mix: Option<((u64, u64), Arc<Audio>, MixReport)>,
    pub mix_epoch: u64,
    pub active: bool,
    pub mute: bool,
    pub recording: Option<Recording>,
    pub review: Option<Review>,
    pub cfg: VoiceConfig,
    pub session_key: String,
    pub key_msg: String,
    pub show_settings: bool,
    pub gen_provider: ProviderKind,
    /// Use each character's own voice (its provider and voice id) instead of one provider for everybody.
    pub own_voices: bool,
    /// Generate missing voices by itself before playing or exporting.
    pub auto_voices: bool,
    pub after: Option<AfterVoices>,
    pub clone_name: String,
    pub clone_files: Vec<PathBuf>,
    pub clone_for: String,
    pub ctx: Option<egui::Context>,
    pub job: Option<VoiceJob>,
    pub reports: Vec<LineReport>,
    pub msg: String,
    pub voices: Vec<VoiceInfo>,
    pub voices_for: Option<ProviderKind>,
    pub sfx_choice: usize,
    pub sfx_name: String,
    pub sfx_seed: u64,
    pub mood: Mood,
    pub bpm: f32,
    pub bars: usize,
    pub music_seed: u64,
    pub music_name: String,
    pub rec_name: String,
    pub import_music: bool,
    /// Chosen microphone (None: system default) and the names Windows reports.
    pub mic: Option<String>,
    pub mics: Vec<String>,
    waves: HashMap<PathBuf, WaveInfo>,
    lines_cache: Option<(u64, String, Vec<DialogueLine>)>,
    selftest_started: Option<Instant>,
}

impl Default for AudioState {
    fn default() -> Self {
        let cfg = VoiceConfig::load();
        AudioState {
            player: None,
            player_err: None,
            loaded_gen: None,
            mix: None,
            mix_epoch: 0,
            active: false,
            mute: false,
            recording: None,
            review: None,
            gen_provider: cfg.default_provider,
            own_voices: true,
            auto_voices: true,
            after: None,
            clone_name: String::new(),
            clone_files: vec![],
            clone_for: String::new(),
            ctx: None,
            cfg,
            session_key: String::new(),
            key_msg: String::new(),
            show_settings: false,
            job: None,
            reports: vec![],
            msg: String::new(),
            voices: vec![],
            voices_for: None,
            sfx_choice: 0,
            sfx_name: String::new(),
            sfx_seed: 1,
            mood: Mood::Cheerful,
            bpm: Mood::Cheerful.default_bpm(),
            bars: 8,
            music_seed: 1,
            music_name: String::new(),
            rec_name: String::new(),
            import_music: false,
            mic: None,
            mics: vec![],
            waves: HashMap::new(),
            lines_cache: None,
            selftest_started: None,
        }
    }
}

impl AudioState {
    /// Forget everything tied to the previous project.
    pub fn on_project_change(&mut self) {
        self.mix = None;
        self.loaded_gen = None;
        self.active = false;
        self.recording = None;
        self.review = None;
        self.reports.clear();
        self.lines_cache = None;
        self.msg.clear();
        if let Some(p) = &self.player {
            p.stop();
        }
    }

    /// The audio output, opened on first use.
    pub fn player(&mut self) -> Option<&Player> {
        if self.player.is_none() && self.player_err.is_none() {
            match Player::new() {
                Ok(p) => self.player = Some(p),
                Err(e) => self.player_err = Some(e),
            }
        }
        self.player.as_ref()
    }

    pub fn wave(&mut self, path: &Path) -> Option<&WaveInfo> {
        let meta = std::fs::metadata(path).ok()?;
        let key = (meta.modified().ok()?, meta.len());
        let stale = self.waves.get(path).map_or(true, |w| w.key != key);
        if stale {
            let a = import::read_any(path).ok()?;
            let info = WaveInfo { key, peaks: dsp::waveform(&a, 120), dur: a.duration(), loud: dsp::loudness_db(&a) };
            self.waves.insert(path.to_path_buf(), info);
        }
        self.waves.get(path)
    }
}

fn draw_wave(ui: &mut egui::Ui, peaks: &[f32], size: egui::Vec2, color: Color32) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, Color32::from_gray(26));
    let n = peaks.len().max(1);
    let mid = rect.center().y;
    for (i, v) in peaks.iter().enumerate() {
        let x = rect.left() + (i as f32 + 0.5) / n as f32 * rect.width();
        let h = (v.clamp(0.0, 1.0) * rect.height() * 0.5).max(0.5);
        p.line_segment([Pos2::new(x, mid - h), Pos2::new(x, mid + h)], Stroke::new(1.0, color));
    }
}

fn level_bar(ui: &mut egui::Ui, level: f32, width: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 10.0), Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, 2.0, Color32::from_gray(30));
    let l = level.clamp(0.0, 1.0).sqrt();
    let col = if level > 0.9 { Color32::from_rgb(240, 90, 80) } else if level > 0.6 { Color32::from_rgb(240, 200, 80) } else { Color32::from_rgb(110, 210, 140) };
    p.rect_filled(Rect::from_min_size(rect.min, egui::vec2(rect.width() * l, rect.height())), 2.0, col);
}

fn safe_name(s: &str) -> String {
    let n: String = s.trim().chars().map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' }).collect();
    n.trim_matches('_').to_string()
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        format!("{}...", s.chars().take(n).collect::<String>())
    }
}

enum Act {
    Play(PathBuf),
    RecordLine { who: String, line: u32, rel: Option<String> },
    ImportLine { who: String, line: u32, rel: Option<String> },
    GenerateLine { who: String, emotion: String, text: String, line: u32, rel: Option<String> },
    GenerateAll { force: bool },
    StopMic,
    CancelRecording,
    KeepReview,
    DiscardReview,
    ListenReview,
    Seek(f64),
    Insert(String),
}

impl App {
    // ------------------------------------------------------------------ mix + playback

    /// The final mix of the current episode (rebuilt only when the script or the audio changed).
    pub fn ensure_mix(&mut self) -> Option<Arc<Audio>> {
        let ws = self.ws.as_ref()?;
        let key = (ws.generation, self.audio.mix_epoch);
        if self.audio.mix.as_ref().map(|m| m.0) != Some(key) {
            let (samples, report) = ws.episode.render_audio();
            self.audio.mix = Some((key, Arc::new(Audio { sr: SR, samples }), report));
        }
        self.audio.mix.as_ref().map(|m| m.1.clone())
    }

    /// Call after any audio file changed on disk.
    pub fn audio_changed(&mut self) {
        self.audio.mix_epoch += 1;
        if let Some(ws) = &mut self.ws {
            ws.recompile();
        }
        self.vp.invalidate();
    }

    /// Keeps the speakers in step with the transport. Called once per frame.
    pub fn sync_audio(&mut self) {
        let want = self.playing && !self.audio.mute && self.recorder.is_none() && self.audio.recording.is_none();
        if want {
            if !self.audio.active {
                let Some(mix) = self.ensure_mix() else { return };
                let t = self.t;
                let key = self.ws.as_ref().map(|w| (w.generation, self.audio.mix_epoch));
                let loaded = self.audio.loaded_gen;
                if let Some(p) = self.audio.player() {
                    if loaded != key {
                        p.load(&mix);
                    }
                    p.play_from(t);
                    self.audio.loaded_gen = key;
                    self.audio.active = true;
                }
            } else if let Some(p) = self.audio.player.as_ref() {
                if (p.position() - self.t).abs() > 0.25 {
                    p.play_from(self.t);
                }
            }
        } else if self.audio.active {
            if let Some(p) = &self.audio.player {
                p.pause();
            }
            self.audio.active = false;
        }
    }

    fn play_clip(&mut self, a: &Audio) {
        self.playing = false;
        self.audio.active = false;
        self.audio.loaded_gen = None;
        if let Some(p) = self.audio.player() {
            p.play_clip(a);
        } else if let Some(e) = &self.audio.player_err {
            self.audio.msg = e.clone();
        }
    }

    fn play_file(&mut self, path: &Path) {
        match import::read_any(path) {
            Ok(a) => self.play_clip(&a),
            Err(e) => self.audio.msg = e,
        }
    }

    // ------------------------------------------------------------------ script edits

    /// Writes `audio "rel"` on the dialogue at `line` of the current script.
    pub fn set_line_audio(&mut self, line: u32, rel: &str) -> Result<(), String> {
        let ws = self.ws.as_mut().ok_or("no project open")?;
        let idx = ws.script;
        let text = ws.docs[idx].text.clone();
        let mut lines: Vec<String> = text.split_inclusive('\n').map(|s| s.to_string()).collect();
        let i = (line as usize).checked_sub(1).filter(|i| *i < lines.len()).ok_or("the dialogue line moved; try again")?;
        let (body, eol) = match lines[i].strip_suffix("\r\n") {
            Some(b) => (b.to_string(), "\r\n"),
            None => match lines[i].strip_suffix('\n') {
                Some(b) => (b.to_string(), "\n"),
                None => (lines[i].clone(), ""),
            },
        };
        let new = oneiro_voice::set_audio_clause(&body, rel).ok_or("that line is not a dialogue line any more")?;
        lines[i] = format!("{}{}", new, eol);
        ws.docs[idx].text = lines.concat();
        ws.recompile();
        self.vp.invalidate();
        Ok(())
    }

    // ------------------------------------------------------------------ recording

    fn start_mic(&mut self, target: RecTarget) {
        self.playing = false;
        match Recorder::start_named(self.audio.mic.as_deref()) {
            Ok(rec) => {
                self.audio.msg = format!("Recording from {}...", rec.device_name);
                self.audio.recording = Some(Recording { rec, target });
            }
            Err(e) => self.audio.msg = e,
        }
    }

    fn stop_mic(&mut self) {
        let Some(r) = self.audio.recording.take() else { return };
        let raw = r.rec.stop();
        let kind = match r.target {
            RecTarget::Line { .. } => ImportKind::Voice,
            RecTarget::Sound { .. } => ImportKind::Sfx,
        };
        if raw.duration() < 0.15 {
            self.audio.msg = "Nothing recorded (too short).".into();
            return;
        }
        let audio = import::process(&raw, kind);
        if audio.samples.is_empty() || dsp::peak(&audio.samples) < 1e-3 {
            self.audio.msg = "The recording is silent. Pick another microphone in the Audio tab (the Windows default can be a virtual one) or check Windows sound settings.".into();
            return;
        }
        let loud = dsp::loudness_db(&audio);
        self.audio.review = Some(Review { audio, target: r.target, loud });
    }

    fn keep_review(&mut self) {
        let Some(rv) = self.audio.review.take() else { return };
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else { return };
        match &rv.target {
            RecTarget::Line { rel, line, had_audio, who } => {
                let path = root.join(rel);
                if let Err(e) = write_wav(&path, &rv.audio.samples, SR) {
                    self.audio.msg = e;
                    return;
                }
                if !*had_audio {
                    if let Err(e) = self.set_line_audio(*line, rel) {
                        self.audio.msg = format!("Saved {} but could not link it: {}", rel, e);
                    }
                }
                self.audio_changed();
                self.status = format!("Saved the take for {} ({:.1}s). Save the script (Ctrl+S) to keep the link.", who, rv.audio.duration());
            }
            RecTarget::Sound { name } => {
                let path = root.join("library").join("sounds").join(format!("{}.wav", name));
                match write_wav(&path, &rv.audio.samples, SR) {
                    Ok(()) => {
                        self.audio_changed();
                        self.status = format!("Saved library/sounds/{}.wav. Use it with: sfx {}", name, name);
                    }
                    Err(e) => self.audio.msg = e,
                }
            }
        }
    }

    // ------------------------------------------------------------------ voices

    fn voice_job_spawn(&mut self, rx: Receiver<VoiceMsg>, label: &str) {
        self.audio.job = Some(VoiceJob { rx, started: Instant::now(), done: 0, total: 1, label: label.into() });
    }

    pub fn generate_all(&mut self, ctx: &egui::Context, force: bool, only: Vec<String>) {
        if self.audio.job.is_some() {
            return;
        }
        let Some(ws) = &self.ws else { return };
        let (root, project, base) = (ws.root.clone(), ws.project.clone(), ws.script_doc().text.clone());
        let cfg = self.audio.cfg.clone();
        let opts = GenOptions { force, provider: if self.audio.own_voices { None } else { Some(self.audio.gen_provider) }, only, session_key: Some(self.audio.session_key.clone()).filter(|k| !k.is_empty()) };
        let (tx, rx) = channel();
        self.voice_job_spawn(rx, "starting...");
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let tx2 = tx.clone();
            let c3 = c2.clone();
            let res = oneiro_voice::generate_script(&root, &project, &base, &cfg, &opts, &mut |done, total, label| {
                let _ = tx2.send(VoiceMsg::Progress { done, total, label: label.to_string() });
                c3.request_repaint();
                true
            });
            let _ = tx.send(VoiceMsg::Script { res, base });
            c2.request_repaint();
        });
    }

    fn generate_line(&mut self, ctx: &egui::Context, who: String, emotion: String, text: String, line: u32, rel: Option<String>) {
        if self.audio.job.is_some() {
            return;
        }
        let Some(ws) = &self.ws else { return };
        let (root, project) = (ws.root.clone(), ws.project.clone());
        let had_audio = rel.is_some();
        let rel = rel.unwrap_or_else(|| {
            let taken = oneiro_voice::dialogues(&ws.script_doc().text).into_iter().filter_map(|l| l.audio).collect();
            let mut taken: std::collections::HashSet<String> = taken;
            oneiro_voice::next_audio_path(&root, &who, &mut taken)
        });
        let cfg = self.audio.cfg.clone();
        let force = if self.audio.own_voices { None } else { Some(self.audio.gen_provider) };
        let key = Some(self.audio.session_key.clone()).filter(|k| !k.is_empty());
        let (tx, rx) = channel();
        self.voice_job_spawn(rx, &format!("{}: {}", who, short(&text, 40)));
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let (kind, req) = oneiro_voice::request_for(&project, &cfg, &who, &emotion, &text, force);
            let res = oneiro_voice::build(kind, &cfg, key.as_deref()).and_then(|p| oneiro_voice::synthesize_to(p.as_ref(), &req, &root.join(&rel)));
            let _ = tx.send(VoiceMsg::Line { res, line, rel, had_audio, who });
            c2.request_repaint();
        });
    }

    /// Builds a short sample with the character's own voice settings and plays it.
    pub fn preview_voice(&mut self, ctx: &egui::Context, def: &oneiro_core::CharacterDef) {
        if self.audio.job.is_some() {
            return;
        }
        let cfg = self.audio.cfg.clone();
        let key = Some(self.audio.session_key.clone()).filter(|k| !k.is_empty());
        let kind = match ProviderKind::from_id(&def.voice_provider) {
            Some(ProviderKind::File) | None => cfg.default_provider,
            Some(k) => k,
        };
        let req = oneiro_voice::VoiceRequest {
            text: format!("Hi, I'm {}. Where's my pie?", def.name),
            voice: def.voice_id.clone(),
            emotion: "neutral".into(),
            pitch: def.voice_pitch,
            speed: def.voice_speed,
            seed: 7,
        };
        let (tx, rx) = channel();
        self.voice_job_spawn(rx, &format!("voice of {}", def.name));
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let res = oneiro_voice::build(kind, &cfg, key.as_deref()).and_then(|p| p.synthesize(&req)).map(|a| import::process(&a, ImportKind::Voice));
            let _ = tx.send(VoiceMsg::Clip(res));
            c2.request_repaint();
        });
    }

    fn list_voices(&mut self, ctx: &egui::Context, kind: ProviderKind) {
        if self.audio.job.is_some() {
            return;
        }
        let cfg = self.audio.cfg.clone();
        let key = Some(self.audio.session_key.clone()).filter(|k| !k.is_empty());
        let (tx, rx) = channel();
        self.voice_job_spawn(rx, &format!("asking {} for its voices", kind.label()));
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let res = oneiro_voice::build(kind, &cfg, key.as_deref()).and_then(|p| p.voices());
            let _ = tx.send(VoiceMsg::Voices(kind, res));
            c2.request_repaint();
        });
    }

    /// Handles messages from the background voice jobs. Called once per frame.
    pub fn poll_voice(&mut self) {
        let mut finished: Vec<VoiceMsg> = vec![];
        let mut last_progress: Option<(usize, usize, String)> = None;
        if let Some(job) = &self.audio.job {
            while let Ok(m) = job.rx.try_recv() {
                match m {
                    VoiceMsg::Progress { done, total, label } => last_progress = Some((done, total, label)),
                    other => finished.push(other),
                }
            }
        }
        if let (Some(job), Some((d, t, l))) = (&mut self.audio.job, last_progress) {
            job.done = d;
            job.total = t.max(1);
            if !l.is_empty() {
                job.label = l;
            }
        }
        for m in finished {
            self.audio.job = None;
            match m {
                VoiceMsg::Script { res, base } => match res {
                    Err(e) => self.audio.msg = e,
                    Ok((text, reports)) => {
                        let made = reports.iter().filter(|r| !r.skipped && r.error.is_none()).count();
                        let failed = reports.iter().filter(|r| r.error.is_some()).count();
                        let current = self.ws.as_ref().map(|w| w.script_doc().text.clone()).unwrap_or_default();
                        if text != base {
                            if current == base {
                                if let Some(ws) = &mut self.ws {
                                    let i = ws.script;
                                    ws.docs[i].text = text;
                                    ws.recompile();
                                }
                            } else {
                                self.audio.msg = "The script changed while voices were being generated; the audio files were saved but not linked. Generate again to link them.".into();
                            }
                        }
                        if self.audio.msg.is_empty() || made > 0 {
                            self.audio.msg = format!("{} voice(s) generated, {} failed. Save the script (Ctrl+S) to keep the links.", made, failed);
                        }
                        self.audio.reports = reports;
                        self.audio_changed();
                    }
                },
                VoiceMsg::Line { res, line, rel, had_audio, who } => match res {
                    Err(e) => self.audio.msg = format!("{}: {}", who, e),
                    Ok(secs) => {
                        if !had_audio {
                            if let Err(e) = self.set_line_audio(line, &rel) {
                                self.audio.msg = e;
                            }
                        }
                        self.audio_changed();
                        self.audio.msg = format!("Generated {} ({:.1}s) for {}.", rel, secs, who);
                        let p = self.ws.as_ref().map(|w| w.root.join(&rel));
                        if let Some(p) = p {
                            self.play_file(&p);
                        }
                    }
                },
                VoiceMsg::Voices(kind, res) => match res {
                    Ok(v) => {
                        self.audio.msg = format!("{}: {} voice(s) found.", kind.label(), v.len());
                        self.audio.voices = v;
                        self.audio.voices_for = Some(kind);
                    }
                    Err(e) => self.audio.msg = e,
                },
                VoiceMsg::Cloned { res, who } => match res {
                    Err(e) => self.audio.msg = e,
                    Ok(id) => {
                        self.audio.msg = format!("Voice cloned (id {}).", id);
                        if !who.is_empty() {
                            match self.assign_voice(&who, &id) {
                                Ok(()) => self.audio.msg = format!("Voice cloned and assigned to {}. Press Play to hear it (lines without audio are generated).", who),
                                Err(e) => self.audio.msg = e,
                            }
                        }
                    }
                },
                VoiceMsg::Clip(res) => match res {
                    Ok(a) => self.play_clip(&a),
                    Err(e) => self.audio.msg = e,
                },
                VoiceMsg::Progress { .. } => {}
            }
        }
        if self.audio.job.is_none() && self.audio.after.is_some() {
            self.run_after_voices();
        }
    }

    // ------------------------------------------------------------------ UI

    pub fn ui_audio(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        let Some(root) = self.ws.as_ref().map(|w| w.root.clone()) else {
            ui.label(tr("No project open."));
            return;
        };
        self.poll_voice();

        // Lines of the current script (cached by script generation).
        let gen = self.ws.as_ref().map_or(0, |w| w.generation);
        if self.audio.lines_cache.as_ref().map_or(true, |c| c.0 != gen) {
            let text = self.ws.as_ref().map(|w| w.script_doc().text.clone()).unwrap_or_default();
            let lines = oneiro_voice::dialogues(&text);
            self.audio.lines_cache = Some((gen, text, lines));
        }
        let lines: Vec<DialogueLine> = self.audio.lines_cache.as_ref().map(|c| c.2.clone()).unwrap_or_default();

        let mut acts: Vec<Act> = vec![];
        self.ensure_mix();

        // ---- header: device, mute, mix summary
        ui.horizontal(|ui| {
            match (&self.audio.player, &self.audio.player_err) {
                (Some(p), _) => {
                    ui.label(RichText::new(format!("Output: {}", short(&p.device_name, 28))).weak().small());
                }
                (None, Some(e)) => {
                    ui.label(RichText::new(short(e, 60)).color(Color32::from_rgb(240, 120, 110)).small());
                }
                _ => {
                    ui.label(RichText::new(tr("Output opens on first playback")).weak().small());
                }
            }
            ui.checkbox(&mut self.audio.mute, tr("Mute"));
            if self.audio.mics.is_empty() {
                self.audio.mics = oneiro_audio::device::input_devices();
            }
            egui::ComboBox::from_id_salt("mic_choice").selected_text(format!("Mic: {}", short(self.audio.mic.as_deref().unwrap_or("system default"), 22))).width(190.0).show_ui(ui, |ui| {
                ui.selectable_value(&mut self.audio.mic, None, tr("system default"));
                for m in self.audio.mics.clone() {
                    ui.selectable_value(&mut self.audio.mic, Some(m.clone()), m);
                }
                if ui.small_button(tr("Refresh list")).clicked() {
                    self.audio.mics = oneiro_audio::device::input_devices();
                }
            });
            if ui.small_button(tr("Voice settings...")).clicked() {
                self.audio.show_settings = true;
            }
        });
        if let Some((_, _, rep)) = &self.audio.mix {
            let col = if rep.limited { Color32::from_rgb(240, 200, 80) } else { Color32::from_gray(150) };
            ui.label(RichText::new(format!("Mix: peak {:.1} dBFS, loudness {:.1} dBFS{}", rep.peak_db, rep.loudness_db, if rep.limited { " - limiter engaged, lower some volumes" } else { "" })).color(col).small());
        }
        if !self.audio.msg.is_empty() {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&self.audio.msg).small().color(Color32::from_rgb(230, 200, 120)));
                if ui.small_button("x").clicked() {
                    self.audio.msg.clear();
                }
            });
        }

        // ---- running job
        if let Some(job) = &self.audio.job {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(format!("{}  ({}/{}, {:.0}s)", short(&job.label, 50), job.done, job.total, job.started.elapsed().as_secs_f32()));
            });
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }

        // ---- take under review
        if let Some(rv) = &self.audio.review {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                let what = match &rv.target {
                    RecTarget::Line { who, .. } => format!("take for {}", who),
                    RecTarget::Sound { name } => format!("sound '{}'", name),
                };
                ui.label(RichText::new(format!("New {}: {:.1}s, {:.1} dBFS (cleaned and levelled)", what, rv.audio.duration(), rv.loud)).strong());
                let peaks = dsp::waveform(&rv.audio, 120);
                draw_wave(ui, &peaks, egui::vec2(ui.available_width().min(360.0), 36.0), VOICE_COL);
                ui.horizontal(|ui| {
                    if ui.button(tr("Listen")).clicked() {
                        acts.push(Act::ListenReview);
                    }
                    if ui.button(RichText::new(tr("Keep")).strong()).clicked() {
                        acts.push(Act::KeepReview);
                    }
                    if ui.button(tr("Discard")).clicked() {
                        acts.push(Act::DiscardReview);
                    }
                });
            });
        }

        // ---- active recording
        if let Some(r) = &self.audio.recording {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("REC {:.1}s", r.rec.seconds())).color(Color32::from_rgb(240, 90, 80)).strong());
                    level_bar(ui, r.rec.level(), 140.0);
                    if ui.button(RichText::new(tr("Stop")).strong()).clicked() {
                        acts.push(Act::StopMic);
                    }
                    if ui.button(tr("Cancel")).clicked() {
                        acts.push(Act::CancelRecording);
                    }
                });
                ui.label(RichText::new(match &r.target {
                    RecTarget::Line { who, .. } => format!("Speak the line for {}. Silence at both ends is trimmed automatically.", who),
                    RecTarget::Sound { name } => format!("Recording sound '{}'.", name),
                }).weak().small());
            });
            ctx.request_repaint();
        }

        let busy = self.audio.job.is_some() || self.audio.recording.is_some();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            // ================================================================ mix timeline
            egui::CollapsingHeader::new(RichText::new(tr("Mix timeline")).strong()).default_open(true).show(ui, |ui| {
                let Some(ep) = self.ws.as_ref().map(|w| w.episode.clone()) else { return };
                let dur = ep.duration.max(0.1);
                let lanes = [("voice", VOICE_COL, AudioKind::Voice), ("sfx", SFX_COL, AudioKind::Sfx), ("music", MUSIC_COL, AudioKind::Music)];
                let (w, lane_h) = (ui.available_width().max(200.0), 22.0);
                let (resp, painter) = ui.allocate_painter(egui::vec2(w, lane_h * 3.0 + 16.0), Sense::click_and_drag());
                let rect = resp.rect;
                painter.rect_filled(rect, 3.0, Color32::from_gray(22));
                let label_w = 40.0;
                let x_of = |t: f64| rect.left() + label_w + ((t / dur) as f32).clamp(0.0, 1.0) * (rect.width() - label_w - 4.0);
                for (i, (name, col, kind)) in lanes.iter().enumerate() {
                    let top = rect.top() + 14.0 + i as f32 * lane_h;
                    painter.text(Pos2::new(rect.left() + 4.0, top + lane_h / 2.0), egui::Align2::LEFT_CENTER, *name, egui::FontId::proportional(10.0), Color32::from_gray(150));
                    for e in ep.audio.iter().filter(|e| e.kind == *kind) {
                        let len = self.audio.wave(&e.path).map(|w| w.dur).unwrap_or(0.5);
                        let end = if e.looped { dur } else { e.start + len };
                        let end = e.stop.map_or(end, |s| end.min(s)).min(dur);
                        let r = Rect::from_min_max(Pos2::new(x_of(e.start), top + 3.0), Pos2::new(x_of(end).max(x_of(e.start) + 3.0), top + lane_h - 3.0));
                        painter.rect_filled(r, 2.0, col.gamma_multiply(0.5 + 0.5 * e.gain.min(1.0)));
                    }
                }
                for k in 0..=4 {
                    let t = dur * k as f64 / 4.0;
                    painter.text(Pos2::new(x_of(t), rect.top() + 1.0), if k == 4 { egui::Align2::RIGHT_TOP } else if k == 0 { egui::Align2::LEFT_TOP } else { egui::Align2::CENTER_TOP }, fmt_time(t), egui::FontId::monospace(9.0), Color32::from_gray(110));
                }
                let px = x_of(self.t);
                painter.line_segment([Pos2::new(px, rect.top() + 12.0), Pos2::new(px, rect.bottom())], Stroke::new(1.5, Color32::from_rgb(255, 80, 90)));
                if resp.clicked() || resp.dragged() {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let t = ((pos.x - rect.left() - label_w) / (rect.width() - label_w - 4.0)) as f64 * dur;
                        acts.push(Act::Seek(t.clamp(0.0, dur)));
                    }
                }
                ui.label(RichText::new(format!("Music ducks {:.0} dB under speech (duck_db in oneiro.toml). Statements: music NAME [volume N] [fade_in Ns] [fade_out Ns] [once], music stop [fade_out Ns], sfx NAME [volume N].", -ep.duck_db.abs())).weak().small());
            });

            // ================================================================ dialogue
            egui::CollapsingHeader::new(RichText::new(format!("Dialogue ({} lines)", lines.len())).strong()).default_open(true).show(ui, |ui| {
                ui.checkbox(&mut self.audio.auto_voices, tr("Generate missing voices automatically when I press Play or Render")).on_hover_text(tr("Uses the voice each character has in the Character builder (its provider and voice)."));
                ui.checkbox(&mut self.audio.own_voices, tr("Use each character's own voice"));
                ui.add_enabled_ui(!self.audio.own_voices, |ui| ui.horizontal(|ui| {
                    ui.label(tr("Voices from"));
                    egui::ComboBox::from_id_salt("gen_provider").selected_text(self.audio.gen_provider.label()).width(210.0).show_ui(ui, |ui| {
                        for k in ProviderKind::ALL {
                            if k.synthesizes() {
                                ui.selectable_value(&mut self.audio.gen_provider, k, k.label());
                            }
                        }
                    });
                }));
                let missing = lines.iter().filter(|l| l.audio.as_ref().map_or(true, |p| !root.join(p).exists())).count();
                ui.horizontal(|ui| {
                    if ui.add_enabled(!busy && missing > 0, egui::Button::new(format!("Generate {} missing", missing))).clicked() {
                        acts.push(Act::GenerateAll { force: false });
                    }
                    if ui.add_enabled(!busy && !lines.is_empty(), egui::Button::new(tr("Regenerate all"))).on_hover_text(tr("Replaces every spoken line with a new synthesized take")).clicked() {
                        acts.push(Act::GenerateAll { force: true });
                    }
                });
                if lines.is_empty() {
                    ui.label(RichText::new(tr("The script has no dialogue yet.")).weak());
                }
                for l in &lines {
                    let rel = l.audio.clone();
                    let path = rel.as_ref().map(|p| root.join(p));
                    let info = path.as_ref().and_then(|p| self.audio.wave(p)).map(|w| (w.dur, w.loud, w.peaks.clone()));
                    ui.separator();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{} ({})", l.who, l.emotion)).strong());
                        ui.label(RichText::new(format!("line {}", l.line)).weak().small());
                        match &info {
                            Some((d, loud, _)) => {
                                ui.label(RichText::new(format!("{:.1}s  {:.0} dBFS", d, loud)).small().color(Color32::from_rgb(120, 220, 150)));
                            }
                            None if rel.is_some() => {
                                ui.label(RichText::new(tr("file missing")).small().color(Color32::from_rgb(240, 120, 110)));
                            }
                            None => {
                                ui.label(RichText::new(tr("no audio (timing estimated)")).small().color(Color32::from_rgb(240, 200, 80)));
                            }
                        }
                    });
                    ui.label(RichText::new(format!("\"{}\"", short(&l.text, 90))).italics());
                    if let Some((_, _, peaks)) = &info {
                        draw_wave(ui, peaks, egui::vec2(ui.available_width().min(380.0), 22.0), VOICE_COL);
                    }
                    ui.horizontal(|ui| {
                        if ui.add_enabled(info.is_some(), egui::Button::new(tr("Play")).small()).clicked() {
                            if let Some(p) = &path {
                                acts.push(Act::Play(p.clone()));
                            }
                        }
                        if ui.add_enabled(!busy, egui::Button::new(tr("Record")).small()).on_hover_text(tr("Record this line with your microphone")).clicked() {
                            acts.push(Act::RecordLine { who: l.who.clone(), line: l.line, rel: rel.clone() });
                        }
                        if ui.add_enabled(!busy, egui::Button::new(tr("Import...")).small()).on_hover_text(tr("Use an existing audio file (wav, mp3, ogg, flac, m4a...)")).clicked() {
                            acts.push(Act::ImportLine { who: l.who.clone(), line: l.line, rel: rel.clone() });
                        }
                        if ui.add_enabled(!busy, egui::Button::new(tr("Generate")).small()).on_hover_text(tr("Synthesize this line with the chosen voice provider")).clicked() {
                            acts.push(Act::GenerateLine { who: l.who.clone(), emotion: l.emotion.clone(), text: l.text.clone(), line: l.line, rel: rel.clone() });
                        }
                    });
                    if let Some(r) = self.audio.reports.iter().find(|r| r.line == l.line && r.error.is_some()) {
                        ui.label(RichText::new(short(r.error.as_deref().unwrap_or(""), 120)).small().color(Color32::from_rgb(240, 120, 110)));
                    }
                }
            });

            // ================================================================ library
            egui::CollapsingHeader::new(RichText::new(tr("Sounds and music")).strong()).default_open(true).show(ui, |ui| {
                let mut sounds: Vec<(PathBuf, bool)> = vec![];
                for (sub, is_music) in [("library/sounds", false), ("library/music", true)] {
                    if let Ok(rd) = std::fs::read_dir(root.join(sub)) {
                        let mut v: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.extension().map_or(false, |x| x == "wav")).collect();
                        v.sort();
                        sounds.extend(v.into_iter().map(|p| (p, is_music)));
                    }
                }
                if sounds.is_empty() {
                    ui.label(RichText::new(tr("No sounds yet. Create some below.")).weak());
                }
                for (p, is_music) in &sounds {
                    let name = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
                    let info = self.audio.wave(p).map(|w| (w.dur, w.peaks.clone()));
                    ui.horizontal(|ui| {
                        if ui.small_button(tr("Play")).clicked() {
                            acts.push(Act::Play(p.clone()));
                        }
                        ui.label(RichText::new(&name).strong());
                        ui.label(RichText::new(if *is_music { "music" } else { "sfx" }).small().color(if *is_music { MUSIC_COL } else { SFX_COL }));
                        if let Some((d, peaks)) = &info {
                            ui.label(RichText::new(format!("{:.1}s", d)).weak().small());
                            draw_wave(ui, peaks, egui::vec2(90.0, 16.0), if *is_music { MUSIC_COL } else { SFX_COL });
                        }
                        if ui.small_button(tr("Insert")).on_hover_text(tr("Insert it in the script at the cursor")).clicked() {
                            acts.push(Act::Insert(format!("{} {}", if *is_music { "music" } else { "sfx" }, name)));
                        }
                    });
                }
                ui.separator();

                // ---- sound effect generator
                ui.label(RichText::new(tr("Create a sound effect")).strong());
                let presets = sfxgen::PRESETS;
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("sfx_preset").selected_text(format!("{} - {}", presets[self.audio.sfx_choice].0, presets[self.audio.sfx_choice].1)).width(230.0).show_ui(ui, |ui| {
                        for (i, (n, d)) in presets.iter().enumerate() {
                            ui.selectable_value(&mut self.audio.sfx_choice, i, format!("{} - {}", n, d));
                        }
                    });
                    ui.label(tr("variation"));
                    ui.add(egui::DragValue::new(&mut self.audio.sfx_seed).range(1..=9999));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("name"));
                    ui.add(egui::TextEdit::singleline(&mut self.audio.sfx_name).hint_text(presets[self.audio.sfx_choice].0).desired_width(120.0));
                    if ui.button(tr("Preview")).clicked() {
                        if let Some(a) = sfxgen::generate(presets[self.audio.sfx_choice].0, self.audio.sfx_seed) {
                            self.play_clip(&a);
                        }
                    }
                    if ui.button(tr("Add to library")).clicked() {
                        let preset = presets[self.audio.sfx_choice].0;
                        let name = if self.audio.sfx_name.trim().is_empty() { preset.to_string() } else { safe_name(&self.audio.sfx_name) };
                        if let Some(a) = sfxgen::generate(preset, self.audio.sfx_seed) {
                            let path = root.join("library").join("sounds").join(format!("{}.wav", name));
                            match write_wav(&path, &a.samples, SR) {
                                Ok(()) => {
                                    self.audio.msg = format!("Added library/sounds/{}.wav. Use it with: sfx {}", name, name);
                                    self.audio_changed();
                                }
                                Err(e) => self.audio.msg = e,
                            }
                        }
                    }
                });
                ui.add_space(4.0);

                // ---- music generator
                ui.label(RichText::new(tr("Create background music")).strong());
                ui.horizontal(|ui| {
                    egui::ComboBox::from_id_salt("music_mood").selected_text(self.audio.mood.id()).width(90.0).show_ui(ui, |ui| {
                        for m in Mood::ALL {
                            if ui.selectable_value(&mut self.audio.mood, m, m.id()).clicked() {
                                self.audio.bpm = m.default_bpm();
                            }
                        }
                    });
                    ui.label(tr("bpm"));
                    ui.add(egui::DragValue::new(&mut self.audio.bpm).range(40.0..=200.0).speed(0.5));
                    ui.label(tr("bars"));
                    ui.add(egui::DragValue::new(&mut self.audio.bars).range(1..=32));
                    ui.label(tr("seed"));
                    ui.add(egui::DragValue::new(&mut self.audio.music_seed).range(1..=9999));
                });
                ui.horizontal(|ui| {
                    ui.label(tr("name"));
                    ui.add(egui::TextEdit::singleline(&mut self.audio.music_name).hint_text(self.audio.mood.id()).desired_width(120.0));
                    let spec = MusicSpec { mood: self.audio.mood, bpm: self.audio.bpm, bars: self.audio.bars, seed: self.audio.music_seed, key: 0 };
                    if ui.button(tr("Preview")).clicked() {
                        let a = musicgen::generate(&spec);
                        self.play_clip(&a);
                    }
                    if ui.button(tr("Add to library")).clicked() {
                        let name = if self.audio.music_name.trim().is_empty() { format!("{}_{}", spec.mood.id(), spec.seed) } else { safe_name(&self.audio.music_name) };
                        let a = musicgen::generate(&spec);
                        let path = root.join("library").join("music").join(format!("{}.wav", name));
                        match write_wav(&path, &a.samples, SR) {
                            Ok(()) => {
                                self.audio.msg = format!("Added library/music/{}.wav ({:.0}s loop). Use it with: music {}", name, a.duration(), name);
                                self.audio_changed();
                            }
                            Err(e) => self.audio.msg = e,
                        }
                    }
                });
                ui.add_space(4.0);

                // ---- import / record
                ui.label(RichText::new(tr("Import or record")).strong());
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.audio.import_music, tr("it is music"));
                    if ui.button(tr("Import file...")).clicked() {
                        if let Some(f) = rfd::FileDialog::new().set_title("Import audio").add_filter("Audio", &["wav", "mp3", "ogg", "flac", "m4a", "aac", "opus", "wma"]).pick_file() {
                            let stem = f.file_stem().map(|s| safe_name(&s.to_string_lossy())).unwrap_or_else(|| "imported".into());
                            let (kind, folder) = if self.audio.import_music { (ImportKind::Music, "library/music") } else { (ImportKind::Sfx, "library/sounds") };
                            match import::import_into(&root, &f, &format!("{}/{}", folder, stem), kind) {
                                Ok(p) => {
                                    self.audio.msg = format!("Imported {} as '{}'.", p.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default(), stem);
                                    self.audio_changed();
                                }
                                Err(e) => self.audio.msg = e,
                            }
                        }
                    }
                });
                ui.horizontal(|ui| {
                    ui.label(tr("record a sound named"));
                    ui.add(egui::TextEdit::singleline(&mut self.audio.rec_name).hint_text(tr("my_sound")).desired_width(110.0));
                    let ok = !safe_name(&self.audio.rec_name).is_empty();
                    if ui.add_enabled(!busy && ok, egui::Button::new(tr("Record"))).clicked() {
                        let name = safe_name(&self.audio.rec_name);
                        acts.push(Act::RecordLine { who: String::new(), line: 0, rel: Some(format!("sound:{}", name)) });
                    }
                });
            });
        });

        // ---- apply actions
        for a in acts {
            match a {
                Act::Play(p) => self.play_file(&p),
                Act::RecordLine { who, line, rel } => {
                    if let Some(name) = rel.as_deref().and_then(|r| r.strip_prefix("sound:")) {
                        self.start_mic(RecTarget::Sound { name: name.to_string() });
                    } else {
                        let had = rel.is_some();
                        let rel = rel.unwrap_or_else(|| self.fresh_audio_path(&who));
                        self.start_mic(RecTarget::Line { who, line, rel, had_audio: had });
                    }
                }
                Act::ImportLine { who, line, rel } => {
                    if let Some(f) = rfd::FileDialog::new().set_title(format!("Audio for {}", who)).add_filter("Audio", &["wav", "mp3", "ogg", "flac", "m4a", "aac", "opus", "wma"]).pick_file() {
                        let had = rel.is_some();
                        let rel = rel.unwrap_or_else(|| self.fresh_audio_path(&who));
                        match import::import_into(&root, &f, &rel, ImportKind::Voice) {
                            Ok(_) => {
                                if !had {
                                    if let Err(e) = self.set_line_audio(line, &rel) {
                                        self.audio.msg = e;
                                    }
                                }
                                self.audio_changed();
                                self.audio.msg = format!("Imported {} for {}. Save the script (Ctrl+S) to keep the link.", rel, who);
                            }
                            Err(e) => self.audio.msg = e,
                        }
                    }
                }
                Act::GenerateLine { who, emotion, text, line, rel } => self.generate_line(&ctx, who, emotion, text, line, rel),
                Act::GenerateAll { force } => self.generate_all(&ctx, force, vec![]),
                Act::StopMic => self.stop_mic(),
                Act::CancelRecording => {
                    self.audio.recording = None;
                    self.audio.msg = "Recording cancelled.".into();
                }
                Act::KeepReview => self.keep_review(),
                Act::DiscardReview => self.audio.review = None,
                Act::ListenReview => {
                    if let Some(rv) = &self.audio.review {
                        let a = rv.audio.clone();
                        self.play_clip(&a);
                    }
                }
                Act::Seek(t) => {
                    self.seek(t);
                    if self.playing {
                        self.audio.active = false;
                    }
                }
                Act::Insert(s) => {
                    self.insert_request = Some(s);
                    self.right_tab = RightTab::Script;
                }
            }
        }

        self.voice_settings_window(&ctx);
    }

    /// Scripted checks for `--audio-selftest` and `--demo-generate`.
    pub fn run_audio_startup(&mut self, ctx: &egui::Context) {
        let f = self.frame_counter;
        if let Some(name) = self.startup.demo_generate.clone() {
            if f == 5 {
                self.audio.gen_provider = ProviderKind::from_id(&name).unwrap_or(ProviderKind::Demo);
                self.generate_all(ctx, false, vec![]);
            }
            if f > 5 && self.audio.job.is_none() {
                let missing = oneiro_voice::dialogues(&self.ws.as_ref().map(|w| w.script_doc().text.clone()).unwrap_or_default()).iter().filter(|l| l.audio.is_none()).count();
                eprintln!("DEMO-GENERATE: {} | lines still without audio: {} | errors: {}", self.audio.msg, missing, self.ws.as_ref().map_or(0, |w| w.diags.iter().filter(|d| d.is_error()).count()));
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.startup.audio_selftest {
            if f == 5 {
                self.toggle_play();
                self.audio.selftest_started = Some(Instant::now());
            }
            if let Some(t0) = self.audio.selftest_started {
                if t0.elapsed().as_secs_f64() > 1.6 {
                    let (pos, playing) = self.audio.player.as_ref().map_or((-1.0, false), |p| (p.position(), p.is_playing()));
                    eprintln!("AUDIO-SELFTEST: player_position={:.3} playhead={:.3} drift={:.3} device_playing={} active={} err={:?}", pos, self.t, (pos - self.t).abs(), playing, self.audio.active, self.audio.player_err);
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    self.audio.selftest_started = None;
                }
            }
        }
    }

    fn fresh_audio_path(&self, who: &str) -> String {
        let root = self.ws.as_ref().map(|w| w.root.clone()).unwrap_or_default();
        let mut taken: std::collections::HashSet<String> = self.ws.as_ref().map(|w| oneiro_voice::dialogues(&w.script_doc().text).into_iter().filter_map(|l| l.audio).collect()).unwrap_or_default();
        oneiro_voice::next_audio_path(&root, if who.is_empty() { "take" } else { who }, &mut taken)
    }

    fn voice_settings_window(&mut self, ctx: &egui::Context) {
        if !self.audio.show_settings {
            return;
        }
        let mut open = true;
        let mut list: Option<ProviderKind> = None;
        let mut clone = false;
        let mut save = false;
        egui::Window::new(tr("Voice settings")).open(&mut open).default_width(430.0).show(ctx, |ui| {
            ui.label(RichText::new(tr("API keys are never saved to disk: only the name of the environment variable is stored.")).weak().small());
            ui.horizontal(|ui| {
                ui.label(tr("Default engine"));
                egui::ComboBox::from_id_salt("default_provider").selected_text(self.audio.cfg.default_provider.label()).show_ui(ui, |ui| {
                    for k in ProviderKind::ALL {
                        ui.selectable_value(&mut self.audio.cfg.default_provider, k, k.label());
                    }
                });
            });
            ui.label(RichText::new("Characters set their own engine and voice in the Character builder (voice section) or in `voice { provider: elevenlabs, voice: \"id\" }`.").weak().small());
            ui.separator();
            ui.label(RichText::new(tr("Windows speech (offline)")).strong());
            ui.horizontal(|ui| {
                ui.label(tr("default voice"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.sapi_voice).hint_text(tr("system default")).desired_width(220.0));
                if ui.button(tr("List voices")).clicked() {
                    list = Some(ProviderKind::Sapi);
                }
            });
            ui.separator();
            ui.label(RichText::new(tr("ElevenLabs")).strong());
            egui::Grid::new("el_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("key variable"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.elevenlabs_key_env).hint_text(tr("ELEVENLABS_API_KEY")));
                ui.end_row();
                ui.label(tr("API key"));
                crate::keystore::key_row(ui, &mut self.audio.session_key, "elevenlabs", &mut self.audio.key_msg);
                ui.end_row();
                ui.label(tr("model"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.elevenlabs_model));
                ui.end_row();
            });
            if ui.button(tr("List my ElevenLabs voices")).clicked() {
                list = Some(ProviderKind::ElevenLabs);
            }
            ui.collapsing(tr("Clone a voice (ElevenLabs)"), |ui| {
                ui.label(RichText::new(tr("Upload clean recordings of ONE person who agreed to it (about a minute or more; WAV, MP3...). Needs an ElevenLabs plan with voice cloning.")).weak().small());
                ui.horizontal(|ui| {
                    ui.label(tr("name"));
                    ui.add(egui::TextEdit::singleline(&mut self.audio.clone_name).desired_width(160.0));
                });
                ui.horizontal(|ui| {
                    if ui.button(tr("Choose recordings...")).clicked() {
                        if let Some(p) = rfd::FileDialog::new().set_title("Recordings of the voice").add_filter("audio", &["wav", "mp3", "m4a", "ogg", "flac"]).pick_files() {
                            self.audio.clone_files = p;
                        }
                    }
                    ui.label(format!("{} file(s)", self.audio.clone_files.len()));
                });
                let who: Vec<String> = self.ws.as_ref().map(|w| w.project.characters.keys().cloned().collect()).unwrap_or_default();
                egui::ComboBox::from_id_salt("clone_for").selected_text(if self.audio.clone_for.is_empty() { "(only create the voice)".to_string() } else { self.audio.clone_for.clone() }).show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.audio.clone_for, String::new(), "(only create the voice)");
                    for n in &who {
                        ui.selectable_value(&mut self.audio.clone_for, n.clone(), format!("use it for {}", n));
                    }
                });
                if ui.add_enabled(self.audio.job.is_none() && !self.audio.clone_files.is_empty() && !self.audio.clone_name.trim().is_empty(), egui::Button::new(tr("Clone voice"))).clicked() {
                    clone = true;
                }
            });
            ui.separator();
            ui.label(RichText::new(tr("OpenAI-compatible TTS / local server")).strong());
            egui::Grid::new("oa_grid").num_columns(2).show(ui, |ui| {
                ui.label(tr("base URL"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.openai_base_url).hint_text(tr("http://localhost:8880/v1")));
                ui.end_row();
                ui.label(tr("model"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.openai_model));
                ui.end_row();
                ui.label(tr("default voice"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.openai_voice));
                ui.end_row();
                ui.label(tr("key variable"));
                ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.openai_key_env).hint_text(tr("(none for local servers)")));
                ui.end_row();
            });
            ui.separator();
            ui.label(RichText::new(tr("Custom command (Piper, espeak-ng...)")).strong());
            ui.add(egui::TextEdit::singleline(&mut self.audio.cfg.command).hint_text("piper --model voice.onnx --output_file {out}").desired_width(f32::INFINITY));
            ui.label(RichText::new("Placeholders: {text} {out} {voice} {speed}. Without {text} the line is sent on stdin.").weak().small());
            ui.separator();
            if self.audio.voices_for.is_some() && !self.audio.voices.is_empty() {
                ui.label(RichText::new(format!("Voices ({})", self.audio.voices_for.map_or("", |k| k.id()))).strong());
                egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                    for v in &self.audio.voices {
                        ui.horizontal(|ui| {
                            if ui.small_button(tr("Copy id")).clicked() {
                                ui.ctx().copy_text(v.id.clone());
                            }
                            ui.label(RichText::new(&v.name).strong());
                            ui.label(RichText::new(short(&v.detail, 40)).weak().small());
                        });
                    }
                });
                ui.separator();
            }
            if ui.button(RichText::new(tr("Save settings")).strong()).clicked() {
                save = true;
            }
        });
        if let Some(k) = list {
            let ctx2 = ctx.clone();
            self.list_voices(&ctx2, k);
        }
        if clone {
            self.start_clone(ctx);
        }
        if save {
            match self.audio.cfg.save() {
                Ok(()) => self.audio.msg = format!("Saved {}", oneiro_voice::config::config_path().display()),
                Err(e) => self.audio.msg = e,
            }
        }
        self.audio.show_settings = open;
    }
}

impl App {
    /// Characters that have lines without audio and a real voice configured (not `file`).
    pub fn auto_voice_targets(&self) -> Vec<String> {
        let Some(ws) = &self.ws else { return vec![] };
        let mut out: Vec<String> = vec![];
        for l in oneiro_voice::dialogues(&ws.script_doc().text) {
            if l.audio.as_ref().map_or(false, |p| ws.root.join(p).exists()) {
                continue;
            }
            let real = ws.project.characters.get(&l.who).and_then(|c| ProviderKind::from_id(&c.voice_provider)).map_or(false, |k| k != ProviderKind::File && k.synthesizes());
            if real && !out.contains(&l.who) {
                out.push(l.who);
            }
        }
        out
    }

    /// If automatic voices are on and some lines have a voice to generate, starts that and remembers what
    /// to do afterwards. Returns true when the caller must wait.
    pub fn ensure_voices(&mut self, after: AfterVoices) -> bool {
        if !self.audio.auto_voices || self.audio.job.is_some() {
            return false;
        }
        let who = self.auto_voice_targets();
        let Some(ctx) = self.audio.ctx.clone() else { return false };
        if who.is_empty() {
            return false;
        }
        self.status = format!("Generating the voices of {}...", who.join(", "));
        self.audio.after = Some(after);
        self.generate_all(&ctx, false, who);
        if self.audio.job.is_none() {
            self.audio.after = None;
            return false;
        }
        true
    }

    /// Runs what was waiting for the voices.
    pub fn run_after_voices(&mut self) {
        let prev = self.audio.auto_voices;
        self.audio.auto_voices = false; // no second round if some line could not be generated
        match self.audio.after.take() {
            Some(AfterVoices::Play) => self.toggle_play(),
            Some(AfterVoices::Export(out, w, h, preview)) => self.start_export(out, w, h, preview),
            None => {}
        }
        self.audio.auto_voices = prev;
    }
}

impl App {
    fn start_clone(&mut self, ctx: &egui::Context) {
        let (cfg, name, files, who) = (self.audio.cfg.clone(), self.audio.clone_name.clone(), self.audio.clone_files.clone(), self.audio.clone_for.clone());
        let key = Some(self.audio.session_key.clone()).filter(|k| !k.is_empty());
        let (tx, rx) = channel();
        self.voice_job_spawn(rx, "cloning the voice...");
        let c2 = ctx.clone();
        std::thread::spawn(move || {
            let res = oneiro_voice::providers::clone_voice(&cfg, key.as_deref(), &name, &files);
            let _ = tx.send(VoiceMsg::Cloned { res, who });
            c2.request_repaint();
        });
    }

    /// Gives a character the ElevenLabs voice `id` (rewrites its .onchar).
    fn assign_voice(&mut self, who: &str, id: &str) -> Result<(), String> {
        let ws = self.ws.as_mut().ok_or("no project open")?;
        let mut def = ws.project.characters.get(who).cloned().ok_or_else(|| format!("no character called {}", who))?;
        def.voice_provider = "elevenlabs".into();
        def.voice_id = id.to_string();
        let path = ws.root.join("library").join("characters").join(format!("{}.onchar", who));
        std::fs::write(&path, def.to_source()).map_err(|e| format!("{}: {}", path.display(), e))?;
        ws.reload_project();
        Ok(())
    }
}
