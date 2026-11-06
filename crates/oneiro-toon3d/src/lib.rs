//! The `toon_3d` style: real 3D characters (procedural meshes posed by the same skeleton as every
//! other style) shaded like a cartoon (flat bands, a toon highlight, ink outlines). Each character is
//! rendered into a transparent layer with a camera that matches the 2D set exactly, and the renderer
//! composites it in depth order.
//!
//! Two backends draw the same mesh: a software rasterizer (always available, identical everywhere) and,
//! when the `gpu` module finds an adapter, wgpu on the graphics card.

pub mod build;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod mesh;
pub mod props;
pub mod raster;

pub use build::{character_mesh, Built};
pub use mesh::{Mesh, Vtx};
pub use raster::{Camera, Style};

use oneiro_core::physics::Physics;
use oneiro_core::timeline::ActorFrame;
use oneiro_core::CharacterDef;
use tiny_skia::Pixmap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    /// The GPU when there is one (and `ONEIRO_3D` does not say otherwise), otherwise the CPU.
    Auto,
    /// The software rasterizer: identical pixels on every machine.
    Cpu,
    /// The GPU, falling back to the CPU if it is not available.
    Gpu,
}

impl Backend {
    /// `ONEIRO_3D=cpu|gpu|auto` overrides what the caller asked for.
    pub fn resolve(self) -> Backend {
        match std::env::var("ONEIRO_3D").ok().as_deref() {
            Some("cpu") => Backend::Cpu,
            Some("gpu") => Backend::Gpu,
            Some("auto") => Backend::Auto,
            _ => self,
        }
    }
}

/// Name of the graphics adapter used for 3D, or `None` when the CPU renderer is in use.
pub fn gpu_name() -> Option<String> {
    #[cfg(feature = "gpu")]
    {
        if Backend::Auto.resolve() == Backend::Cpu {
            return None;
        }
        return gpu::gpu().map(|g| g.adapter_name.clone());
    }
    #[allow(unreachable_code)]
    None
}

/// Renders one character into a `w x h` transparent layer seen through `cam`.
#[allow(clippy::too_many_arguments)]
pub fn render_layer(def: &CharacterDef, fr: &ActorFrame, phys: Option<&Physics>, cam: &Camera, w: u32, h: u32, style: &Style, backend: Backend) -> Pixmap {
    let built = character_mesh(def, fr, phys);
    render_mesh(&built.mesh, cam, w, h, style, backend)
}

pub fn render_mesh(mesh: &Mesh, cam: &Camera, w: u32, h: u32, style: &Style, backend: Backend) -> Pixmap {
    #[cfg(feature = "gpu")]
    {
        if backend.resolve() != Backend::Cpu {
            if let Some(g) = gpu::gpu() {
                if let Some(pm) = g.render(mesh, cam, w, h, style) {
                    return pm;
                }
            }
        }
    }
    #[cfg(not(feature = "gpu"))]
    let _ = backend;
    raster::render(mesh, cam, w, h, style)
}
