//! Script compiler: turns a parsed script plus the project library into an `Episode`
//! (scenes with actor timelines and camera segments), reporting diagnostics along the way.

use crate::camera::{self, CameraPose, CameraSeg, SolveEnv};
use crate::catalog::*;
use crate::defs::*;
use crate::lock::Lock;
use crate::math::*;
use crate::project::Project;
use crate::timeline::*;
use oneiro_audio::{lipsync, read_wav};
use oneiro_lang::suggest::did_you_mean;
use oneiro_lang::{self as ast, Diagnostic, Pos};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f64::consts::PI;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// ------------------------------------------------------------------ output model

#[derive(Clone, Debug)]
pub struct AudioEvent {
    pub kind: AudioKind,
    pub start: f64,
    pub path: PathBuf,
    pub gain: f32,
    pub fade_in: f64,
    pub fade_out: f64,
    /// Cut time (after which nothing plays); `None` plays to the end (or loops until the episode ends).
    pub stop: Option<f64>,
    pub looped: bool,
}

pub use oneiro_audio::mix::Kind as AudioKind;

/// Story state accumulated by `give`, `wound`, `relation`, `remember` and `X.field = value`.
#[derive(Clone, Debug, Default)]
pub struct StateLog {
    pub fields: BTreeMap<String, String>,
    pub inventory: BTreeMap<String, Vec<String>>,
    pub wounds: BTreeMap<String, Vec<(String, String)>>,
    pub relations: Vec<(String, String, String)>,
    pub memories: BTreeMap<String, Vec<String>>,
}

impl StateLog {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
            && self.inventory.is_empty()
            && self.wounds.is_empty()
            && self.relations.is_empty()
            && self.memories.is_empty()
    }

    pub fn render(&self) -> String {
        let mut s = String::from("oneiro 0.1\n# State at the end of this episode.\nstate {\n");
        for (k, v) in &self.fields {
            s.push_str(&format!("  {}: {}\n", k.replace('.', "_"), v));
        }
        for (who, items) in &self.inventory {
            s.push_str(&format!("  inventory {} {{ items: [{}] }}\n", who, items.join(", ")));
        }
        for (who, ws) in &self.wounds {
            for (part, st) in ws {
                s.push_str(&format!("  wound {} {{ part: {}, state: {} }}\n", who, part, st));
            }
        }
        for (a, b, v) in &self.relations {
            s.push_str(&format!("  relation {}_to_{} {{ value: {} }}\n", a, b, v));
        }
        for (who, ms) in &self.memories {
            for m in ms {
                s.push_str(&format!("  memory {} {{ text: \"{}\" }}\n", who, m.replace('"', "'")));
            }
        }
        s.push_str("}\n");
        s
    }
}

pub struct Scene {
    pub number: u32,
    pub title: String,
    pub set: SetDef,
    pub time_of_day: String,
    /// Global start time of the scene in the episode.
    pub t0: f64,
    pub dur: f64,
    pub actors: Vec<ActorTimeline>,
    pub cameras: Vec<CameraSeg>,
    /// Time span of every top-level statement (links script lines to the timeline).
    pub spans: Vec<StmtSpan>,
}

#[derive(Clone, Debug)]
pub struct StmtSpan {
    /// 1-based script line.
    pub line: u32,
    pub t0: f64,
    pub t1: f64,
    pub label: String,
}

fn stmt_label(s: &ast::Stmt) -> String {
    match &s.kind {
        ast::StmtKind::Prop(k, _) => k.clone(),
        ast::StmtKind::Place(p) => format!("place {}", p.entity),
        ast::StmtKind::Camera(c) => camera::describe(c).replacen("camera ", "", 1),
        ast::StmtKind::Action(a) => format!("{} {}", a.subject, a.verb),
        ast::StmtKind::Dialogue(d) => format!("{}: {}", d.who, d.text),
        ast::StmtKind::Sfx(x) => format!("sfx {}", x.name),
        ast::StmtKind::Music(m) => format!("music {}", m.name),
        ast::StmtKind::Wait(w) => format!("wait {}s", w),
        ast::StmtKind::Together(v) => {
            let inner: Vec<String> = v.iter().map(stmt_label).collect();
            inner.join(" & ")
        }
        _ => "state".into(),
    }
}

impl Scene {
    pub fn camera_at(&self, t: f64) -> CameraPose {
        match self.cameras.iter().rev().find(|c| c.t0 <= t).or_else(|| self.cameras.first()) {
            Some(seg) => seg.pose_at(t, &self.actors),
            None => CameraPose { pos: [0.0, 1.9, -6.0], target: [0.0, 1.0, 0.0], fov: 49.0, roll: 0.0 },
        }
    }

    pub fn frames_at(&self, t: f64) -> Vec<ActorFrame> {
        self.actors
            .iter()
            .enumerate()
            .map(|(i, a)| {
                let kin = a.kin(t);
                let resolve = |g: GazeTarget| match g {
                    GazeTarget::None => None,
                    GazeTarget::Point(p) => Some(p),
                    GazeTarget::Actor(n) => self.actors.iter().find(|o| o.name == n).map(|o| o.head_pos(t)),
                };
                let (gp, gc, gaze_u) = a.gaze_pair(t);
                let (gaze_prev, gaze) = (resolve(gp), resolve(gc));
                let (viseme, speaking) = a.viseme_at(t);
                ActorFrame {
                    idx: i,
                    kin,
                    gesture: a.gesture_at(t),
                    expr: a.expr_at(t),
                    gaze,
                    gaze_prev,
                    gaze_u,
                    viseme,
                    speaking,
                    blink: a.blink_at(t),
                    t,
                }
            })
            .collect()
    }
}

pub struct Episode {
    pub name: String,
    pub fps: u32,
    pub width: u32,
    pub height: u32,
    pub scenes: Vec<Scene>,
    pub duration: f64,
    pub audio: Vec<AudioEvent>,
    pub state: StateLog,
    pub root: PathBuf,
    /// How far the music ducks under speech, in dB (0 = off). Set by `duck_db` in oneiro.toml.
    pub duck_db: f32,
}

impl Episode {
    /// Loads every sound of the episode as mixer events (missing files are skipped; the compiler
    /// already reports them).
    pub fn mix_events(&self) -> Vec<oneiro_audio::mix::Event> {
        self.audio
            .iter()
            .filter_map(|e| {
                read_wav(&e.path).ok().map(|a| {
                    let mut ev = oneiro_audio::mix::Event::new(e.kind, e.start, a, e.gain);
                    ev.fade_in = e.fade_in;
                    ev.fade_out = e.fade_out;
                    ev.stop = e.stop;
                    ev.looped = e.looped;
                    ev
                })
            })
            .collect()
    }

