//! Shared wgpu helpers for multi-tab multi-core rendering (`TabPlayerGuard`), hardware opaque
//! game blitting (`GpuBlit` with `BlendState::REPLACE`), and zero-copy offscreen GPU rendering
//! (`GpuTextureTarget`, eliminating costly GPU->CPU staging readbacks and egui alpha-blending).

use ruffle_core::Player;
use ruffle_render_wgpu::target::{LaterSubmit, RenderTarget, RenderTargetFrame};
use std::borrow::Cow;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard};
use wgpu::util::DeviceExt;

const BLIT_WGSL: &str = r#"
struct VertexOutput {
    @location(0) tex_coord: vec2<f32>,
    @builtin(position) position: vec4<f32>,
};

fn srgb_to_linear(srgb: vec4<f32>) -> vec4<f32> {
    var rgb: vec3<f32> = srgb.rgb;
    if (srgb.a > 0.0) {
        rgb = rgb / srgb.a;
    }
    let a = rgb / 12.92;
    let b = pow((rgb + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    let c = step(vec3<f32>(0.04045), rgb);
    return vec4<f32>(mix(a, b, c) * srgb.a, srgb.a);
}

@vertex
fn vs_main(
    @location(0) a_pos: vec2<f32>,
    @location(1) a_tex_coord: vec2<f32>,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coord = a_tex_coord;
    out.position = vec4<f32>(a_pos, 0.0, 1.0);
    return out;
}

@group(0) @binding(0) var r_tex_color: texture_2d<f32>;
@group(0) @binding(1) var r_tex_sampler: sampler;

@fragment
fn fs_main_linear_framebuffer(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(r_tex_color, r_tex_sampler, in.tex_coord);
    return vec4<f32>(tex.rgb, 1.0);
}

@fragment
fn fs_main_srgb_framebuffer(in: VertexOutput) -> @location(0) vec4<f32> {
    let tex = textureSample(r_tex_color, r_tex_sampler, in.tex_coord);
    let lin = srgb_to_linear(tex);
    return vec4<f32>(lin.rgb, 1.0);
}
"#;

/// RAII guard that locks a tab's [`Player`] AND swaps `ruffle_render_wgpu::segments`'s
/// thread-local command-encoder queue to that tab's private storage.
pub struct TabPlayerGuard<'a> {
    player: MutexGuard<'a, Player>,
    segments_slot: &'a Mutex<Vec<wgpu::CommandEncoder>>,
    outer_segments: Vec<wgpu::CommandEncoder>,
}

impl<'a> TabPlayerGuard<'a> {
    pub fn lock(
        player: &'a Mutex<Player>,
        segments_slot: &'a Mutex<Vec<wgpu::CommandEncoder>>,
    ) -> Self {
        let guard = player.lock().unwrap_or_else(|e| e.into_inner());
        Self::from_guard(guard, segments_slot)
    }

    pub fn try_lock(
        player: &'a Mutex<Player>,
        segments_slot: &'a Mutex<Vec<wgpu::CommandEncoder>>,
    ) -> Option<Self> {
        let guard = player.try_lock().ok()?;
        Some(Self::from_guard(guard, segments_slot))
    }

    fn from_guard(
        player: MutexGuard<'a, Player>,
        segments_slot: &'a Mutex<Vec<wgpu::CommandEncoder>>,
    ) -> Self {
        let outer_segments = ruffle_render_wgpu::segments::take_pending();
        let tab_segments = std::mem::take(
            &mut *segments_slot
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner()),
        );
        ruffle_render_wgpu::segments::restore(tab_segments);
        Self {
            player,
            segments_slot,
            outer_segments,
        }
    }
}

impl Deref for TabPlayerGuard<'_> {
    type Target = Player;

    fn deref(&self) -> &Self::Target {
        &self.player
    }
}

impl DerefMut for TabPlayerGuard<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.player
    }
}

impl Drop for TabPlayerGuard<'_> {
    fn drop(&mut self) {
        let remaining = ruffle_render_wgpu::segments::take_pending();
        *self
            .segments_slot
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = remaining;
        if !self.outer_segments.is_empty() {
            ruffle_render_wgpu::segments::restore(std::mem::take(&mut self.outer_segments));
        }
    }
}

