mod state;

use arc_protocol::{socket_path, ActivityItem, Event, Notification, Request, Response};
use bytemuck::{Pod, Zeroable};
use glyphon::{
    Attrs, Buffer, Cache, Color, Family, FontSystem, Metrics, Resolution, Shaping, SwashCache,
    TextArea, TextAtlas, TextBounds, TextRenderer, Viewport, Weight,
};
use raw_window_handle::{
    RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
};
use smithay_client_toolkit::{
    compositor::{CompositorHandler, CompositorState},
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat,
    output::{OutputHandler, OutputState},
    registry::{ProvidesRegistryState, RegistryState},
    registry_handlers,
    seat::{
        keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers},
        pointer::{PointerEvent, PointerEventKind, PointerHandler, BTN_LEFT},
        Capability, SeatHandler, SeatState,
    },
    shell::{
        wlr_layer::{
            Anchor, KeyboardInteractivity, Layer, LayerShell, LayerShellHandler, LayerSurface,
            LayerSurfaceConfigure,
        },
        WaylandSurface,
    },
};
use state::{Phase, ShellState, SurfaceMode};
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::net::UnixStream,
    process::Command,
    ptr::NonNull,
    sync::mpsc::{self, Receiver},
    thread,
    time::Duration,
};
use wayland_client::{
    globals::registry_queue_init,
    protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface},
    Connection, Proxy, QueueHandle,
};
use wgpu::util::DeviceExt;

const CAPSULE: (u32, u32) = (600, 76);
const SHEET: (u32, u32) = (720, 350);
const PROMPT: (u32, u32) = (680, 104);
const TOP_MARGIN: i32 = 58;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct UiUniform {
    size: [f32; 2],
    mode: f32,
    phase: f32,
}

fn main() -> anyhow::Result<()> {
    let conn = Connection::connect_to_env()?;
    let (globals, mut event_queue) = registry_queue_init(&conn)?;
    let qh = event_queue.handle();
    let compositor = CompositorState::bind(&globals, &qh)?;
    let layer_shell = LayerShell::bind(&globals, &qh)?;
    let surface = compositor.create_surface(&qh);
    let layer =
        layer_shell.create_layer_surface(&qh, surface, Layer::Overlay, Some("arc-shell"), None);
    layer.set_anchor(Anchor::TOP);
    layer.set_margin(TOP_MARGIN, 0, 0, 0);
    layer.set_keyboard_interactivity(KeyboardInteractivity::None);
    layer.set_exclusive_zone(-1);
    layer.set_size(1, 1);
    layer.commit();

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::all(),
        ..Default::default()
    });
    let raw_display_handle = RawDisplayHandle::Wayland(WaylandDisplayHandle::new(
        NonNull::new(conn.backend().display_ptr() as *mut _).unwrap(),
    ));
    let raw_window_handle = RawWindowHandle::Wayland(WaylandWindowHandle::new(
        NonNull::new(layer.wl_surface().id().as_ptr() as *mut _).unwrap(),
    ));
    let gpu_surface = unsafe {
        instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
            raw_display_handle,
            raw_window_handle,
        })?
    };
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
        compatible_surface: Some(&gpu_surface),
        ..Default::default()
    }))
    .ok_or_else(|| anyhow::anyhow!("no wgpu adapter supports the Arc layer surface"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&Default::default(), None))?;
    let format = gpu_surface.get_capabilities(&adapter).formats[0];
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Arc desktop surface"),
        source: wgpu::ShaderSource::Wgsl(SHADER.into()),
    });
    let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("Arc surface uniforms"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });
    let uniform_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("Arc surface state"),
        contents: bytemuck::bytes_of(&UiUniform {
            size: [1.0, 1.0],
            mode: 0.0,
            phase: 0.0,
        }),
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
    });
    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Arc surface bindings"),
        layout: &uniform_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniform_buffer.as_entire_binding(),
        }],
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Arc shell pipeline"),
        bind_group_layouts: &[&uniform_layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Arc shell"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        multiview: None,
        cache: None,
    });
    let mut font_system = FontSystem::new();
    let swash_cache = SwashCache::new();
    let text_cache = Cache::new(&device);
    let viewport = Viewport::new(&device, &text_cache);
    let mut atlas = TextAtlas::new(&device, &queue, &text_cache, format);
    let text_renderer = TextRenderer::new(&mut atlas, &device, Default::default(), None);
    let text_buffer = Buffer::new(&mut font_system, Metrics::new(15.0, 22.0));

    let mut shell = ArcShell {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        keyboard: None,
        pointer: None,
        exit: false,
        configured: false,
        width: 1,
        height: 1,
        requested_surface: SurfaceMode::Hidden,
        layer,
        adapter,
        surface: gpu_surface,
        device,
        queue,
        pipeline,
        uniform_buffer,
        bind_group,
        font_system,
        swash_cache,
        viewport,
        atlas,
        text_renderer,
        text_buffer,
        events: subscribe_to_core(),
        state: ShellState::default(),
    };
    while !shell.exit {
        event_queue.blocking_dispatch(&mut shell)?;
        shell.apply_events();
    }
    Ok(())
}

