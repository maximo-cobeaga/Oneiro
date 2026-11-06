//! The GPU backend (wgpu): the same meshes and the same cel shading as the CPU rasterizer, drawn with
//! 4x multisampling into an off-screen target and read back. Headless: it never opens a window. If there
//! is no adapter, `gpu()` returns `None` and the CPU rasterizer is used instead.

use crate::mesh::{Mesh, Vtx};
use crate::raster::{Camera, Style, INK};
use std::sync::{Mutex, OnceLock};
use tiny_skia::Pixmap;

const SHADER: &str = r#"
struct U {
    cam: vec4<f32>,
    right: vec4<f32>,
    up: vec4<f32>,
    fwd: vec4<f32>,
    light: vec4<f32>,
    tint: vec4<f32>,
    // f, cx, cy, outline_px
    lens: vec4<f32>,
    // w, h, depth A, depth B
    size: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: U;

struct VIn {
    @location(0) p: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) c: vec3<f32>,
};
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) wp: vec3<f32>,
    @location(1) n: vec3<f32>,
    @location(2) c: vec3<f32>,
};

fn project(p: vec3<f32>) -> vec4<f32> {
    let rel = p - u.cam.xyz;
    let xc = dot(rel, u.right.xyz);
    let yc = dot(rel, u.up.xyz);
    let zc = dot(rel, u.fwd.xyz);
    let f = u.lens.x;
    let x = (f * xc + (u.lens.y - u.size.x * 0.5) * zc) * (2.0 / u.size.x);
    let y = (f * yc - (u.lens.z - u.size.y * 0.5) * zc) * (2.0 / u.size.y);
    let z = u.size.z * zc + u.size.w;
    return vec4<f32>(x, y, z, zc);
}

@vertex
fn vs_main(v: VIn) -> VOut {
    var o: VOut;
    o.pos = project(v.p);
    o.wp = v.p;
    o.n = v.n;
    o.c = v.c;
    return o;
}

@vertex
fn vs_outline(v: VIn) -> VOut {
    var o: VOut;
    let zc = max(dot(v.p - u.cam.xyz, u.fwd.xyz), 0.05);
    let p2 = v.p + v.n * (u.lens.w * zc / u.lens.x);
    o.pos = project(p2);
    o.wp = p2;
    o.n = v.n;
    o.c = v.c;
    return o;
}

fn band(ndl: f32) -> f32 {
    return 0.58 + 0.22 * smoothstep(-0.12, -0.02, ndl) + 0.20 * smoothstep(0.22, 0.30, ndl);
}

@fragment
fn fs_main(i: VOut) -> @location(0) vec4<f32> {
    let n = normalize(i.n);
    let view = normalize(u.cam.xyz - i.wp);
    let l = u.light.xyz;
    let ndl = dot(n, l);
    let h = normalize(l + view);
    let spec = smoothstep(0.985, 0.992, dot(n, h)) * 0.16;
    let rim = smoothstep(0.62, 0.70, 1.0 - max(dot(n, view), 0.0)) * 0.10 * smoothstep(0.0, 0.3, ndl);
    let col = clamp(i.c * band(ndl) * u.tint.xyz + vec3<f32>(spec + rim), vec3<f32>(0.0), vec3<f32>(1.0));
    return vec4<f32>(col, 1.0);
}

@fragment
fn fs_outline(i: VOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.07, 0.06, 0.08, 1.0);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    cam: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
    fwd: [f32; 4],
    light: [f32; 4],
    tint: [f32; 4],
    lens: [f32; 4],
    size: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    p: [f32; 3],
    n: [f32; 3],
    c: [f32; 3],
}

pub struct Gpu {
    device: wgpu::Device,
    queue: wgpu::Queue,
    main: wgpu::RenderPipeline,
    outline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    pub adapter_name: String,
    /// Serializes use of the device: frames are rendered from many threads.
    cache: Mutex<Cache>,
}

const SAMPLES: u32 = 4;
const COLOR: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

