//! Multi-viewport manager for `grimlite-ri` desktop (`winit` + `egui_winit` + `egui_wgpu`).
//!
//! Enables `egui::Context::show_viewport_immediate` so Bot Manager and Tools windows open as real
//! native OS windows on macOS, Windows, and Linux and can be dragged freely outside the main game
//! window. Platforms without multi-viewport support (e.g. Android) fall back automatically to
//! embedded in-canvas `egui::Window`s.

use egui::{Context, ImmediateViewport, ViewportId};
use ruffle_render_wgpu::descriptors::Descriptors;
use ruffle_render_wgpu::wgpu;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::window::{Window, WindowId};

struct ChildViewport {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    egui_winit: egui_winit::State,
    used_this_frame: bool,
    close_requested: bool,
}

struct ViewportStore {
    descriptors: Option<Arc<Descriptors>>,
    egui_renderer: Option<Arc<Mutex<egui_wgpu::Renderer>>>,
    surface_format: Option<wgpu::TextureFormat>,
    children: HashMap<ViewportId, ChildViewport>,
    window_to_viewport: HashMap<WindowId, ViewportId>,
    active_event_loop: *const ActiveEventLoop,
}

impl Default for ViewportStore {
    fn default() -> Self {
        Self {
            descriptors: None,
            egui_renderer: None,
            surface_format: None,
            children: HashMap::new(),
            window_to_viewport: HashMap::new(),
            active_event_loop: std::ptr::null(),
        }
    }
}

thread_local! {
    static VIEWPORTS: RefCell<ViewportStore> = RefCell::new(ViewportStore::default());
}

struct EventLoopGuard;

impl Drop for EventLoopGuard {
    fn drop(&mut self) {
        VIEWPORTS.with(|store| {
            if let Ok(mut s) = store.try_borrow_mut() {
                s.active_event_loop = std::ptr::null();
            }
        });
    }
}

/// Installs the native immediate-viewport renderer on `egui_ctx`.
pub fn install(
    egui_ctx: &Context,
    descriptors: Arc<Descriptors>,
    egui_renderer: Arc<Mutex<egui_wgpu::Renderer>>,
    surface_format: wgpu::TextureFormat,
) {
    VIEWPORTS.with(|store| {
        let mut s = store.borrow_mut();
        s.descriptors = Some(descriptors);
        s.egui_renderer = Some(egui_renderer);
        s.surface_format = Some(surface_format);
    });
    egui_ctx.set_embed_viewports(false);
    Context::set_immediate_viewport_renderer(render_immediate_viewport);
}

/// Runs `f` with `event_loop` temporarily available for creating new child OS windows,
/// and automatically closes any child windows that were not requested during this frame.
pub fn run_frame<R>(event_loop: &ActiveEventLoop, f: impl FnOnce() -> R) -> R {
    VIEWPORTS.with(|store| {
        let mut s = store.borrow_mut();
        s.active_event_loop = event_loop as *const ActiveEventLoop;
        for child in s.children.values_mut() {
            child.used_this_frame = false;
        }
    });
    let _guard = EventLoopGuard;
    let out = f();
    VIEWPORTS.with(|store| {
        let mut s = store.borrow_mut();
        let stale: Vec<ViewportId> = s
            .children
            .iter()
            .filter(|(_, c)| !c.used_this_frame)
            .map(|(id, _)| *id)
            .collect();
        for id in stale {
            if let Some(child) = s.children.remove(&id) {
                s.window_to_viewport.remove(&child.window.id());
            }
        }
    });
    out
}

