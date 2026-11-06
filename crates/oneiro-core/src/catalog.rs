//! Built-in vocabulary and parts catalog. This is the "global library" shipped with Oneiro.

use crate::math::Rgb;

// ------------------------------------------------------------------ parts
pub const HEADS: &[&str] = &["round", "square", "oval"];
pub const EYES: &[&str] = &["dots", "ovals", "wide"];
pub const MOUTHS: &[&str] = &["visemes_basic"];
pub const BODIES: &[&str] = &["kid", "adult", "round"];
pub const RIGS: &[&str] = &["biped_cutout"];
pub const PROP_KINDS: &[&str] = &["counter", "stool", "table", "wall_clock", "window", "door", "pie_case"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Slot {
    Hat,
    Top,
    Bottom,
    Shoes,
    Accessory,
}

pub struct WardrobeItem {
    pub name: &'static str,
    pub slot: Slot,
    pub color: Rgb,
    pub accent: Rgb,
}

pub const WARDROBE: &[WardrobeItem] = &[
    WardrobeItem { name: "red_beanie", slot: Slot::Hat, color: Rgb(208, 48, 47), accent: Rgb(242, 209, 74) },
    WardrobeItem { name: "green_hat", slot: Slot::Hat, color: Rgb(46, 139, 87), accent: Rgb(28, 90, 55) },
    WardrobeItem { name: "yellow_hat", slot: Slot::Hat, color: Rgb(240, 200, 60), accent: Rgb(200, 150, 30) },
    WardrobeItem { name: "chef_hat", slot: Slot::Hat, color: Rgb(250, 250, 246), accent: Rgb(210, 210, 205) },
    WardrobeItem { name: "blue_coat", slot: Slot::Top, color: Rgb(45, 93, 168), accent: Rgb(27, 58, 112) },
    WardrobeItem { name: "yellow_jacket", slot: Slot::Top, color: Rgb(227, 181, 42), accent: Rgb(170, 130, 20) },
    WardrobeItem { name: "orange_parka", slot: Slot::Top, color: Rgb(232, 118, 28), accent: Rgb(170, 80, 15) },
    WardrobeItem { name: "green_sweater", slot: Slot::Top, color: Rgb(63, 155, 74), accent: Rgb(40, 110, 50) },
    WardrobeItem { name: "brown_jacket", slot: Slot::Top, color: Rgb(122, 75, 42), accent: Rgb(85, 50, 25) },
    WardrobeItem { name: "apron_white", slot: Slot::Top, color: Rgb(244, 244, 240), accent: Rgb(200, 200, 195) },
    WardrobeItem { name: "jeans", slot: Slot::Bottom, color: Rgb(43, 63, 115), accent: Rgb(30, 45, 85) },
    WardrobeItem { name: "brown_pants", slot: Slot::Bottom, color: Rgb(91, 61, 36), accent: Rgb(65, 42, 24) },
    WardrobeItem { name: "gray_pants", slot: Slot::Bottom, color: Rgb(110, 110, 118), accent: Rgb(80, 80, 88) },
    WardrobeItem { name: "black_shoes", slot: Slot::Shoes, color: Rgb(30, 30, 34), accent: Rgb(60, 60, 66) },
    WardrobeItem { name: "red_scarf", slot: Slot::Accessory, color: Rgb(190, 40, 50), accent: Rgb(140, 25, 35) },
];

pub fn wardrobe_item(name: &str) -> Option<&'static WardrobeItem> {
    WARDROBE.iter().find(|w| w.name == name)
}

pub fn wardrobe_names() -> Vec<&'static str> {
    WARDROBE.iter().map(|w| w.name).collect()
}

// ------------------------------------------------------------------ vocabulary
pub const SHOTS: &[&str] = &[
    "extreme_wide",
    "wide",
    "establishing",
    "full",
    "medium",
    "medium_close",
    "close",
    "extreme_close",
    "over_shoulder",
];
pub const ANGLES: &[&str] = &["eye_level", "low", "high", "dutch", "top_down"];
pub const MOVES: &[&str] = &[
    "static",
    "pan",
    "pan_left",
    "pan_right",
    "tilt",
    "tilt_up",
    "tilt_down",
    "dolly_in",
    "dolly_out",
    "truck_left",
    "truck_right",
    "orbit",
    "crane_up",
    "handheld",
];
pub const SIDES: &[&str] = &["north", "south", "east", "west", "left", "right", "front", "back"];
pub const TIMES: &[&str] = &["day", "dusk", "night"];
pub const RELATIONS: &[&str] = &["at", "beside", "left_of", "right_of", "in_front_of", "behind", "near"];
pub const PLANNED_RELATIONS: &[&str] = &["on", "inside"];