fn init() -> Result<Gpu, String> {
    let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions { power_preference: wgpu::PowerPreference::HighPerformance, compatible_surface: None, force_fallback_adapter: false, apply_limit_buckets: false }))
        .map_err(|e| format!("no graphics adapter: {}", e))?;
    let info = adapter.get_info();
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).map_err(|e| format!("cannot open the graphics device: {}", e))?;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("toon3d"), source: wgpu::ShaderSource::Wgsl(SHADER.into()) });
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: None,
        entries: &[wgpu::BindGroupLayoutEntry { binding: 0, visibility: wgpu::ShaderStages::VERTEX_FRAGMENT, ty: wgpu::BindingType::Buffer { ty: wgpu::BufferBindingType::Uniform, has_dynamic_offset: false, min_binding_size: None }, count: None }],
    });
    let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor { label: None, bind_group_layouts: &[Some(&layout)], immediate_size: 0 });
    let attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3];
    let vbuf = wgpu::VertexBufferLayout { array_stride: std::mem::size_of::<GpuVertex>() as u64, step_mode: wgpu::VertexStepMode::Vertex, attributes: &attrs };
    let make = |vs: &str, fs: &str, cull: wgpu::Face, depth_cmp: wgpu::CompareFunction| {
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: None,
            layout: Some(&pl),
            vertex: wgpu::VertexState { module: &shader, entry_point: Some(vs), compilation_options: Default::default(), buffers: &[Some(vbuf.clone())] },
            fragment: Some(wgpu::FragmentState { module: &shader, entry_point: Some(fs), compilation_options: Default::default(), targets: &[Some(wgpu::ColorTargetState { format: COLOR, blend: None, write_mask: wgpu::ColorWrites::ALL })] }),
            // Triangles facing the camera are clockwise on screen (the world is left-handed to the view).
            primitive: wgpu::PrimitiveState { topology: wgpu::PrimitiveTopology::TriangleList, front_face: wgpu::FrontFace::Cw, cull_mode: Some(cull), ..Default::default() },
            depth_stencil: Some(wgpu::DepthStencilState { format: DEPTH, depth_write_enabled: Some(true), depth_compare: Some(depth_cmp), stencil: Default::default(), bias: Default::default() }),
            multisample: wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false },
            multiview_mask: None,
            cache: None,
        })
    };
    let main = make("vs_main", "fs_main", wgpu::Face::Back, wgpu::CompareFunction::LessEqual);
    let outline = make("vs_outline", "fs_outline", wgpu::Face::Front, wgpu::CompareFunction::Less);
    Ok(Gpu { device, queue, main, outline, layout, adapter_name: format!("{} ({:?})", info.name, info.backend), cache: Mutex::new(Cache::default()) })
}

/// The shared GPU, opened on first use. `None` when this machine has no usable adapter.
pub fn gpu() -> Option<&'static Gpu> {
    static GPU: OnceLock<Option<Gpu>> = OnceLock::new();
    GPU.get_or_init(|| match init() {
        Ok(g) => Some(g),
        Err(e) => {
            eprintln!("oneiro: {} - using the CPU renderer for 3D", e);
            None
        }
    })
    .as_ref()
}

/// Off-screen render target of one size, reused between frames.
struct Target {
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve_tex: wgpu::Texture,
    resolve: wgpu::TextureView,
    readback: wgpu::Buffer,
    row: u32,
}

/// Everything that is reused between calls (guarded by the device lock).
#[derive(Default)]
pub struct Cache {
    targets: std::collections::HashMap<(u32, u32), Target>,
    vertex: Option<(wgpu::Buffer, u64)>,
    index: Option<(wgpu::Buffer, u64)>,
    uniform: Option<(wgpu::Buffer, wgpu::BindGroup)>,
}