/// Routes a `winit::WindowEvent` to a detached child viewport if `window_id` belongs to one.
/// Returns `true` if handled by a child viewport.
pub fn handle_window_event(window_id: WindowId, event: &WindowEvent) -> bool {
    VIEWPORTS.with(|store| {
        let Ok(mut s) = store.try_borrow_mut() else {
            return false;
        };
        let Some(&vp_id) = s.window_to_viewport.get(&window_id) else {
            return false;
        };
        let descriptors = s.descriptors.clone();
        let Some(child) = s.children.get_mut(&vp_id) else {
            return false;
        };

        if matches!(event, WindowEvent::CloseRequested) {
            child.close_requested = true;
        }

        if let WindowEvent::Resized(size) = event
            && size.width > 0
            && size.height > 0
            && let Some(desc) = descriptors.as_ref()
        {
            child.surface_config.width = size.width;
            child.surface_config.height = size.height;
            child
                .surface
                .configure(&desc.device, &child.surface_config);
        }

        let _ = child.egui_winit.on_window_event(&child.window, event);
        true
    })
}

pub fn has_open_viewports() -> bool {
    VIEWPORTS.with(|store| !store.borrow().children.is_empty())
}

/// Drops all detached child windows cleanly before the graphics device shuts down.
pub fn clear() {
    VIEWPORTS.with(|store| {
        let mut s = store.borrow_mut();
        s.children.clear();
        s.window_to_viewport.clear();
        s.descriptors = None;
        s.egui_renderer = None;
    });
}