    /// The final soundtrack (mono, 48 kHz) with voices, effects and ducked music.
    pub fn render_audio(&self) -> (Vec<f32>, oneiro_audio::mix::MixReport) {
        let opts = oneiro_audio::mix::MixOptions { duck_db: self.duck_db, ..Default::default() };
        oneiro_audio::mix::mix_with(&self.mix_events(), self.duration, &opts)
    }

    pub fn frame_count(&self) -> usize {
        (self.duration * self.fps as f64).ceil() as usize
    }

    /// Scene index and scene-local time for a global time.
    pub fn locate(&self, t: f64) -> Option<(usize, f64)> {
        for (i, s) in self.scenes.iter().enumerate() {
            if t < s.t0 + s.dur || i == self.scenes.len() - 1 {
                return Some((i, (t - s.t0).clamp(0.0, s.dur)));
            }
        }
        None
    }
}

// ------------------------------------------------------------------ compiler internals

#[derive(Clone, Debug)]
#[allow(dead_code)]
enum Tgt {
    Anchor { name: String, pos: V3, facing: Option<f64>, seat: Option<f64>, solid: bool },
    Actor(String),
    Dir(f64),
}

struct ActorBuild {
    tl: ActorTimeline,
    pos: V2,
    yaw: f64,
    seated: bool,
    seat_h: f64,
    explicit_gaze: bool,
}

struct PendingCam {
    t: f64,
    stmt: ast::CameraStmt,
    pos: Pos,
    index: u32,
}

struct SceneB {
    number: u32,
    set: SetDef,
    set_name: String,
    time_of_day: String,
    global_t: f64,
    actors: BTreeMap<String, ActorBuild>,
    cursor: f64,
    cams: Vec<PendingCam>,
    cam_count: u32,
}

struct Cx<'a> {
    label: String,
    root: PathBuf,
    aspect: f64,
    lock: &'a Lock,
    chars: BTreeMap<String, Arc<CharacterDef>>,
    used_chars: HashSet<String>,
    diags: Vec<Diagnostic>,
    audio: Vec<AudioEvent>,
    /// Index in `audio` of the music that is currently playing.
    music_open: Option<usize>,
    state: StateLog,
}

/// Compiles the script at `script_path`. Always returns an episode (possibly incomplete when
/// there are errors); callers must check the diagnostics for errors before rendering.
pub fn compile(project: &Project, script_path: &Path, lock: &Lock) -> (Episode, Vec<Diagnostic>) {
    let label = script_path
        .strip_prefix(&project.root)
        .unwrap_or(script_path)
        .to_string_lossy()
        .replace('\\', "/");
    match std::fs::read_to_string(script_path) {
        Ok(src) => compile_source(project, &label, &src, lock),
        Err(e) => (
            empty_episode(project),
            vec![Diagnostic::error("E0003", &label, Pos { line: 1, col: 1 }, format!("cannot read script: {}", e))],
        ),
    }
}

fn empty_episode(project: &Project) -> Episode {
    Episode {
        name: project.manifest.name.clone(),
        fps: project.manifest.fps,
        width: project.manifest.width,
        height: project.manifest.height,
        scenes: vec![],
        duration: 0.0,
        audio: vec![],
        state: StateLog::default(),
        root: project.root.clone(),
        duck_db: project.manifest.duck_db,
    }
}

/// Compiles script text that is not necessarily on disk (used by the IDE editor).
pub fn compile_source(project: &Project, label: &str, src: &str, lock: &Lock) -> (Episode, Vec<Diagnostic>) {
    let label = label.to_string();
    let mut episode = empty_episode(project);
    let (file, mut diags) = ast::parse(src, &label);

    let mut chars: BTreeMap<String, CharacterDef> = project.characters.clone();
    let mut sets: BTreeMap<String, SetDef> = project.sets.clone();
    for b in &file.blocks {
        match b.kind.as_str() {
            "character" => {
                let c = CharacterDef::from_block(b, &label, &mut diags);
                chars.insert(c.name.clone(), c);
            }
            "set" => {
                let s = SetDef::from_block(b, &label, &mut diags);
                sets.insert(s.name.clone(), s);
            }
            "style" => {}
            other => diags.push(
                Diagnostic::warning("W0101", &label, b.pos, format!("unknown block '{}' ignored", other)),
            ),
        }
    }

    // Style support check (episode default and every `use character X style Y`).
    let style_error = |style: &str, pos: Pos, diags: &mut Vec<Diagnostic>| {
        if STYLES.contains(&style) {
            return;
        }
        let (msg, sugg) = if PLANNED_STYLES.contains(&style) {
            (
                format!("style '{}' is planned but not available yet", style),
                format!("available styles: {}; toon_3d arrives with the 3D renderer (Phase 7)", STYLES.join(", ")),
            )
        } else {
            let known: Vec<&str> = STYLES.iter().chain(PLANNED_STYLES.iter()).copied().collect();
            (format!("unknown style '{}'", style), format!("{} (available: {})", did_you_mean(style, known), STYLES.join(", ")))
        };
        diags.push(Diagnostic::error("E2301", &label, pos, msg).suggest(sugg));
    };
    let default_style = file.default_style.clone().unwrap_or_else(|| project.manifest.default_style.clone());
    style_error(&default_style, Pos { line: 1, col: 1 }, &mut diags);
    for u in &file.uses {
        if let (true, Some(s)) = (u.kind == "character", &u.style) {
            style_error(s, u.pos, &mut diags);
        }
    }
    // Resolution order for each character: `use ... style X` > the character's own `style:` > the episode default.
    let use_styles: BTreeMap<String, String> = file
        .uses
        .iter()
        .filter(|u| u.kind == "character")
        .filter_map(|u| u.style.clone().filter(|s| STYLES.contains(&s.as_str())).map(|s| (u.name.clone(), s)))
        .collect();
    let default_style = if STYLES.contains(&default_style.as_str()) { default_style } else { "cutout_2d".to_string() };
    for (name, c) in chars.iter_mut() {
        c.style = use_styles.get(name).cloned().or_else(|| Some(c.style.clone()).filter(|s| !s.is_empty())).unwrap_or_else(|| default_style.clone());
    }

    // `use` declarations.
    let mut used_chars = HashSet::new();
    let mut used_sets = HashSet::new();
    for u in &file.uses {
        match u.kind.as_str() {
            "character" => {
                if chars.contains_key(&u.name) {
                    used_chars.insert(u.name.clone());
                } else {
                    diags.push(
                        Diagnostic::error("E1051", &label, u.pos, format!("character '{}' not found in the library", u.name))
                            .suggest(format!(
                                "{} (expected library/characters/{}.onchar)",
                                did_you_mean(&u.name, chars.keys().map(|s| s.as_str())),
                                u.name
                            )),
                    );
                }
            }
            "set" => {
                if sets.contains_key(&u.name) {
                    used_sets.insert(u.name.clone());
                } else {
                    diags.push(
                        Diagnostic::error("E1050", &label, u.pos, format!("set '{}' not found in the library", u.name))
                            .suggest(format!(
                                "{} (expected library/sets/{}.onset)",
                                did_you_mean(&u.name, sets.keys().map(|s| s.as_str())),
                                u.name
                            )),
                    );
                }
            }
            "style" => {}
            other => diags.push(
                Diagnostic::error("E0108", &label, u.pos, format!("cannot 'use' a {}", other))
                    .suggest("use character <Name> or use set <name>"),
            ),
        }
    }

    let mut cx = Cx {
        label: label.clone(),
        root: project.root.clone(),
        aspect: project.manifest.width as f64 / project.manifest.height as f64,
        lock,
        chars: chars.into_iter().map(|(k, v)| (k, Arc::new(v))).collect(),
        used_chars,
        diags,
        audio: vec![],
        music_open: None,
        state: StateLog::default(),
    };

    let mut global_t = 0.0;
    for sc in &file.scenes {
        if !used_sets.contains(&sc.set) {
            let known: Vec<&str> = sets.keys().map(|s| s.as_str()).collect();
            let sugg = if sets.contains_key(&sc.set) {
                format!("add: use set {}", sc.set)
            } else {
                did_you_mean(&sc.set, known)
            };
            cx.diags.push(
                Diagnostic::error("E1050", &label, sc.pos, format!("set '{}' is not declared", sc.set)).suggest(sugg),
            );
            continue;
        }
        let mut scene = cx.scene(sc, sets.get(&sc.set).unwrap().clone(), global_t);
        crate::physics::attach(&mut scene);
        global_t += scene.dur;
        episode.scenes.push(scene);
    }
    episode.duration = global_t;
    episode.audio = cx.audio;
    episode.state = cx.state;
    (episode, cx.diags)
}

