//! Per-actor timelines. The compiler turns a script into these segments; the renderer asks
//! for the actor's state at any time `t` (pure function of `t`, so renders are deterministic).

use crate::catalog::{expression, ExprParams, Gesture};
use crate::defs::CharacterDef;
use crate::math::*;
use oneiro_audio::{Viseme, VisemeTrack};
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct MoveSeg {
    pub t0: f64,
    pub t1: f64,
    pub from: V2,
    pub to: V2,
    pub run: bool,
}

#[derive(Clone, Debug)]
pub struct TurnSeg {
    pub t0: f64,
    pub t1: f64,
    pub from: f64,
    pub delta: f64,
}

#[derive(Clone, Debug)]
pub struct SitSeg {
    pub t0: f64,
    pub t1: f64,
    pub from: f64,
    pub to: f64,
    pub height: f64,
}

#[derive(Clone, Debug)]
pub struct GestureSeg {
    pub t0: f64,
    pub t1: f64,
    pub kind: Gesture,
    pub hold: bool,
    pub target: Option<V3>,
}

#[derive(Clone, Debug)]
pub struct ExprKey {
    pub t: f64,
    pub name: String,
    pub intensity: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GazeTarget {
    None,
    Point(V3),
    Actor(String),
}

#[derive(Clone, Debug)]
pub struct GazeKey {
    pub t: f64,
    pub target: GazeTarget,
}

#[derive(Clone, Debug)]
pub struct SpeechSeg {
    pub t0: f64,
    pub t1: f64,
    pub track: Arc<VisemeTrack>,
}

#[derive(Clone, Debug)]
pub struct ActorTimeline {
    pub name: String,
    pub def: Arc<CharacterDef>,
    pub init_pos: V2,
    pub init_yaw: f64,
    pub init_sit: f64,
    pub init_seat_h: f64,
    pub moves: Vec<MoveSeg>,
    pub turns: Vec<TurnSeg>,
    pub sits: Vec<SitSeg>,
    pub gestures: Vec<GestureSeg>,
    pub exprs: Vec<ExprKey>,
    pub gazes: Vec<GazeKey>,
    pub speech: Vec<SpeechSeg>,
    pub blinks: Vec<f64>,
    /// Cape and tail simulation (3D characters only), filled in by `physics::attach` when the scene is compiled.
    pub physics: Option<std::sync::Arc<crate::physics::Physics>>,
}

#[derive(Clone, Debug)]
pub struct Kin {
    pub pos: V2,
    pub yaw: f64,
    pub sit: f64,
    pub seat_h: f64,
    /// Distance walked so far (drives the stride cycle).
    pub odo: f64,
    pub speed: f64,
    pub running: bool,
}

#[derive(Clone, Debug)]
pub struct GestureState {
    pub kind: Gesture,
    pub time: f64,
    pub weight: f64,
    pub target: Option<V3>,
}

#[derive(Clone, Debug)]
pub struct ActorFrame {
    pub idx: usize,
    pub kin: Kin,
    pub gesture: Option<GestureState>,
    pub expr: ExprParams,
    /// World-space point the head should look at (and the previous one, for blending).
    pub gaze: Option<V3>,
    pub gaze_prev: Option<V3>,
    pub gaze_u: f64,
    pub viseme: Viseme,
    pub speaking: bool,
    pub blink: f64,
    pub t: f64,
}

impl ActorTimeline {
    pub fn new(name: &str, def: Arc<CharacterDef>) -> ActorTimeline {
        ActorTimeline {
            name: name.to_string(),
            def,
            init_pos: [0.0, 0.0],
            init_yaw: std::f64::consts::PI,
            init_sit: 0.0,
            init_seat_h: 0.62,
            moves: vec![],
            turns: vec![],
            sits: vec![],
            gestures: vec![],
            exprs: vec![],
            gazes: vec![],
            speech: vec![],
            blinks: vec![],
            physics: None,
        }
    }