pub const IMPLEMENTED_VERBS: &[&str] = &[
    "walks_to",
    "runs_to",
    "turns_to",
    "stops",
    "sits_on",
    "stands",
    "waves",
    "points_at",
    "shrugs",
    "nods",
    "shakes_head",
    "crosses_arms",
    "uncrosses_arms",
    "facepalms",
    "looks_at",
    "looks_away",
    "emote",
];
pub const PLANNED_VERBS: &[&str] = &[
    "walks_along",
    "follows",
    "lies_on",
    "crouches",
    "leans_on",
    "stares_at",
    "picks_up",
    "drops",
    "hands",
    "opens",
    "closes",
    "pushes",
    "hits",
];

pub const MANNER_WORDS: &[&str] = &["slowly", "quickly", "calmly", "angrily", "happily", "nervously", "sadly"];

// ------------------------------------------------------------------ gestures
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gesture {
    Wave,
    Point,
    Shrug,
    Nod,
    ShakeHead,
    CrossArms,
    FacePalm,
}

/// verb -> (gesture, default duration, holds until the next gesture)
pub fn gesture_for_verb(v: &str) -> Option<(Gesture, f64, bool)> {
    Some(match v {
        "waves" => (Gesture::Wave, 1.6, false),
        "points_at" => (Gesture::Point, 1.3, false),
        "shrugs" => (Gesture::Shrug, 1.0, false),
        "nods" => (Gesture::Nod, 0.9, false),
        "shakes_head" => (Gesture::ShakeHead, 1.0, false),
        "crosses_arms" => (Gesture::CrossArms, 0.7, true),
        "facepalms" => (Gesture::FacePalm, 1.5, false),
        _ => return None,
    })
}

// ------------------------------------------------------------------ expressions
#[derive(Clone, Copy, Debug)]
pub struct ExprParams {
    /// Eyebrow tilt in degrees; positive = angry (inner ends down).
    pub brow_angle: f64,
    /// Eyebrow height, -1..1.
    pub brow_y: f64,
    /// Eye openness multiplier.
    pub eye_open: f64,
    /// Mouth curve, -1 (frown) .. 1 (smile).
    pub smile: f64,
    /// Extra mouth opening while speaking.
    pub mouth_bias: f64,
    /// Sweat drop amount.
    pub sweat: f64,
}

impl ExprParams {
    pub const NEUTRAL: ExprParams =
        ExprParams { brow_angle: 0.0, brow_y: 0.0, eye_open: 1.0, smile: 0.0, mouth_bias: 0.0, sweat: 0.0 };

    pub fn lerp(a: &ExprParams, b: &ExprParams, t: f64) -> ExprParams {
        let l = |x: f64, y: f64| x + (y - x) * t;
        ExprParams {
            brow_angle: l(a.brow_angle, b.brow_angle),
            brow_y: l(a.brow_y, b.brow_y),
            eye_open: l(a.eye_open, b.eye_open),
            smile: l(a.smile, b.smile),
            mouth_bias: l(a.mouth_bias, b.mouth_bias),
            sweat: l(a.sweat, b.sweat),
        }
    }
}

pub const EXPRESSIONS: &[&str] = &[
    "neutral",
    "happy",
    "sad",
    "angry",
    "annoyed",
    "surprised",
    "nervous",
    "bored",
    "confused",
    "scared",
    "laughing",
    "determined",
];

pub fn expression(name: &str) -> Option<ExprParams> {
    let p = |brow_angle, brow_y, eye_open, smile, mouth_bias, sweat| ExprParams {
        brow_angle,
        brow_y,
        eye_open,
        smile,
        mouth_bias,
        sweat,
    };
    Some(match name {
        "neutral" => ExprParams::NEUTRAL,
        "happy" => p(-4.0, 0.3, 0.9, 0.8, 0.0, 0.0),
        "sad" => p(-18.0, 0.1, 0.7, -0.6, 0.0, 0.0),
        "angry" => p(24.0, -0.4, 0.75, -0.5, 0.0, 0.0),
        "annoyed" => p(14.0, -0.2, 0.55, -0.3, 0.0, 0.0),
        "surprised" => p(0.0, 1.0, 1.35, 0.0, 0.3, 0.0),
        "nervous" => p(-14.0, 0.4, 1.1, -0.2, 0.0, 1.0),
        "bored" => p(0.0, -0.3, 0.45, -0.1, 0.0, 0.0),
        "confused" => p(-8.0, 0.5, 1.0, -0.2, 0.0, 0.0),
        "scared" => p(-20.0, 0.8, 1.4, -0.5, 0.2, 1.0),
        "laughing" => p(-4.0, 0.4, 0.5, 1.0, 0.5, 0.0),
        "determined" => p(12.0, -0.3, 0.9, 0.1, 0.0, 0.0),
        _ => return None,
    })
}