impl<'a> Cx<'a> {
    fn err(&mut self, code: &str, pos: Pos, msg: impl Into<String>, sugg: Option<String>) {
        let mut d = Diagnostic::error(code, &self.label, pos, msg);
        if let Some(s) = sugg {
            d = d.suggest(s);
        }
        self.diags.push(d);
    }
    fn warn(&mut self, code: &str, pos: Pos, msg: impl Into<String>, sugg: Option<String>) {
        let mut d = Diagnostic::warning(code, &self.label, pos, msg);
        if let Some(s) = sugg {
            d = d.suggest(s);
        }
        self.diags.push(d);
    }

    // -------------------------------------------------------------- scene
    fn scene(&mut self, sc: &ast::Scene, set: SetDef, global_t: f64) -> Scene {
        let mut sb = SceneB {
            number: sc.number,
            set_name: set.name.clone(),
            set,
            time_of_day: "day".into(),
            global_t,
            actors: BTreeMap::new(),
            cursor: 0.0,
            cams: vec![],
            cam_count: 0,
        };
        let mut spans: Vec<StmtSpan> = Vec::new();
        for s in &sc.stmts {
            let start = sb.cursor;
            let d = self.stmt(&mut sb, s, start);
            sb.cursor += d;
            spans.push(StmtSpan { line: s.pos.line, t0: start, t1: start + d, label: stmt_label(s) });
        }

        // Scene end: after the last thing finishes.
        let mut end = sb.cursor;
        for a in sb.actors.values() {
            let tl = &a.tl;
            for m in &tl.moves {
                end = end.max(m.t1);
            }
            for t in &tl.turns {
                end = end.max(t.t1);
            }
            for s in &tl.sits {
                end = end.max(s.t1);
            }
            for s in &tl.speech {
                end = end.max(s.t1);
            }
            for g in &tl.gestures {
                if g.t1.is_finite() {
                    end = end.max(g.t1);
                }
            }
        }
        let dur = (end + 0.6).max(1.0);

        let mut actors: Vec<ActorTimeline> = Vec::new();
        for (name, mut a) in std::mem::take(&mut sb.actors) {
            let tl = &mut a.tl;
            tl.moves.sort_by(|x, y| x.t0.partial_cmp(&y.t0).unwrap());
            tl.turns.sort_by(|x, y| x.t0.partial_cmp(&y.t0).unwrap());
            tl.sits.sort_by(|x, y| x.t0.partial_cmp(&y.t0).unwrap());
            tl.exprs.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap());
            tl.gazes.sort_by(|x, y| x.t.partial_cmp(&y.t).unwrap());
            for g in tl.gestures.iter_mut() {
                if !g.t1.is_finite() {
                    g.t1 = dur + 1.0;
                }
            }
            tl.blinks = blink_schedule(&name, dur);
            actors.push(a.tl);
        }

