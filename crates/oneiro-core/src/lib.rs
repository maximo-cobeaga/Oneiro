//! Oneiro core: world model, library/project loading, script compiler, solver and timelines.

pub mod camera;
pub mod catalog;
pub mod compile;
pub mod defs;
pub mod lock;
pub mod math;
pub mod nav;
pub mod obj;
pub mod physics;
pub mod pose;
pub mod project;
pub mod raster;
pub mod svg;
pub mod timeline;

pub use camera::{CameraPose, CameraSeg};
pub use compile::{compile, compile_source, AudioEvent, AudioKind, Episode, Scene, StateLog, StmtSpan, Transition};
pub use defs::{Anchor, CharacterDef, Dims, PartDef, PropInst, SetDef, SvgPart, SvgRig, Toon3d};
pub use math::Rgb;
pub use project::{Manifest, Project};
pub use timeline::{ActorFrame, ActorTimeline};