struct ArcShell {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    exit: bool,
    configured: bool,
    width: u32,
    height: u32,
    requested_surface: SurfaceMode,
    layer: LayerSurface,
    adapter: wgpu::Adapter,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    font_system: FontSystem,
    swash_cache: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text_renderer: TextRenderer,
    text_buffer: Buffer,
    events: Receiver<Event>,
    state: ShellState,
}

impl ArcShell {
    fn configure_gpu(&self) {
        let capabilities = self.surface.get_capabilities(&self.adapter);
        self.surface.configure(
            &self.device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: capabilities.formats[0],
                width: self.width.max(1),
                height: self.height.max(1),
                present_mode: wgpu::PresentMode::Fifo,
                desired_maximum_frame_latency: 2,
                alpha_mode: capabilities
                    .alpha_modes
                    .iter()
                    .copied()
                    .find(|mode| {
                        matches!(
                            mode,
                            wgpu::CompositeAlphaMode::PreMultiplied
                                | wgpu::CompositeAlphaMode::PostMultiplied
                        )
                    })
                    .unwrap_or(wgpu::CompositeAlphaMode::Auto),
                view_formats: vec![],
            },
        );
    }

    fn apply_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            self.state.handle(&event);
        }
        self.state.tick();
        if self.requested_surface != self.state.surface {
            if self.state.surface == SurfaceMode::Expanded {
                self.state.activity = fetch_activity(16);
            }
            self.requested_surface = self.state.surface;
            let (width, height, keyboard) = match self.state.surface {
                SurfaceMode::Hidden => (1, 1, KeyboardInteractivity::None),
                SurfaceMode::Capsule => (CAPSULE.0, CAPSULE.1, KeyboardInteractivity::None),
                SurfaceMode::Expanded => (SHEET.0, SHEET.1, KeyboardInteractivity::OnDemand),
                SurfaceMode::Prompt => (PROMPT.0, PROMPT.1, KeyboardInteractivity::Exclusive),
            };
            self.layer.set_size(width, height);
            self.layer.set_keyboard_interactivity(keyboard);
            self.layer.commit();
        }
    }

    fn label_spans(&self) -> Vec<(String, Attrs<'static>)> {
        let primary = Attrs::new()
            .family(Family::SansSerif)
            .color(Color::rgb(232, 233, 241))
            .metrics(Metrics::new(14.0, 21.0));
        let strong = primary.weight(Weight::SEMIBOLD);
        let muted = primary
            .color(Color::rgb(157, 164, 190))
            .metrics(Metrics::new(13.0, 21.0));
        let eyebrow = primary
            .weight(Weight::SEMIBOLD)
            .color(Color::rgb(195, 160, 224))
            .metrics(Metrics::new(11.0, 19.0));
        let accent = primary
            .weight(Weight::BOLD)
            .color(phase_color(self.state.phase));

        match self.state.surface {
            SurfaceMode::Hidden => Vec::new(),
            SurfaceMode::Capsule => vec![
                (format!("{}   ", phase_mark(self.state.phase)), accent),
                (format!("{}\n", self.state.headline), strong),
                (format!("      {}", one_line(&self.state.detail, 82)), muted),
            ],
            SurfaceMode::Prompt => vec![
                ("ASK ARC\n".into(), eyebrow),
                ("›  ".into(), accent),
                (
                    if self.state.prompt.is_empty() {
                        "Type a request…▎".into()
                    } else {
                        format!("{}▎", self.state.prompt)
                    },
                    if self.state.prompt.is_empty() {
                        muted
                    } else {
                        strong
                    },
                ),
                ("\nEnter to send   ·   Esc to close".into(), muted),
            ],
            SurfaceMode::Expanded => {
                let mut output = vec![
                    ("ARC ACTIVITY\n".into(), eyebrow),
                    (format!("{}   ", phase_mark(self.state.phase)), accent),
                    (format!("{}\n", self.state.headline), strong),
                    (format!("{}\n\n", one_line(&self.state.detail, 88)), muted),
                    ("RECENT\n".into(), eyebrow),
                ];
                if self.state.activity.is_empty() {
                    output.push((
                        "No recent activity. Arc stays out of the way until you need it.\n".into(),
                        muted,
                    ));
                } else {
                    for item in self.state.activity.iter().rev().take(4).rev() {
                        output.push((format!("{}\n", item.title), strong));
                        output.push((format!("{}\n", one_line(&item.body, 92)), muted));
                        if let Some(source) = &item.source_uri {
                            output.push((format!("Source · {}\n", one_line(source, 82)), eyebrow));
                        }
                    }
                }
                output.push(("\n".into(), muted));
                if self.state.confirmation.is_some() {
                    output.push(("ALLOW     DENY".into(), accent));
                    output.push((
                        "                         STOP     OPEN INSPECTOR".into(),
                        muted,
                    ));
                } else {
                    output.push((
                        "STOP     ·     OPEN INSPECTOR     ·     ESC TO CLOSE".into(),
                        muted,
                    ));
                }
                output
            }
        }
    }

    fn draw(&mut self, qh: &QueueHandle<Self>) {
        self.apply_events();
        let spans = self.label_spans();
        self.text_buffer.set_size(
            &mut self.font_system,
            Some(self.width.saturating_sub(64) as f32),
            Some(self.height.saturating_sub(32) as f32),
        );
        self.text_buffer.set_rich_text(
            &mut self.font_system,
            spans.iter().map(|(text, attrs)| (text.as_str(), *attrs)),
            Attrs::new()
                .family(Family::SansSerif)
                .color(Color::rgb(232, 233, 241)),
            Shaping::Advanced,
        );
        self.text_buffer
            .shape_until_scroll(&mut self.font_system, false);
        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.width,
                height: self.height,
            },
        );
        if self.state.surface != SurfaceMode::Hidden {
            let _ = self.text_renderer.prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                [TextArea {
                    buffer: &self.text_buffer,
                    left: 32.0,
                    top: if self.state.surface == SurfaceMode::Capsule {
                        16.0
                    } else {
                        20.0
                    },
                    scale: 1.0,
                    bounds: TextBounds {
                        left: 0,
                        top: 0,
                        right: self.width as i32 - 16,
                        bottom: self.height as i32 - 12,
                    },
                    default_color: Color::rgb(232, 233, 241),
                    custom_glyphs: &[],
                }],
                &mut self.swash_cache,
            );
        }
        let frame = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(_) => {
                self.configure_gpu();
                return;
            }
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.device.create_command_encoder(&Default::default());
        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::bytes_of(&UiUniform {
                size: [self.width as f32, self.height as f32],
                mode: surface_code(self.state.surface),
                phase: phase_code(self.state.phase),
            }),
        );
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Arc native desktop integration"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            if self.state.surface != SurfaceMode::Hidden {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &self.bind_group, &[]);
                pass.draw(0..3, 0..1);
                let _ = self
                    .text_renderer
                    .render(&self.atlas, &self.viewport, &mut pass);
            }
        }
        self.queue.submit(Some(encoder.finish()));
        frame.present();
        self.atlas.trim();
        self.layer
            .wl_surface()
            .frame(qh, self.layer.wl_surface().clone());
        self.layer.commit();
    }

    fn submit_prompt(&mut self) {
        let text = self.state.prompt.trim().to_owned();
        if !text.is_empty() {
            rpc(
                "conversation/submit",
                serde_json::json!({ "utteranceId": uuid::Uuid::new_v4(), "text": text }),
            );
            self.state.phase = Phase::Thinking;
            self.state.headline = "Thinking".into();
            self.state.detail = "Starting your request".into();
            self.state.surface = SurfaceMode::Capsule;
        } else {
            self.state.collapse();
        }
        self.state.prompt.clear();
    }

    fn click(&mut self, x: f64, y: f64) {
        match self.state.surface {
            SurfaceMode::Capsule => self.state.toggle_expanded(),
            SurfaceMode::Expanded if y > self.height as f64 - 76.0 => {
                if self.state.confirmation.is_some() && x < 230.0 {
                    let allow = x < 120.0;
                    rpc(
                        "confirmations/respond",
                        serde_json::json!({ "id": self.state.confirmation, "allow": allow }),
                    );
                    self.state.confirmation = None;
                    self.state.phase = if allow {
                        Phase::Thinking
                    } else {
                        Phase::Stopped
                    };
                    self.state.collapse();
                } else if x > self.width as f64 - 230.0 {
                    let _ = Command::new("arc-inspector").spawn();
                    self.state.collapse();
                } else {
                    rpc("system/stop", serde_json::json!({}));
                }
            }
            _ => {}
        }
    }
}