        let cameras = self.solve_cameras(&sb, &actors, dur);
        Scene {
            number: sc.number,
            title: sc.title.clone(),
            set: sb.set,
            time_of_day: sb.time_of_day,
            t0: global_t,
            dur,
            actors,
            cameras,
            spans,
        }
    }

    fn solve_cameras(&mut self, sb: &SceneB, actors: &[ActorTimeline], dur: f64) -> Vec<CameraSeg> {
        let mut segs: Vec<CameraSeg> = Vec::new();
        let mut env = SolveEnv { set: &sb.set, actors, sides: HashMap::new(), aspect: self.aspect };

        let default_stmt = ast::CameraStmt { words: vec!["wide".into()], ..Default::default() };
        let need_default = sb.cams.first().map_or(true, |c| c.t > 0.001);
        if need_default {
            if let Ok((pose, _)) = camera::solve(&default_stmt, 0.0, &mut env) {
                segs.push(CameraSeg {
                    t0: 0.0,
                    t1: dur,
                    base: pose,
                    follow: None,
                    mv: None,
                    mv_dur: 1.0,
                    source: "camera wide (automatic)".into(),
                    pinned: false,
                    seed: 0.0,
                    index: 0,
                });
            }
        }

        for pc in &sb.cams {
            let source = camera::describe(&pc.stmt);
            let solved = camera::solve(&pc.stmt, pc.t, &mut env);
            let pinned = self.lock.shots.get(&(sb.number, pc.index)).filter(|l| l.pinned);
            let (mut pose, follow) = match (solved, pinned) {
                (Ok((p, f)), _) => (p, f),
                (Err(_), Some(l)) => (l.pose, None),
                (Err((code, msg, sugg)), None) => {
                    let mut d = Diagnostic::error(code, &self.label, pc.pos, msg);
                    if let Some(s) = sugg {
                        d = d.suggest(s);
                    }
                    self.diags.push(d);
                    continue;
                }
            };
            let mut is_pinned = false;
            if let Some(l) = pinned {
                pose = l.pose;
                is_pinned = true;
                if l.source != source {
                    self.diags.push(
                        Diagnostic::warning(
                            "W3001",
                            &self.label,
                            pc.pos,
                            format!("pinned shot {} was solved for \"{}\" but the script now says \"{}\"", pc.index, l.source, source),
                        )
                        .suggest("review the pinned values in the .onlock file or unpin the shot"),
                    );
                }
            }
            let follow = follow.and_then(|name| {
                actors.iter().find(|a| a.name == name).map(|a| (name, a.kin(pc.t).pos))
            });
            segs.push(CameraSeg {
                t0: pc.t,
                t1: dur,
                base: pose,
                follow,
                mv: pc.stmt.mv.clone().filter(|m| m != "static"),
                mv_dur: pc.stmt.dur.unwrap_or(3.0),
                source,
                pinned: is_pinned,
                seed: hash_str(&format!("{}:{}", sb.number, pc.index)) as f64 % 100.0,
                index: pc.index,
            });
        }
        for i in 0..segs.len() {
            let next = segs.get(i + 1).map(|s| s.t0).unwrap_or(dur);
            segs[i].t1 = next;
        }
        segs
    }

    // -------------------------------------------------------------- statements
    fn stmt(&mut self, sb: &mut SceneB, s: &ast::Stmt, start: f64) -> f64 {
        let pos = s.pos;
        match &s.kind {
            ast::StmtKind::Prop(k, v) => {
                match k.as_str() {
                    "time" => match v.as_str() {
                        Some(t) if TIMES.contains(&t) => sb.time_of_day = t.to_string(),
                        other => self.err(
                            "E1070",
                            pos,
                            format!("invalid time of day '{}'", other.unwrap_or("?")),
                            Some(format!("valid values: {}", TIMES.join(", "))),
                        ),
                    },
                    other => self.warn(
                        "W1071",
                        pos,
                        format!("unknown scene property '{}' ignored", other),
                        Some("supported scene properties: time".into()),
                    ),
                }
                0.0
            }
            ast::StmtKind::Place(p) => {
                self.place(sb, p, pos, start);
                0.0
            }
            ast::StmtKind::Camera(c) => {
                sb.cam_count += 1;
                let index = sb.cam_count;
                sb.cams.push(PendingCam { t: start, stmt: c.clone(), pos, index });
                0.0
            }
            ast::StmtKind::Action(a) => self.action(sb, a, pos, start),
            ast::StmtKind::Dialogue(d) => self.dialogue(sb, d, pos, start),
            ast::StmtKind::Sfx(x) => {
                if let Some(path) = self.find_sound(&x.name, "sounds", pos) {
                    self.audio.push(AudioEvent {
                        kind: AudioKind::Sfx,
                        start: sb.global_t + start,
                        path,
                        gain: x.volume.unwrap_or(0.7) as f32,
                        fade_in: 0.0,
                        fade_out: 0.0,
                        stop: None,
                        looped: false,
                    });
                }
                0.0
            }
            ast::StmtKind::Music(m) => {
                let now = sb.global_t + start;
                // Whatever music is playing fades out from now on (a crossfade when a new track starts).
                let closing = |cx: &mut Cx, fade: f64| {
                    if let Some(i) = cx.music_open.take() {
                        let ev = &mut cx.audio[i];
                        ev.fade_out = fade;
                        ev.stop = Some(now + fade);
                    }
                };
                if m.stop {
                    let fade = m.fade_out.unwrap_or(1.0);
                    closing(self, fade);
                } else if let Some(path) = self.find_sound(&m.name, "music", pos) {
                    let fade = self.music_open.map(|i| self.audio[i].fade_out).unwrap_or(1.0);
                    closing(self, fade.max(m.fade_in.unwrap_or(0.0)));
                    self.audio.push(AudioEvent {
                        kind: AudioKind::Music,
                        start: now,
                        path,
                        gain: m.volume.unwrap_or(0.4) as f32,
                        fade_in: m.fade_in.unwrap_or(0.0),
                        fade_out: m.fade_out.unwrap_or(1.0),
                        stop: None,
                        looped: !m.once,
                    });
                    self.music_open = Some(self.audio.len() - 1);
                }
                0.0
            }
            ast::StmtKind::Wait(w) => *w,
            ast::StmtKind::Together(v) => {
                let mut m = 0.0f64;
                for c in v {
                    m = m.max(self.stmt(sb, c, start));
                }
                m
            }
            ast::StmtKind::Give { who, item } => {
                if self.check_character(who, pos) {
                    self.state.inventory.entry(who.clone()).or_default().push(item.clone());
                }
                0.0
            }
            ast::StmtKind::Wound { who, part, state } => {
                if self.check_character(who, pos) {
                    self.state.wounds.entry(who.clone()).or_default().push((part.clone(), state.clone()));
                }
                0.0
            }
            ast::StmtKind::Relation { from, to, value } => {
                if self.check_character(from, pos) && self.check_character(to, pos) {
                    self.state.relations.push((from.clone(), to.clone(), value.clone()));
                }
                0.0
            }
            ast::StmtKind::Remember { who, text } => {
                if self.check_character(who, pos) {
                    self.state.memories.entry(who.clone()).or_default().push(text.clone());
                }
                0.0
            }
            ast::StmtKind::Assign { target, value } => {
                let (who, field) = target.split_once('.').unwrap_or((target.as_str(), ""));
                if field.is_empty() {
                    self.err("E1049", pos, format!("cannot assign to '{}'", target), Some("write: Marta.mood = angry".into()));
                } else if self.check_character(who, pos) {
                    let v = match value {
                        ast::Value::Str(s) | ast::Value::Ident(s) => s.clone(),
                        ast::Value::Num(n, u) => format!("{}{}", n, u),
                        _ => "?".into(),
                    };
                    self.state.fields.insert(format!("{}.{}", who, field), v);
                }
                0.0
            }
        }
    }

    fn check_character(&mut self, name: &str, pos: Pos) -> bool {
        if self.used_chars.contains(name) {
            return true;
        }
        let known: Vec<String> = self.chars.keys().cloned().collect();
        let sugg = if self.chars.contains_key(name) {
            format!("add: use character {}", name)
        } else {
            did_you_mean(name, known.iter().map(|s| s.as_str()))
        };
        self.err("E1040", pos, format!("unknown character '{}'", name), Some(sugg));
        false
    }

    fn find_sound(&mut self, name: &str, folder: &str, pos: Pos) -> Option<PathBuf> {
        let mut candidates = vec![];
        if name.contains('/') || name.contains('.') {
            candidates.push(self.root.join(name));
        }
        candidates.push(self.root.join("library").join(folder).join(format!("{}.wav", name)));
        candidates.push(self.root.join("library").join("sounds").join(format!("{}.wav", name)));
        for c in &candidates {
            if c.exists() {
                return Some(c.clone());
            }
        }
        self.err(
            "E2401",
            pos,
            format!("sound '{}' not found", name),
            Some(format!("put a WAV file at library/{}/{}.wav (or run `oneiro demo-audio` for placeholder sounds)", folder, name)),
        );
        None
    }

    // -------------------------------------------------------------- targets
    fn resolve(&mut self, sb: &SceneB, s: &str, pos: Pos) -> Option<Tgt> {
        if let Some(rest) = s.strip_prefix("@pt:") {
            let mut it = rest.split(':');
            let x: f64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            let z: f64 = it.next().and_then(|v| v.parse().ok()).unwrap_or(0.0);
            return Some(Tgt::Anchor { name: "point".into(), pos: [x, 0.0, z], facing: None, seat: None, solid: false });
        }
        if let Some(y) = dir_yaw(s) {
            return Some(Tgt::Dir(y));
        }
        match s {
            "camera" => return Some(Tgt::Dir(PI)),
            "away_from_camera" => return Some(Tgt::Dir(0.0)),
            _ => {}
        }
        if sb.actors.contains_key(s) {
            return Some(Tgt::Actor(s.to_string()));
        }
        if self.chars.contains_key(s) {
            self.err(
                "E1041",
                pos,
                format!("'{}' has not been placed in this scene yet", s),
                Some(format!("add: place {} at <anchor>", s)),
            );
            return None;
        }
        let n = match s.split_once('.') {
            Some((prefix, rest)) if prefix == sb.set_name => rest,
            _ => s,
        };
        if let Some(a) = sb.set.anchors.get(n) {
            return Some(Tgt::Anchor {
                name: n.to_string(),
                pos: a.pos,
                facing: a.facing,
                seat: if a.seat { Some(a.seat_height) } else { None },
                solid: a.solid,
            });
        }
        if let Some(p) = sb.set.props.iter().find(|p| p.name == n) {
            return Some(Tgt::Anchor { name: n.to_string(), pos: [p.at[0], 0.0, p.at[1]], facing: None, seat: None, solid: true });
        }
        let mut names: Vec<&str> = sb.set.anchors.keys().map(|k| k.as_str()).collect();
        names.extend(sb.set.props.iter().map(|p| p.name.as_str()));
        names.extend(sb.actors.keys().map(|k| k.as_str()));
        self.err(
            "E1044",
            pos,
            format!("unknown anchor, prop or character '{}' in set '{}'", s, sb.set_name),
            Some(did_you_mean(n, names)),
        );
        None
    }

    // -------------------------------------------------------------- place
    fn place(&mut self, sb: &mut SceneB, p: &ast::Place, pos: Pos, start: f64) {
        if !self.check_character(&p.entity, pos) {
            return;
        }
        if sb.actors.contains_key(&p.entity) {
            self.err(
                "E1047",
                pos,
                format!("'{}' is already placed in this scene", p.entity),
                Some(format!("use '{} walks_to <anchor>' to move them", p.entity)),
            );
            return;
        }
        if start > 0.0 {
            self.warn(
                "W1044",
                pos,
                format!("'{}' is placed after the scene started; characters exist from the first frame", p.entity),
                Some("move the 'place' statements to the top of the scene".into()),
            );
        }
        if !RELATIONS.contains(&p.rel.as_str()) {
            if PLANNED_RELATIONS.contains(&p.rel.as_str()) {
                self.err("E2010", pos, format!("relation '{}' is not available yet", p.rel), None);
            } else {
                self.err(
                    "E1048",
                    pos,
                    format!("unknown relation '{}'", p.rel),
                    Some(did_you_mean(&p.rel, RELATIONS.iter().copied())),
                );
            }
            return;
        }
        let Some(tgt) = self.resolve(sb, &p.target, pos) else { return };
        let d = p.dist.unwrap_or(0.9);
        let mut seat: Option<f64> = None;
        let mut default_yaw: Option<f64> = None;
        let (base, bf): (V2, Option<f64>) = match &tgt {
            Tgt::Anchor { pos: ap, facing, seat: s, .. } => {
                if p.rel == "at" {
                    seat = *s;
                    default_yaw = *facing;
                }
                ([ap[0], ap[2]], *facing)
            }
            Tgt::Actor(n) => {
                let a = &sb.actors[n];
                (a.pos, Some(a.yaw))
            }
            Tgt::Dir(_) => {
                self.err("E1049", pos, "cannot place a character at a direction", Some("place at an anchor or relative to another character".into()));
                return;
            }
        };
        let f = bf.map(fwd).unwrap_or([0.0, -1.0]);
        let r = bf.map(|y| [y.cos(), -y.sin()]).unwrap_or([1.0, 0.0]);
        let xz = match p.rel.as_str() {
            "at" => match &tgt {
                Tgt::Actor(_) => [base[0] + r[0] * d, base[1] + r[1] * d],
                _ => base,
            },
            "beside" | "right_of" => [base[0] + r[0] * d, base[1] + r[1] * d],
            "left_of" => [base[0] - r[0] * d, base[1] - r[1] * d],
            "in_front_of" => [base[0] + f[0] * d, base[1] + f[1] * d],
            "behind" => [base[0] - f[0] * d, base[1] - f[1] * d],
            _ => [base[0] + (f[0] * 0.5 + r[0] * 0.7) * d, base[1] + (f[1] * 0.5 + r[1] * 0.7) * d],
        };
        let hw = sb.set.width / 2.0 - 0.3;
        let hd = sb.set.depth / 2.0 - 0.3;
        // Placing something "at" a solid anchor (a counter, a wall) puts it in front of it, facing it.
        let mut xz = xz;
        if let (Tgt::Anchor { pos: ap, solid: true, facing: None, seat: None, .. }, "at") = (&tgt, p.rel.as_str()) {
            let a = [ap[0], ap[2]];
            let u = norm2([-a[0], -a[1]]);
            let u = if a[0].abs() + a[1].abs() < 1e-6 { [0.0, -1.0] } else { u };
            xz = [a[0] + u[0] * 0.75, a[1] + u[1] * 0.75];
            default_yaw = Some(yaw_to(xz, a));
        }
        let mut xz = [xz[0].clamp(-hw, hw), xz[1].clamp(-hd, hd)];
        let mut yaw = default_yaw.unwrap_or(PI);
        if let Some(fc) = &p.facing {
            if let Some(t) = self.resolve(sb, fc, pos) {
                yaw = match t {
                    Tgt::Dir(y) => y,
                    Tgt::Anchor { pos: ap, .. } => yaw_to(xz, [ap[0], ap[2]]),
                    Tgt::Actor(n) => yaw_to(xz, sb.actors[&n].pos),
                };
            }
        }
        if let Some(l) = self.lock.places.get(&(sb.number, p.entity.clone())).filter(|l| l.pinned) {
            xz = l.pos;
            yaw = l.yaw;
        }
        let def = self.chars[&p.entity].clone();
        let mut tl = ActorTimeline::new(&p.entity, def);
        tl.init_pos = xz;
        tl.init_yaw = yaw;
        if let Some(h) = seat {
            tl.init_sit = 1.0;
            tl.init_seat_h = h;
        }
        sb.actors.insert(
            p.entity.clone(),
            ActorBuild { tl, pos: xz, yaw, seated: seat.is_some(), seat_h: seat.unwrap_or(0.62), explicit_gaze: false },
        );
    }

    // -------------------------------------------------------------- actions
    fn action(&mut self, sb: &mut SceneB, a: &ast::Action, pos: Pos, start: f64) -> f64 {
        if !self.check_character(&a.subject, pos) {
            return 0.0;
        }
        if !sb.actors.contains_key(&a.subject) {
            self.err(
                "E1041",
                pos,
                format!("'{}' has not been placed in this scene yet", a.subject),
                Some(format!("add: place {} at <anchor>", a.subject)),
            );
            return 0.0;
        }
        let verb = a.verb.as_str();
        if !IMPLEMENTED_VERBS.contains(&verb) {
            if PLANNED_VERBS.contains(&verb) {
                self.err(
                    "E2010",
                    pos,
                    format!("verb '{}' is part of the language but not available in this version", verb),
                    Some(format!("available verbs: {}", IMPLEMENTED_VERBS.join(", "))),
                );
            } else {
                let all: Vec<&str> = IMPLEMENTED_VERBS.iter().chain(PLANNED_VERBS.iter()).copied().collect();
                self.err("E1042", pos, format!("unknown verb '{}'", verb), Some(did_you_mean(verb, all)));
            }
            return 0.0;
        }
        // Positional words, without manner adverbs.
        let words: Vec<String> = a
            .args
            .iter()
            .filter_map(|x| match x {
                ast::Arg::Word(w) if !MANNER_WORDS.contains(&w.as_str()) => Some(w.clone()),
                ast::Arg::Point(px, pz) => Some(format!("@pt:{}:{}", px, pz)),
                _ => None,
            })
            .collect();
        let slow = a.args.iter().any(|x| matches!(x, ast::Arg::Word(w) if w == "slowly"));
        let quick = a.args.iter().any(|x| matches!(x, ast::Arg::Word(w) if w == "quickly"));
        let speed_mul = if slow { 0.6 } else if quick { 1.5 } else { 1.0 };
        let who = a.subject.clone();

        match verb {
            "walks_to" | "runs_to" | "sits_on" => {
                let Some(t) = words.first() else {
                    self.err("E1043", pos, format!("'{}' needs a destination", verb), Some(format!("write: {} {} diner.counter", who, verb)));
                    return 0.0;
                };
                let Some(tgt) = self.resolve(sb, t, pos) else { return 0.0 };
                if verb == "sits_on" {
                    let seat_h = match &tgt {
                        Tgt::Anchor { seat, .. } => seat.unwrap_or(0.5),
                        _ => {
                            self.err("E1043", pos, "sits_on needs an anchor (a stool, chair...)", None);
                            return 0.0;
                        }
                    };
                    let mut d = self.walk(sb, &who, &tgt, start, false, speed_mul, a.dur, true);
                    let ab = sb.actors.get_mut(&who).unwrap();
                    ab.tl.sits.push(SitSeg { t0: start + d, t1: start + d + 0.6, from: 0.0, to: 1.0, height: seat_h });
                    ab.seated = true;
                    ab.seat_h = seat_h;
                    d += 0.6;
                    d
                } else {
                    self.walk(sb, &who, &tgt, start, verb == "runs_to", speed_mul, a.dur, false)
                }
            }
            "turns_to" => {
                let Some(t) = words.first() else {
                    self.err("E1043", pos, "turns_to needs a target", None);
                    return 0.0;
                };
                let Some(tgt) = self.resolve(sb, t, pos) else { return 0.0 };
                let ab = sb.actors.get(&who).unwrap();
                let want = match &tgt {
                    Tgt::Dir(y) => *y,
                    Tgt::Anchor { pos: ap, .. } => yaw_to(ab.pos, [ap[0], ap[2]]),
                    Tgt::Actor(n) => yaw_to(ab.pos, sb.actors[n].pos),
                };
                let ab = sb.actors.get_mut(&who).unwrap();
                let diff = angle_diff(want, ab.yaw);
                let dur = (diff.abs() / 3.5).clamp(0.2, 0.7);
                push_turn(ab, start, want, dur);
                dur
            }
            "stops" => 0.0,
            "stands" => {
                let ab = sb.actors.get_mut(&who).unwrap();
                if ab.seated {
                    ab.tl.sits.push(SitSeg { t0: start, t1: start + 0.5, from: 1.0, to: 0.0, height: ab.seat_h });
                    ab.seated = false;
                    0.5
                } else {
                    0.0
                }
            }
            "looks_at" | "points_at" => {
                let target = if let Some(t) = words.first() {
                    let Some(tgt) = self.resolve(sb, t, pos) else { return 0.0 };
                    Some(tgt)
                } else {
                    None
                };
                let (gt, point): (GazeTarget, Option<V3>) = match &target {
                    None => {
                        self.err("E1043", pos, format!("{} needs a target", verb), Some(format!("write: {} {} diner.clock", who, verb)));
                        return 0.0;
                    }
                    Some(Tgt::Actor(n)) => {
                        let h = sb.actors[n].tl.def.height() * 0.78;
                        let p = sb.actors[n].pos;
                        (GazeTarget::Actor(n.clone()), Some([p[0], h, p[1]]))
                    }
                    Some(Tgt::Anchor { pos: ap, .. }) => {
                        let y = if ap[1] > 0.0 { ap[1] } else { 1.0 };
                        let p = [ap[0], y, ap[2]];
                        (GazeTarget::Point(p), Some(p))
                    }
                    Some(Tgt::Dir(yw)) => {
                        let ab = &sb.actors[&who];
                        let f = fwd(*yw);
                        let p = [ab.pos[0] + f[0] * 5.0, 1.0, ab.pos[1] + f[1] * 5.0];
                        (GazeTarget::Point(p), Some(p))
                    }
                };
                let ab = sb.actors.get_mut(&who).unwrap();
                if verb == "looks_at" {
                    ab.tl.gazes.push(GazeKey { t: start, target: gt });
                    ab.explicit_gaze = true;
                    0.4
                } else {
                    let (g, d, hold) = gesture_for_verb("points_at").unwrap();
                    push_gesture(ab, start, a.dur.unwrap_or(d), g, hold, point);
                    ab.tl.gazes.push(GazeKey { t: start, target: gt });
                    a.dur.unwrap_or(d)
                }
            }
            "looks_away" => {
                let ab = sb.actors.get_mut(&who).unwrap();
                ab.tl.gazes.push(GazeKey { t: start, target: GazeTarget::None });
                ab.explicit_gaze = false;
                0.3
            }
            "emote" => {
                let Some(name) = words.first() else {
                    self.err("E1043", pos, "emote needs an expression name", Some(format!("valid expressions: {}", EXPRESSIONS.join(", "))));
                    return 0.0;
                };
                if expression(name).is_none() {
                    self.err(
                        "E1046",
                        pos,
                        format!("unknown expression '{}'", name),
                        Some(did_you_mean(name, EXPRESSIONS.iter().copied())),
                    );
                    return 0.0;
                }
                let mut intensity = 1.0;
                let mut it = a.args.iter();
                while let Some(x) = it.next() {
                    if matches!(x, ast::Arg::Word(w) if w == "intensity") {
                        if let Some(ast::Arg::Num(n, _)) = it.next() {
                            intensity = n.clamp(0.0, 1.0);
                        }
                    }
                }
                let ab = sb.actors.get_mut(&who).unwrap();
                ab.tl.exprs.push(ExprKey { t: start, name: name.clone(), intensity });
                0.3
            }
            "uncrosses_arms" => {
                let ab = sb.actors.get_mut(&who).unwrap();
                if let Some(last) = ab.tl.gestures.last_mut() {
                    if last.hold && last.t1 > start {
                        last.t1 = start + 0.25;
                    }
                }
                0.3
            }
            v => {
                // Gesture verbs.
                let (g, d, hold) = gesture_for_verb(v).unwrap();
                let dur = a.dur.unwrap_or(d);
                let ab = sb.actors.get_mut(&who).unwrap();
                push_gesture(ab, start, dur, g, hold, None);
                dur
            }
        }
    }

    /// Walks an actor to a target. Returns the elapsed time.
    #[allow(clippy::too_many_arguments)]
    fn walk(
        &mut self,
        sb: &mut SceneB,
        who: &str,
        tgt: &Tgt,
        start: f64,
        run: bool,
        speed_mul: f64,
        dur_override: Option<f64>,
        ignore_solid: bool,
    ) -> f64 {
        let (cur, seated) = {
            let a = &sb.actors[who];
            (a.pos, a.seated)
        };
        let mut exclude: Option<String> = None;
        let mut dest = match tgt {
            Tgt::Anchor { pos, solid, .. } => {
                let ap = [pos[0], pos[2]];
                if *solid && !ignore_solid {
                    let u = norm2([ap[0] - cur[0], ap[1] - cur[1]]);
                    [ap[0] - u[0] * 0.75, ap[1] - u[1] * 0.75]
                } else {
                    ap
                }
            }
            Tgt::Actor(n) => {
                exclude = Some(n.clone());
                let o = sb.actors[n].pos;
                let u = norm2([o[0] - cur[0], o[1] - cur[1]]);
                [o[0] - u[0] * 0.9, o[1] - u[1] * 0.9]
            }
            Tgt::Dir(_) => cur,
        };
        // Keep a respectful distance from other characters.
        for (n, o) in sb.actors.iter() {
            if n == who || exclude.as_deref() == Some(n.as_str()) {
                continue;
            }
            if dist2(dest, o.pos) < 0.75 {
                let u = norm2([dest[0] - o.pos[0], dest[1] - o.pos[1]]);
                let u = if (dest[0] - o.pos[0]).abs() + (dest[1] - o.pos[1]).abs() < 1e-6 { [1.0, 0.0] } else { u };
                dest = [o.pos[0] + u[0] * 0.85, o.pos[1] + u[1] * 0.85];
            }
        }
        let hw = sb.set.width / 2.0 - 0.3;
        let hd = sb.set.depth / 2.0 - 0.3;
        dest = [dest[0].clamp(-hw, hw), dest[1].clamp(-hd, hd)];

        let mut t = start;
        let a = sb.actors.get_mut(who).unwrap();
        if seated {
            a.tl.sits.push(SitSeg { t0: t, t1: t + 0.5, from: 1.0, to: 0.0, height: a.seat_h });
            a.seated = false;
            t += 0.5;
        }
        let d = dist2(cur, dest);
        if d > 0.05 {
            let want = yaw_to(cur, dest);
            let diff = angle_diff(want, a.yaw).abs();
            let tt = (diff / 3.5).clamp(0.12, 0.6);
            push_turn(a, t, want, tt);
            t += tt;
            let speed = if run { 3.0 } else { 1.25 } * speed_mul;
            let dur = dur_override.unwrap_or(d / speed).max(0.3);
            a.tl.moves.push(MoveSeg { t0: t, t1: t + dur, from: cur, to: dest, run });
            a.pos = dest;
            t += dur;
        }
        let final_yaw = match tgt {
            Tgt::Anchor { facing: Some(y), .. } => Some(*y),
            Tgt::Anchor { pos, solid: true, .. } if !ignore_solid => Some(yaw_to(dest, [pos[0], pos[2]])),
            Tgt::Dir(y) => Some(*y),
            Tgt::Actor(n) => {
                let o = sb.actors[n].pos;
                Some(yaw_to(dest, o))
            }
            _ => None,
        };
        if let Some(y) = final_yaw {
            let a = sb.actors.get_mut(who).unwrap();
            if angle_diff(y, a.yaw).abs() > 0.05 {
                push_turn(a, t, y, 0.3);
                t += 0.3;
            }
        }
        t - start
    }

    // -------------------------------------------------------------- dialogue
    fn dialogue(&mut self, sb: &mut SceneB, d: &ast::Dialogue, pos: Pos, start: f64) -> f64 {
        if !self.check_character(&d.who, pos) {
            return 0.0;
        }
        if !sb.actors.contains_key(&d.who) {
            self.err(
                "E1041",
                pos,
                format!("'{}' has not been placed in this scene yet", d.who),
                Some(format!("add: place {} at <anchor>", d.who)),
            );
            return 0.0;
        }
        if expression(&d.emotion).is_none() {
            self.err(
                "E1046",
                pos,
                format!("unknown emotion '{}'", d.emotion),
                Some(did_you_mean(&d.emotion, EXPRESSIONS.iter().copied())),
            );
            return 0.0;
        }
        // Audio (or an estimated duration when the line has no audio file).
        let lead = 0.05;
        let (track, speech_dur, audio_path): (Arc<oneiro_audio::VisemeTrack>, f64, Option<PathBuf>) = match &d.audio {
            Some(p) => {
                let full = self.root.join(p);
                match load_track(&full) {
                    Some((track, dur)) => (track, dur, Some(full)),
                    None => {
                        self.err(
                            "E2401",
                            pos,
                            format!("audio file '{}' not found or unreadable", p),
                            Some("record or import a WAV file there, or run `oneiro demo-audio` to synthesize placeholder voices".into()),
                        );
                        let dur = (d.text.chars().count() as f64 * 0.065).max(0.8);
                        (Arc::new(lipsync::from_text(&d.text, dur)), dur, None)
                    }
                }
            }
            None => {
                let dur = (d.text.chars().count() as f64 * 0.065).max(0.8);
                (Arc::new(lipsync::from_text(&d.text, dur)), dur, None)
            }
        };
        if let Some(path) = audio_path {
            self.audio.push(AudioEvent { kind: AudioKind::Voice, start: sb.global_t + start + lead, path, gain: 1.0, fade_in: 0.0, fade_out: 0.0, stop: None, looped: false });
        }

        // Partner for gaze / body turning.
        let me = sb.actors[&d.who].pos;
        let partner: Option<(String, V2)> = sb
            .actors
            .iter()
            .filter(|(n, _)| **n != d.who)
            .map(|(n, a)| (n.clone(), a.pos))
            .filter(|(_, p)| dist2(me, *p) < 8.0)
            .min_by(|a, b| dist2(me, a.1).partial_cmp(&dist2(me, b.1)).unwrap());

        {
            let a = sb.actors.get_mut(&d.who).unwrap();
            a.tl.exprs.push(ExprKey { t: start, name: d.emotion.clone(), intensity: 1.0 });
            a.tl.speech.push(SpeechSeg { t0: start + lead, t1: start + lead + speech_dur, track });
            if let Some((pn, pp)) = &partner {
                if !a.explicit_gaze {
                    a.tl.gazes.push(GazeKey { t: start, target: GazeTarget::Actor(pn.clone()) });
                }
                if !a.seated {
                    let want = yaw_to(a.pos, *pp);
                    if angle_diff(want, a.yaw).abs() > 1.4 {
                        push_turn(a, start, want, 0.4);
                    }
                }
            }
        }
        if let Some((pn, pp)) = partner {
            let l = sb.actors.get_mut(&pn).unwrap();
            if !l.explicit_gaze {
                l.tl.gazes.push(GazeKey { t: start, target: GazeTarget::Actor(d.who.clone()) });
            }
            if !l.seated {
                let want = yaw_to(pp, me);
                if angle_diff(want, l.yaw).abs() > 1.9 {
                    push_turn(l, start + 0.2, want, 0.45);
                }
            }
        }
        lead + speech_dur + 0.25
    }
}

