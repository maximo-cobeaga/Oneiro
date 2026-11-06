//! Small helper renders for the IDE: a character on a plain backdrop and a set without actors.

use crate::Renderer;
use oneiro_audio::{Shape, Viseme, VisemeTrack};
use oneiro_core::camera::{self, CameraPose, CameraSeg, SolveEnv};
use oneiro_core::catalog::EXPRESSIONS;
use oneiro_core::timeline::{ActorTimeline, ExprKey, MoveSeg, SpeechSeg};
use oneiro_core::{CharacterDef, Episode, Rgb, Scene, SetDef};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use tiny_skia::Pixmap;

pub fn plain_set() -> SetDef {
    SetDef {
        name: "studio".into(),
        width: 8.0,
        depth: 6.0,
        height: 3.0,
        anchors: BTreeMap::new(),
        props: vec![],
        wall: Rgb(214, 218, 230),
        wall_alt: Rgb(206, 210, 224),
        floor: Rgb(196, 200, 212),
        floor_alt: Rgb(188, 192, 206),
        trim: Rgb(150, 156, 178),
        look: "flat".into(),
        open: false,
        sky: (Rgb(120, 190, 240), Rgb(255, 244, 214)),
        ground: Rgb(226, 208, 150),
        backdrops: Default::default(),
        sprites: vec![],
        solids: vec![],
    }
}

fn episode_with(scene: Scene, w: u32, h: u32) -> Episode {
    Episode {
        name: "preview".into(),
        fps: 24,
        width: w,
        height: h,
        duration: scene.dur,
        scenes: vec![scene],
        audio: vec![],
        duck_db: -9.0,
        state: Default::default(),
        root: PathBuf::new(),
    }
}

fn single_camera(pose: CameraPose, source: &str, dur: f64) -> Vec<CameraSeg> {
    vec![CameraSeg {
        t0: 0.0,
        t1: dur,
        base: pose,
        follow: None,
        mv: None,
        mv_dur: 1.0,
        source: source.into(),
        pinned: false,
        seed: 0.0,
        index: 0,
    }]
}

/// Options for a character portrait.
pub struct PortraitOpts {
    /// 0 = facing the camera, 90 = facing screen-right, 180 = facing away.
    pub turn_deg: f64,
    pub expression: String,
    /// Mouth shape to show (None = closed).
    pub mouth: Option<(Shape, f32)>,
    pub time: f64,
    pub walking: bool,
}

impl Default for PortraitOpts {
    fn default() -> Self {
        PortraitOpts { turn_deg: 0.0, expression: "neutral".into(), mouth: None, time: 0.0, walking: false }
    }
}

pub fn character_portrait(def: &CharacterDef, opts: &PortraitOpts, w: u32, h: u32) -> Pixmap {
    let hgt = def.height();
    let mut tl = ActorTimeline::new(&def.name, Arc::new(def.clone()));
    tl.init_pos = [0.0, 0.0];
    tl.init_yaw = std::f64::consts::PI - opts.turn_deg.to_radians();
    if EXPRESSIONS.contains(&opts.expression.as_str()) {
        tl.exprs.push(ExprKey { t: 0.0, name: opts.expression.clone(), intensity: 1.0 });
    }
    if let Some((shape, open)) = opts.mouth {
        tl.speech.push(SpeechSeg {
            t0: 0.0,
            t1: 1000.0,
            track: Arc::new(VisemeTrack { hop: 1.0, frames: vec![Viseme { shape, open }; 1100] }),
        });
    }
    if opts.walking {
        tl.moves.push(MoveSeg { t0: -1000.0, t1: 1000.0, from: [0.0, 0.0], to: [0.0, 0.0], run: false });
    }
    let vis = hgt * 1.35;
    let fov: f64 = 28.0;
    let dist = vis / (2.0 * (fov / 2.0).to_radians().tan());
    let pose = CameraPose { pos: [0.0, hgt * 0.5, -dist], target: [0.0, hgt * 0.5, 0.0], fov, roll: 0.0 };
    let scene = Scene {
        number: 1,
        title: String::new(),
        set: plain_set(),
        time_of_day: "day".into(),
        t0: 0.0,
        dur: 2000.0,
        actors: vec![tl],
        cameras: single_camera(pose, "portrait", 2000.0),
        transitions: vec![],
        spans: vec![],
    };
    let ep = episode_with(scene, w, h);
    let r = Renderer::new(&ep, w, h);
    let sc = &ep.scenes[0];
    let mut frames = sc.frames_at(opts.time);
    if opts.walking {
        // Walk in place: the distance walked advances with time.
        frames[0].kin.odo = opts.time * 1.25;
        frames[0].kin.speed = 1.25;
    }
    r.render_scene(sc, pose, frames)
}