impl Gpu {
    fn target(&self, cache: &mut Cache, tw: u32, th: u32) {
        if cache.targets.len() > 12 && !cache.targets.contains_key(&(tw, th)) {
            cache.targets.clear();
        }
        cache.targets.entry((tw, th)).or_insert_with(|| {
            let dev = &self.device;
            let extent = wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 };
            let tex = |format, samples, usage| dev.create_texture(&wgpu::TextureDescriptor { label: None, size: extent, mip_level_count: 1, sample_count: samples, dimension: wgpu::TextureDimension::D2, format, usage, view_formats: &[] });
            let msaa = tex(COLOR, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let depth = tex(DEPTH, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
            let resolve_tex = tex(COLOR, 1, wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC);
            let row = ((tw * 4 + 255) / 256) * 256;
            let readback = dev.create_buffer(&wgpu::BufferDescriptor { label: None, size: (row * th) as u64, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
            Target { msaa: msaa.create_view(&Default::default()), depth: depth.create_view(&Default::default()), resolve: resolve_tex.create_view(&Default::default()), resolve_tex, readback, row }
        });
    }

    /// A buffer of at least `bytes` bytes (grown in powers of two), reused between frames.
    fn grow(&self, slot: &mut Option<(wgpu::Buffer, u64)>, bytes: u64, usage: wgpu::BufferUsages) {
        if slot.as_ref().map_or(true, |(_, cap)| *cap < bytes) {
            let cap = bytes.next_power_of_two().max(4096);
            *slot = Some((self.device.create_buffer(&wgpu::BufferDescriptor { label: None, size: cap, usage: usage | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false }), cap));
        }
    }

    /// Renders `mesh` into a transparent premultiplied RGBA image.
    pub fn render(&self, mesh: &Mesh, cam: &Camera, w: u32, h: u32, style: &Style) -> Option<Pixmap> {
        if w == 0 || h == 0 || w > 8192 || h > 8192 {
            return None;
        }
        let mut guard = self.cache.lock().ok()?;
        let cache = &mut *guard;
        let dev = &self.device;
        // Targets are shared by every layer of about the same size (rounded up to 64 pixels); the picture is
        // cropped from the top-left corner, which is where the pixel coordinates of the camera start.
        let (tw, th) = ((w + 63) / 64 * 64, (h + 63) / 64 * 64);
        let (near, far) = (0.05f32, 200.0f32);
        let (a, b) = (far / (far - near), -far * near / (far - near));
        let f4 = |v: [f64; 3]| [v[0] as f32, v[1] as f32, v[2] as f32, 0.0];
        let l = cam.light();
        let u = Uniforms {
            cam: f4(cam.pos),
            right: f4(cam.right),
            up: f4(cam.up),
            fwd: f4(cam.fwd),
            light: [l[0], l[1], l[2], 0.0],
            tint: [style.tint[0], style.tint[1], style.tint[2], 1.0],
            lens: [cam.f as f32, cam.cx as f32, cam.cy as f32, style.outline_px],
            size: [tw as f32, th as f32, a, b],
        };
        let _ = INK;
        if cache.uniform.is_none() {
            let ubuf = dev.create_buffer(&wgpu::BufferDescriptor { label: None, size: std::mem::size_of::<Uniforms>() as u64, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST, mapped_at_creation: false });
            let bind = dev.create_bind_group(&wgpu::BindGroupDescriptor { label: None, layout: &self.layout, entries: &[wgpu::BindGroupEntry { binding: 0, resource: ubuf.as_entire_binding() }] });
            cache.uniform = Some((ubuf, bind));
        }
        self.queue.write_buffer(&cache.uniform.as_ref()?.0, 0, bytemuck::bytes_of(&u));
        if !mesh.is_empty() {
            let verts: Vec<GpuVertex> = mesh.v.iter().map(|v: &Vtx| GpuVertex { p: v.p, n: v.n, c: v.c }).collect();
            self.grow(&mut cache.vertex, (verts.len() * std::mem::size_of::<GpuVertex>()) as u64, wgpu::BufferUsages::VERTEX);
            self.queue.write_buffer(&cache.vertex.as_ref()?.0, 0, bytemuck::cast_slice(&verts));
            self.grow(&mut cache.index, (mesh.i.len() * 4) as u64, wgpu::BufferUsages::INDEX);
            self.queue.write_buffer(&cache.index.as_ref()?.0, 0, bytemuck::cast_slice(&mesh.i));
        }
        self.target(cache, tw, th);
        let target = cache.targets.get(&(tw, th))?;
        let mut enc = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &target.msaa, depth_slice: None, resolve_target: Some(&target.resolve), ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT), store: wgpu::StoreOp::Discard } })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment { view: &target.depth, depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }), stencil_ops: None }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if !mesh.is_empty() {
                let (vb, ib) = (&cache.vertex.as_ref()?.0, &cache.index.as_ref()?.0);
                pass.set_bind_group(0, &cache.uniform.as_ref()?.1, &[]);
                pass.set_vertex_buffer(0, vb.slice(..));
                pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
                if style.outline_px > 0.0 {
                    pass.set_pipeline(&self.outline);
                    pass.draw_indexed(0..mesh.i.len() as u32, 0, 0..1);
                }
                pass.set_pipeline(&self.main);
                pass.draw_indexed(0..mesh.i.len() as u32, 0, 0..1);
            }
        }
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo { texture: &target.resolve_tex, mip_level: 0, origin: wgpu::Origin3d::ZERO, aspect: wgpu::TextureAspect::All },
            wgpu::TexelCopyBufferInfo { buffer: &target.readback, layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(target.row), rows_per_image: Some(th) } },
            wgpu::Extent3d { width: tw, height: th, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        let slice = target.readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        let _ = dev.poll(wgpu::PollType::wait_indefinitely());
        rx.recv().ok()?.ok()?;
        let data = slice.get_mapped_range().ok()?;
        let mut out = Pixmap::new(w, h)?;
        for y in 0..h as usize {
            let src = &data[y * target.row as usize..y * target.row as usize + w as usize * 4];
            out.data_mut()[y * w as usize * 4..(y + 1) * w as usize * 4].copy_from_slice(src);
        }
        drop(data);
        target.readback.unmap();
        Some(out)
    }
}