fn push_turn(a: &mut ActorBuild, t0: f64, to: f64, dur: f64) {
    let diff = angle_diff(to, a.yaw);
    a.tl.turns.push(TurnSeg { t0, t1: t0 + dur, from: a.yaw, delta: diff });
    a.yaw += diff;
}

fn push_gesture(a: &mut ActorBuild, t0: f64, dur: f64, kind: Gesture, hold: bool, target: Option<V3>) {
    if let Some(last) = a.tl.gestures.last_mut() {
        if last.hold && last.t1 > t0 {
            last.t1 = t0;
        }
    }
    let t1 = if hold { f64::INFINITY } else { t0 + dur };
    a.tl.gestures.push(GestureSeg { t0, t1, kind, hold, target });
}

type TrackCache = std::sync::Mutex<HashMap<(PathBuf, u64, u64), (Arc<oneiro_audio::VisemeTrack>, f64)>>;

/// Reads a WAV and computes its viseme track, cached by path + modification time + size so that
/// the IDE can recompile on every keystroke without re-analyzing audio.
fn load_track(path: &Path) -> Option<(Arc<oneiro_audio::VisemeTrack>, f64)> {
    static CACHE: std::sync::OnceLock<TrackCache> = std::sync::OnceLock::new();
    let meta = std::fs::metadata(path).ok()?;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let key = (path.to_path_buf(), mtime, meta.len());
    let cache = CACHE.get_or_init(Default::default);
    if let Some(hit) = cache.lock().ok()?.get(&key) {
        return Some(hit.clone());
    }
    let a = read_wav(path).ok()?;
    let value = (Arc::new(lipsync::from_audio(&a)), a.duration());
    cache.lock().ok()?.insert(key, value.clone());
    Some(value)
}