/// A wide view of a set with nobody in it.
pub fn set_preview(set: &SetDef, w: u32, h: u32) -> Pixmap {
    let mut env = SolveEnv { set, actors: &[], sides: HashMap::new(), aspect: w as f64 / h as f64 };
    let stmt = oneiro_lang::CameraStmt { words: vec!["wide".into()], ..Default::default() };
    let pose = camera::solve(&stmt, 0.0, &mut env)
        .map(|x| x.0)
        .unwrap_or(CameraPose { pos: [0.0, 1.9, -6.0], target: [0.0, 1.0, 0.0], fov: 49.0, roll: 0.0 });
    let scene = Scene {
        number: 1,
        title: String::new(),
        set: set.clone(),
        time_of_day: "day".into(),
        t0: 0.0,
        dur: 10.0,
        actors: vec![],
        cameras: single_camera(pose, "preview", 10.0),
        transitions: vec![],
        spans: vec![],
    };
    let ep = episode_with(scene, w, h);
    let r = Renderer::new(&ep, w, h);
    r.render_scene(&ep.scenes[0], pose, vec![])
}

/// A grid of stills, one per camera shot (taken in the middle of each shot), so that a model or a
/// person can judge framing at a glance. Returns the sheet and one caption per tile (row-major).
pub fn contact_sheet(ep: &Episode, max_shots: usize, tile_w: u32, cols: usize) -> Option<(Pixmap, Vec<String>)> {
    let mut moments: Vec<(f64, String)> = Vec::new();
    for sc in &ep.scenes {
        for c in &sc.cameras {
            if c.t1 - c.t0 < 0.2 {
                continue;
            }
            let t = sc.t0 + (c.t0 + c.t1) / 2.0;
            let label = if c.index > 0 { format!("shot {}: {}", c.index, c.source) } else { c.source.clone() };
            moments.push((t, label));
        }
    }
    if moments.is_empty() {
        return None;
    }
    if moments.len() > max_shots {
        let n = moments.len();
        moments = (0..max_shots).map(|i| moments[i * (n - 1) / (max_shots - 1).max(1)].clone()).collect();
    }
    let aspect = ep.height as f64 / ep.width as f64;
    let tile_w = tile_w.max(160) & !1;
    let tile_h = ((tile_w as f64 * aspect) as u32).max(90) & !1;
    let cols = cols.clamp(1, moments.len());
    let rows = moments.len().div_ceil(cols);
    let mut sheet = Pixmap::new(cols as u32 * tile_w + (cols as u32 - 1) * 4, rows as u32 * tile_h + (rows as u32 - 1) * 4)?;
    sheet.fill(tiny_skia::Color::from_rgba8(10, 10, 14, 255));
    let renderer = Renderer::new(ep, tile_w, tile_h);
    let mut captions = Vec::new();
    for (i, (t, label)) in moments.iter().enumerate() {
        let pm = renderer.render_at(*t);
        let (cx, cy) = ((i % cols) as i32 * (tile_w as i32 + 4), (i / cols) as i32 * (tile_h as i32 + 4));
        sheet.draw_pixmap(cx, cy, pm.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        captions.push(format!("{}. t={:.1}s  {}", i + 1, t, label));
    }
    Some((sheet, captions))
}