impl CompositorHandler for ArcShell {
    fn scale_factor_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: i32,
    ) {
    }
    fn transform_changed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: wl_output::Transform,
    ) {
    }
    fn frame(&mut self, _: &Connection, qh: &QueueHandle<Self>, _: &wl_surface::WlSurface, _: u32) {
        self.draw(qh);
    }
    fn surface_enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_surface::WlSurface,
        _: &wl_output::WlOutput,
    ) {
    }
}
impl OutputHandler for ArcShell {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}
impl LayerShellHandler for ArcShell {
    fn closed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &LayerSurface) {
        self.exit = true;
    }
    fn configure(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        _: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _: u32,
    ) {
        self.width = configure.new_size.0.max(1);
        self.height = configure.new_size.1.max(1);
        self.configure_gpu();
        if !self.configured {
            self.configured = true;
            self.draw(qh);
        }
    }
}
impl SeatHandler for ArcShell {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(value) = self.keyboard.take() {
                value.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(value) = self.pointer.take() {
                value.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}
impl KeyboardHandler for ArcShell {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.state.surface == SurfaceMode::Expanded {
            self.state.collapse();
        }
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        match event.keysym {
            Keysym::Escape => self.state.collapse(),
            Keysym::Return | Keysym::KP_Enter if self.state.surface == SurfaceMode::Prompt => {
                self.submit_prompt()
            }
            Keysym::BackSpace if self.state.surface == SurfaceMode::Prompt => {
                self.state.prompt.pop();
            }
            _ if self.state.surface == SurfaceMode::Prompt => {
                if let Some(text) = event.utf8 {
                    if !text.chars().any(char::is_control) {
                        self.state.prompt.push_str(&text);
                    }
                }
            }
            _ => {}
        }
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: KeyEvent,
    ) {
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        _: Modifiers,
        _: u32,
    ) {
    }
}
impl PointerHandler for ArcShell {
    fn pointer_frame(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            if &event.surface == self.layer.wl_surface() {
                if let PointerEventKind::Press {
                    button: BTN_LEFT, ..
                } = event.kind
                {
                    self.click(event.position.0, event.position.1);
                }
            }
        }
    }
}

