//! AI integration for Oneiro: pluggable providers (agent CLIs under the user's own login, Anthropic
//! API, OpenAI-compatible/local servers), the validating "director" loop, the guide the models
//! read, and the tool set that the MCP server exposes to external agents.

pub mod b64;
pub mod config;
pub mod cutter_ai;
pub mod director;
pub mod guide;
pub mod mcp;
pub mod parts;
pub mod providers;
pub mod tools;
pub mod types;

pub use config::{AiConfig, ProviderKind};
pub use director::{direct_character, direct_scene, CharacterResult, DirectorOpts, ProjectCtx, SceneResult, Turn};
pub use parts::{direct_character_parts, validate_parts, PartsResult};
pub use types::{ChatRequest, ChatResponse, Message, Provider, Role};
