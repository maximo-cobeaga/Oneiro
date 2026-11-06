//! Oneiro core: world model, library/project loading, script compiler, solver and timelines.

pub mod camera;
pub mod catalog;
pub mod compile;
pub mod defs;
pub mod lock;
pub mod math;
pub mod project;
pub mod timeline;

pub use camera::{CameraPose, CameraSeg};
pub use compile::{compile, compile_source, AudioEvent, AudioKind, Episode, Scene, StateLog, StmtSpan};
pub use defs::{Anchor, CharacterDef, Dims, PartDef, PropInst, SetDef};
pub use math::Rgb;
pub use project::{Manifest, Project};
pub use timeline::{ActorFrame, ActorTimeline};