    pub fn kin(&self, t: f64) -> Kin {
        let mut pos = self.init_pos;
        let mut odo = 0.0;
        let mut speed = 0.0;
        let mut running = false;
        for m in &self.moves {
            let len = dist2(m.from, m.to);
            if t >= m.t1 {
                pos = m.to;
                odo += len;
            } else if t > m.t0 {
                let u = (t - m.t0) / (m.t1 - m.t0).max(1e-6);
                let e = smooth(u);
                pos = [lerp(m.from[0], m.to[0], e), lerp(m.from[1], m.to[1], e)];
                odo += len * e;
                speed = len / (m.t1 - m.t0).max(1e-6) * 6.0 * u * (1.0 - u);
                running = m.run;
                break;
            } else {
                break;
            }
        }
        let mut yaw = self.init_yaw;
        for tn in &self.turns {
            if t >= tn.t1 {
                yaw = tn.from + tn.delta;
            } else if t > tn.t0 {
                let u = (t - tn.t0) / (tn.t1 - tn.t0).max(1e-6);
                yaw = tn.from + tn.delta * smooth(u);
                break;
            } else {
                break;
            }
        }
        let mut sit = self.init_sit;
        let mut seat_h = self.init_seat_h;
        for s in &self.sits {
            if t >= s.t1 {
                sit = s.to;
                seat_h = s.height;
            } else if t > s.t0 {
                let u = (t - s.t0) / (s.t1 - s.t0).max(1e-6);
                sit = lerp(s.from, s.to, smooth(u));
                seat_h = s.height;
                break;
            } else {
                break;
            }
        }
        Kin { pos, yaw, sit, seat_h, odo, speed, running }
    }

    pub fn gesture_at(&self, t: f64) -> Option<GestureState> {
        let g = self.gestures.iter().rev().find(|g| t >= g.t0 && t < g.t1)?;
        let ramp = 0.25;
        let mut w = ((t - g.t0) / ramp).clamp(0.0, 1.0);
        if g.t1.is_finite() && !g.hold {
            w = w.min(((g.t1 - t) / ramp).clamp(0.0, 1.0));
        }
        Some(GestureState { kind: g.kind, time: t - g.t0, weight: smooth(w), target: g.target })
    }

    pub fn expr_at(&self, t: f64) -> ExprParams {
        let params = |k: &ExprKey| {
            let e = expression(&k.name).unwrap_or(ExprParams::NEUTRAL);
            ExprParams::lerp(&ExprParams::NEUTRAL, &e, k.intensity)
        };
        let mut idx: Option<usize> = None;
        for (i, k) in self.exprs.iter().enumerate() {
            if k.t <= t {
                idx = Some(i);
            } else {
                break;
            }
        }
        match idx {
            None => ExprParams::NEUTRAL,
            Some(i) => {
                let cur = params(&self.exprs[i]);
                let prev = if i == 0 { ExprParams::NEUTRAL } else { params(&self.exprs[i - 1]) };
                let u = smooth((t - self.exprs[i].t) / 0.18);
                ExprParams::lerp(&prev, &cur, u)
            }
        }
    }

    /// (previous target, current target, blend 0..1)
    pub fn gaze_pair(&self, t: f64) -> (GazeTarget, GazeTarget, f64) {
        let mut idx: Option<usize> = None;
        for (i, k) in self.gazes.iter().enumerate() {
            if k.t <= t {
                idx = Some(i);
            } else {
                break;
            }
        }
        match idx {
            None => (GazeTarget::None, GazeTarget::None, 1.0),
            Some(i) => {
                let prev = if i == 0 { GazeTarget::None } else { self.gazes[i - 1].target.clone() };
                (prev, self.gazes[i].target.clone(), smooth((t - self.gazes[i].t) / 0.35))
            }
        }
    }

    pub fn viseme_at(&self, t: f64) -> (Viseme, bool) {
        for s in &self.speech {
            if t >= s.t0 && t < s.t1 {
                return (s.track.at(t - s.t0), true);
            }
        }
        (Viseme::REST, false)
    }

    pub fn blink_at(&self, t: f64) -> f64 {
        for &b in &self.blinks {
            let d = t - b;
            if (0.0..0.14).contains(&d) {
                return (std::f64::consts::PI * d / 0.14).sin();
            }
        }
        0.0
    }

    /// How much the body is raised when seated (seat height minus standing leg length).
    pub fn lift(&self, t: f64) -> f64 {
        let k = self.kin(t);
        k.sit * (k.seat_h + 0.03 - self.def.dims().leg)
    }

    /// Head center position at `t` (for gaze and framing).
    pub fn head_pos(&self, t: f64) -> V3 {
        let k = self.kin(t);
        let d = self.def.dims();
        let y = d.leg + d.torso_h + d.neck + d.head_r * 0.95 + self.lift(t);
        [k.pos[0], y, k.pos[1]]
    }
}

/// Deterministic blink schedule.
pub fn blink_schedule(name: &str, duration: f64) -> Vec<f64> {
    let mut s = hash_str(name) | 1;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        (s % 10_000) as f64 / 10_000.0
    };
    let mut t = 0.8 + next() * 1.5;
    let mut out = vec![];
    while t < duration {
        out.push(t);
        t += 2.0 + next() * 3.0;
    }
    out
}