delegate_compositor!(ArcShell);
delegate_output!(ArcShell);
delegate_seat!(ArcShell);
delegate_keyboard!(ArcShell);
delegate_pointer!(ArcShell);
delegate_layer!(ArcShell);
delegate_registry!(ArcShell);
impl ProvidesRegistryState for ArcShell {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

fn rpc(method: &str, params: serde_json::Value) {
    if let Ok(mut stream) = UnixStream::connect(socket_path("arc-core")) {
        let request = Request {
            jsonrpc: "2.0".into(),
            id: 1,
            method: method.into(),
            params,
        };
        let _ = writeln!(
            stream,
            "{}",
            serde_json::to_string(&request).unwrap_or_default()
        );
    }
}

fn fetch_activity(limit: usize) -> Vec<ActivityItem> {
    let Ok(mut stream) = UnixStream::connect(socket_path("arc-core")) else {
        return Vec::new();
    };
    let request = Request {
        jsonrpc: "2.0".into(),
        id: 2,
        method: "activity/list".into(),
        params: serde_json::json!({ "limit": limit }),
    };
    if writeln!(
        stream,
        "{}",
        serde_json::to_string(&request).unwrap_or_default()
    )
    .is_err()
    {
        return Vec::new();
    }
    let mut line = String::new();
    if BufReader::new(stream).read_line(&mut line).is_err() {
        return Vec::new();
    }
    serde_json::from_str::<Response>(&line)
        .ok()
        .and_then(|response| response.result)
        .and_then(|result| serde_json::from_value(result.get("items")?.clone()).ok())
        .unwrap_or_default()
}

fn subscribe_to_core() -> Receiver<Event> {
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || loop {
        match UnixStream::connect(socket_path("arc-core")) {
            Ok(mut stream) => {
                let request = Request {
                    jsonrpc: "2.0".into(),
                    id: 1,
                    method: "events/subscribe".into(),
                    params: serde_json::json!({ "topics": ["*"] }),
                };
                if writeln!(stream, "{}", serde_json::to_string(&request).unwrap()).is_err() {
                    continue;
                }
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                loop {
                    line.clear();
                    if reader
                        .read_line(&mut line)
                        .ok()
                        .filter(|count| *count > 0)
                        .is_none()
                    {
                        break;
                    }
                    if let Ok(notification) = serde_json::from_str::<Notification>(&line) {
                        if sender.send(notification.params).is_err() {
                            return;
                        }
                    }
                }
            }
            Err(_) => thread::sleep(Duration::from_secs(1)),
        }
    });
    receiver
}