fn render_immediate_viewport(ctx: &Context, mut viewport: ImmediateViewport<'_>) {
    let vp_id = viewport.ids.this;

    // 1. Extract an existing child viewport or gather setup info, then release the RefCell borrow
    //    BEFORE calling `egui_winit::create_window` (on macOS, AppKit fires synchronous WindowEvents
    //    inside `create_window`, which re-enter `handle_window_event`).
    let existing = VIEWPORTS.with(|store| {
        let mut s = store.borrow_mut();
        let descriptors = s.descriptors.clone()?;
        let egui_renderer = s.egui_renderer.clone()?;
        let surface_format = s.surface_format?;
        let event_loop_ptr = s.active_event_loop;
        let child = s.children.remove(&vp_id);
        Some((child, descriptors, egui_renderer, surface_format, event_loop_ptr))
    });

    let Some((existing_child, descriptors, egui_renderer, surface_format, event_loop_ptr)) = existing
    else {
        return;
    };

    let mut child = match existing_child {
        Some(mut c) => {
            c.used_this_frame = true;
            c
        }
        None => {
            if event_loop_ptr.is_null() {
                return;
            }
            // SAFETY: `active_event_loop` is set strictly inside `run_frame` on this main thread
            // and cleared by `EventLoopGuard` before `run_frame` returns.
            let event_loop = unsafe { &*event_loop_ptr };
            let window = match egui_winit::create_window(ctx, event_loop, &viewport.builder) {
                Ok(w) => Arc::new(w),
                Err(e) => {
                    tracing::error!("Failed to create child viewport window: {e}");
                    return;
                }
            };
            let surface = match descriptors.wgpu_instance.create_surface(window.clone()) {
                Ok(surf) => surf,
                Err(e) => {
                    tracing::error!("Failed to create child viewport surface: {e}");
                    return;
                }
            };
            let size: PhysicalSize<u32> = window.inner_size();
            let surface_config = wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: surface_format,
                color_space: wgpu::SurfaceColorSpace::Auto,
                width: size.width.max(1),
                height: size.height.max(1),
                present_mode: Default::default(),
                desired_maximum_frame_latency: 2,
                alpha_mode: Default::default(),
                view_formats: Default::default(),
            };
            surface.configure(&descriptors.device, &surface_config);

            let mut egui_winit = egui_winit::State::new(
                ctx.clone(),
                vp_id,
                window.as_ref(),
                None,
                None,
                None,
            );
            egui_winit.set_max_texture_side(descriptors.limits.max_texture_dimension_2d as usize);

            let win_id = window.id();
            VIEWPORTS.with(|store| {
                store.borrow_mut().window_to_viewport.insert(win_id, vp_id);
            });

            ChildViewport {
                window,
                surface,
                surface_config,
                egui_winit,
                used_this_frame: true,
                close_requested: false,
            }
        }
    };

    let c = &mut child;

    // 2. Run egui UI for this child viewport
    let was_close_requested = std::mem::take(&mut c.close_requested);
    let mut raw_input = c.egui_winit.take_egui_input(&c.window);
    if was_close_requested {
        raw_input
            .viewports
            .entry(vp_id)
            .or_default()
            .events
            .push(egui::ViewportEvent::Close);
    }
    let mut full_output = ctx.run_ui(raw_input, |ui| (viewport.viewport_ui_cb)(ui));
    c.egui_winit
        .handle_platform_output(&c.window, full_output.platform_output);

    // 3. Render & present onto the child window's wgpu surface using the shared egui_wgpu::Renderer
    let size = c.window.inner_size();
    if let Ok(mut renderer) = egui_renderer.lock() {
        for (id, image_deltas) in &full_output.textures_delta.set {
            for image_delta in image_deltas {
                renderer.update_texture(
                    &descriptors.device,
                    &descriptors.queue,
                    *id,
                    image_delta,
                );
            }
        }

        if size.width > 0 && size.height > 0 {
            if size.width != c.surface_config.width || size.height != c.surface_config.height {
                c.surface_config.width = size.width;
                c.surface_config.height = size.height;
                c.surface
                    .configure(&descriptors.device, &c.surface_config);
            }

            let clipped_primitives =
                ctx.tessellate(full_output.shapes, full_output.pixels_per_point);
            let screen_descriptor = egui_wgpu::ScreenDescriptor {
                size_in_pixels: [size.width, size.height],
                pixels_per_point: c.window.scale_factor() as f32,
            };

            let surface_texture = match c.surface.get_current_texture() {
                wgpu::CurrentSurfaceTexture::Success(tex)
                | wgpu::CurrentSurfaceTexture::Suboptimal(tex) => Some(tex),
                wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                    c.surface
                        .configure(&descriptors.device, &c.surface_config);
                    None
                }
                _ => None,
            };

            if let Some(surface_texture) = surface_texture {
                let view = surface_texture
                    .texture
                    .create_view(&wgpu::TextureViewDescriptor::default());
                let mut encoder = descriptors.device.create_command_encoder(
                    &wgpu::CommandEncoderDescriptor {
                        label: Some("grimlite_child_viewport_encoder"),
                    },
                );
                let cmd_buffers = renderer.update_buffers(
                    &descriptors.device,
                    &descriptors.queue,
                    &mut encoder,
                    &clipped_primitives,
                    &screen_descriptor,
                );
                {
                    let render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("grimlite_child_viewport_pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: 0.1,
                                    g: 0.1,
                                    b: 0.12,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                            depth_slice: None,
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });
                    renderer.render(
                        &mut render_pass.forget_lifetime(),
                        &clipped_primitives,
                        &screen_descriptor,
                    );
                }
                descriptors
                    .queue
                    .submit(cmd_buffers.into_iter().chain(std::iter::once(encoder.finish())));
                c.window.pre_present_notify();
                descriptors.queue.present(surface_texture);
            }
        }

        for id in &full_output.textures_delta.free {
            renderer.free_texture(id);
        }
    }
    full_output.textures_delta.clear();

    // 4. Close immediately if requested (unless cancelled), otherwise put back into the store
    let cancel_close = full_output
        .viewport_output
        .get(&vp_id)
        .is_some_and(|v| v.commands.contains(&egui::ViewportCommand::CancelClose));
    if was_close_requested && !cancel_close {
        let win_id = child.window.id();
        VIEWPORTS.with(|store| {
            store.borrow_mut().window_to_viewport.remove(&win_id);
        });
        drop(child);
    } else {
        VIEWPORTS.with(|store| {
            store.borrow_mut().children.insert(vp_id, child);
        });
    }
}