/// Hardware full-viewport quad blitter (`BlendState::REPLACE`, zero alpha blending).
pub struct GpuBlit {
    module: wgpu::ShaderModule,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    vertices: wgpu::Buffer,
}

impl std::fmt::Debug for GpuBlit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GpuBlit")
    }
}

impl GpuBlit {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("grimlite_blit"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(BLIT_WGSL)),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("grimlite_blit_bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let quad: [[f32; 4]; 6] = [
            [-1.0, 1.0, 0.0, 0.0],
            [1.0, 1.0, 1.0, 0.0],
            [1.0, -1.0, 1.0, 1.0],
            [1.0, -1.0, 1.0, 1.0],
            [-1.0, -1.0, 0.0, 1.0],
            [-1.0, 1.0, 0.0, 0.0],
        ];
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("grimlite_blit_quad"),
            contents: bytemuck::cast_slice(&quad),
            usage: wgpu::BufferUsages::VERTEX,
        });
        Self {
            module,
            layout,
            sampler,
            vertices,
        }
    }

    pub fn create_pipeline(
        &self,
        device: &wgpu::Device,
        surface_format: wgpu::TextureFormat,
    ) -> wgpu::RenderPipeline {
        let pipe_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("grimlite_blit_pl"),
            bind_group_layouts: &[Some(&self.layout)],
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("grimlite_blit_pipeline"),
            layout: Some(&pipe_layout),
            vertex: wgpu::VertexState {
                module: &self.module,
                entry_point: Some("vs_main"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 16,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2],
                })],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.module,
                entry_point: Some(if surface_format.is_srgb() {
                    "fs_main_srgb_framebuffer"
                } else {
                    "fs_main_linear_framebuffer"
                }),
                targets: &[Some(wgpu::ColorTargetState {
                    format: surface_format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    pub fn bind_group(&self, device: &wgpu::Device, view: &wgpu::TextureView) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("grimlite_blit_bg"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    pub fn draw(
        &self,
        pipeline: &wgpu::RenderPipeline,
        bind_group: &wgpu::BindGroup,
        pass: &mut wgpu::RenderPass<'static>,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertices.slice(..));
        pass.draw(0..6, 0..1);
    }
}

/// Pure-GPU offscreen [`RenderTarget`] with **zero** CPU staging buffer and direct [`GpuBlit`]
/// bind group (`BlendState::REPLACE`, zero alpha-blending).
#[derive(Debug)]
pub struct GpuTextureTarget {
    blit: Arc<GpuBlit>,
    texture: wgpu::Texture,
    pub bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

#[derive(Debug)]
pub struct GpuTextureFrame(wgpu::TextureView);

impl RenderTargetFrame for GpuTextureFrame {
    fn into_view(self) -> wgpu::TextureView {
        self.0
    }

    fn view(&self) -> &wgpu::TextureView {
        &self.0
    }
}

impl GpuTextureTarget {
    pub fn new(blit: Arc<GpuBlit>, device: &wgpu::Device, width: u32, height: u32) -> Self {
        let width = width.max(1);
        let height = height.max(1);
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("grimlite_gpu_target"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = blit.bind_group(device, &view);
        Self {
            blit,
            texture,
            bind_group,
            width,
            height,
        }
    }
}

impl RenderTarget for GpuTextureTarget {
    type Frame = GpuTextureFrame;

    fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let width = width.max(1);
        let height = height.max(1);
        if self.width == width && self.height == height {
            return;
        }
        *self = Self::new(self.blit.clone(), device, width, height);
    }

    fn format(&self) -> wgpu::TextureFormat {
        wgpu::TextureFormat::Rgba8Unorm
    }

    fn width(&self) -> u32 {
        self.width
    }

    fn height(&self) -> u32 {
        self.height
    }

    fn get_next_texture(&mut self) -> Option<Self::Frame> {
        Some(GpuTextureFrame(
            self.texture
                .create_view(&wgpu::TextureViewDescriptor::default()),
        ))
    }

    fn submit<I: IntoIterator<Item = wgpu::CommandBuffer>>(
        &self,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        command_buffers: I,
        _frame: Self::Frame,
    ) -> wgpu::SubmissionIndex {
        queue.submit(command_buffers)
    }

    fn submit_later(&self, _frame: Self::Frame) -> Result<LaterSubmit, Self::Frame> {
        Ok(Box::new(|queue, command_buffers| {
            queue.submit(command_buffers)
        }))
    }
}