fn phase_mark(phase: Phase) -> &'static str {
    match phase {
        Phase::Listening => "●",
        Phase::Transcribing | Phase::Thinking | Phase::Acting => "◌",
        Phase::Speaking => "◉",
        Phase::NeedsAttention | Phase::Error => "!",
        Phase::Stopped => "■",
        Phase::Idle => "·",
    }
}

fn phase_color(phase: Phase) -> Color {
    match phase {
        Phase::Listening | Phase::Speaking => Color::rgb(166, 209, 137),
        Phase::Transcribing | Phase::Thinking | Phase::Acting => Color::rgb(202, 158, 230),
        Phase::NeedsAttention => Color::rgb(229, 200, 144),
        Phase::Error => Color::rgb(231, 130, 132),
        Phase::Stopped | Phase::Idle => Color::rgb(148, 156, 187),
    }
}

fn phase_code(phase: Phase) -> f32 {
    match phase {
        Phase::Listening | Phase::Speaking => 1.0,
        Phase::Transcribing | Phase::Thinking | Phase::Acting => 2.0,
        Phase::NeedsAttention => 3.0,
        Phase::Error => 4.0,
        Phase::Stopped | Phase::Idle => 0.0,
    }
}

fn surface_code(surface: SurfaceMode) -> f32 {
    match surface {
        SurfaceMode::Hidden => 0.0,
        SurfaceMode::Capsule => 1.0,
        SurfaceMode::Expanded => 2.0,
        SurfaceMode::Prompt => 3.0,
    }
}

fn one_line(value: &str, max: usize) -> String {
    let clean = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if clean.chars().count() > max {
        clean.chars().take(max).collect::<String>() + "…"
    } else {
        clean
    }
}

const SHADER: &str = r#"
struct VertexOut { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
struct UiUniform { size: vec2<f32>, mode: f32, phase: f32 }
@group(0) @binding(0) var<uniform> ui: UiUniform;
@vertex fn vs_main(@builtin(vertex_index) index: u32) -> VertexOut {
  var positions = array<vec2<f32>, 3>(vec2(-1.0,-1.0), vec2(3.0,-1.0), vec2(-1.0,3.0));
  var output: VertexOut; output.position = vec4(positions[index],0.0,1.0); output.uv = positions[index]*0.5+vec2(0.5); return output;
}
fn rounded_box(point: vec2<f32>, half_size: vec2<f32>, radius: f32) -> f32 {
  let q = abs(point) - half_size + vec2(radius);
  return length(max(q, vec2(0.0))) + min(max(q.x, q.y), 0.0) - radius;
}
fn phase_color(value: f32) -> vec3<f32> {
  if (value < 0.5) { return vec3(0.58, 0.61, 0.73); }
  if (value < 1.5) { return vec3(0.65, 0.82, 0.54); }
  if (value < 2.5) { return vec3(0.79, 0.62, 0.90); }
  if (value < 3.5) { return vec3(0.90, 0.78, 0.56); }
  return vec3(0.91, 0.51, 0.52);
}
@fragment fn fs_main(input: VertexOut) -> @location(0) vec4<f32> {
  let point = (input.uv - vec2(0.5)) * ui.size;
  let half_size = ui.size * 0.5 - vec2(5.0);
  let radius = select(15.0, 20.0, ui.mode == 1.0);
  let distance = rounded_box(point, half_size, radius);
  let panel = 1.0 - smoothstep(-0.75, 0.75, distance);
  let inner = 1.0 - smoothstep(-0.75, 0.75, distance + 1.0);
  let border = max(panel - inner, 0.0);
  let shadow = (1.0 - smoothstep(0.0, 7.0, distance)) * (1.0 - panel) * 0.34;
  let top_light = (1.0 - input.uv.y) * 0.003;
  let base = vec3(0.006, 0.007, 0.013) + vec3(top_light);
  let accent = phase_color(ui.phase);
  let accent_edge = border * (0.04 + 0.03 * (1.0 - input.uv.y));
  let color = base * panel + accent * accent_edge + vec3(0.0) * shadow;
  return vec4(color, panel * 0.975 + border * 0.02 + shadow);
}
"#;
