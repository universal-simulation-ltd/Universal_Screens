//! ExtenderScreen client: connect to an `extender-host`, decode the streamed
//! H.264, render it live in a window, and capture local input to send back. The
//! cross-platform receiver half (macOS / Windows / Linux).
//!
//! A network thread converts each frame's AVCC bytes (plus the SPS/PPS from
//! `StreamStart`) to Annex-B, decodes them with openh264 (software, portable)
//! into RGBA, and stashes the pixels for the wgpu render loop on the main thread.
//!
//! Run: cargo run -p extender-client [-- HOST_ADDR]   (default 127.0.0.1:9000)

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use extender_core::{Session, StreamEvent};
use extender_protocol::{self as protocol, Button, CaptureMode, ClientHello, Input};
use openh264::decoder::Decoder;
use openh264::formats::YUVSource;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{
    DeviceEvent, DeviceId, ElementState, MouseButton, MouseScrollDelta, WindowEvent,
};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{CursorGrabMode, Fullscreen, Window, WindowId};

const DEFAULT_ADDR: &str = "127.0.0.1:9000";

/// Resolution reported to the host when no monitor can be enumerated.
const FALLBACK_RES: (u32, u32) = (1920, 1080);

/// Resolution presets offered to the user, as a percentage of the monitor's
/// native size. Each keeps the panel's aspect ratio (a uniform scale); pick one
/// with `--res N` (default `[0]`, native). Index into this list.
const SCALE_PRESETS: [u32; 4] = [100, 75, 67, 50];

/// One decoded frame, tightly packed RGBA (stride == width * 4).
struct Frame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

type Shared = Arc<Mutex<Option<Frame>>>;

// ---- networking + decode ------------------------------------------------

/// Connect to the host (via the shared `extender-core` session, which owns the
/// socket + handshake + input upload) and feed decoded frames into `shared`
/// until the stream ends. Runs on its own thread; errors are reported here.
fn run_network(addr: String, shared: Shared, input_rx: Receiver<Input>, hello: ClientHello) {
    println!("connecting to {addr}...");
    let session = match Session::connect(&addr, &hello, input_rx) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("client error: {e}");
            return;
        }
    };
    println!(
        "connected to {addr}; sent hello {}x{} (protocol v{})",
        hello.width, hello.height, hello.protocol_version
    );

    if let Err(e) = decode_loop(&session, &shared) {
        eprintln!("client error: {e}");
        return;
    }
    println!("stream ended");
}

/// Drain `StreamEvent`s from the session, decoding each H.264 frame (software,
/// portable `openh264`) into RGBA for the render loop. The session uploads input
/// on its own thread, so this just consumes video.
fn decode_loop(session: &Session, shared: &Shared) -> Result<(), Box<dyn std::error::Error>> {
    let mut decoder: Option<Decoder> = None;
    let mut sps_pps: Vec<u8> = Vec::new();

    while let Some(event) = session.next_event() {
        match event {
            StreamEvent::Start { width, height, codec, parameter_sets } => {
                println!("stream: {width}x{height} {codec:?}");
                sps_pps = protocol::annex_b_parameter_sets(&parameter_sets);
                decoder = Some(Decoder::new()?);
            }
            StreamEvent::Frame { keyframe, data, .. } => {
                let Some(decoder) = decoder.as_mut() else {
                    return Err("Frame arrived before StreamStart".into());
                };
                // Build an Annex-B access unit; prepend SPS/PPS on keyframes so the
                // decoder always has parameters to lock onto.
                let mut au = if keyframe { sps_pps.clone() } else { Vec::new() };
                protocol::append_annex_b(&mut au, &data);
                match decoder.decode(&au) {
                    Ok(Some(yuv)) => {
                        let (w, h) = yuv.dimensions();
                        let mut rgba = vec![0u8; yuv.rgba8_len()];
                        yuv.write_rgba8(&mut rgba);
                        if let Ok(mut slot) = shared.lock() {
                            *slot = Some(Frame { width: w as u32, height: h as u32, rgba });
                        }
                    }
                    Ok(None) => {} // decoder needs more data before it emits a picture
                    Err(e) => eprintln!("decode error: {e}"),
                }
            }
            // The desktop client renders the live video stream; Snapshot, HostInfo,
            // and WindowList are for the mobile clicker, so ignore them here.
            StreamEvent::Snapshot { .. }
            | StreamEvent::HostInfo { .. }
            | StreamEvent::WindowList { .. } => {}
        }
    }
    Ok(())
}

/// Map a winit mouse button to the protocol button, ignoring extra buttons.
fn map_button(button: MouseButton) -> Option<Button> {
    match button {
        MouseButton::Left => Some(Button::Left),
        MouseButton::Right => Some(Button::Right),
        MouseButton::Middle => Some(Button::Middle),
        _ => None,
    }
}

/// Map a winit physical key to its USB-HID keyboard usage id (the neutral code
/// carried on the wire). Returns `None` for keys not yet mapped.
#[rustfmt::skip]
fn key_to_hid(code: KeyCode) -> Option<u32> {
    let usage: u32 = match code {
        // Letters a–z.
        KeyCode::KeyA => 0x04, KeyCode::KeyB => 0x05, KeyCode::KeyC => 0x06, KeyCode::KeyD => 0x07,
        KeyCode::KeyE => 0x08, KeyCode::KeyF => 0x09, KeyCode::KeyG => 0x0A, KeyCode::KeyH => 0x0B,
        KeyCode::KeyI => 0x0C, KeyCode::KeyJ => 0x0D, KeyCode::KeyK => 0x0E, KeyCode::KeyL => 0x0F,
        KeyCode::KeyM => 0x10, KeyCode::KeyN => 0x11, KeyCode::KeyO => 0x12, KeyCode::KeyP => 0x13,
        KeyCode::KeyQ => 0x14, KeyCode::KeyR => 0x15, KeyCode::KeyS => 0x16, KeyCode::KeyT => 0x17,
        KeyCode::KeyU => 0x18, KeyCode::KeyV => 0x19, KeyCode::KeyW => 0x1A, KeyCode::KeyX => 0x1B,
        KeyCode::KeyY => 0x1C, KeyCode::KeyZ => 0x1D,
        // Digits 1–9, 0.
        KeyCode::Digit1 => 0x1E, KeyCode::Digit2 => 0x1F, KeyCode::Digit3 => 0x20,
        KeyCode::Digit4 => 0x21, KeyCode::Digit5 => 0x22, KeyCode::Digit6 => 0x23,
        KeyCode::Digit7 => 0x24, KeyCode::Digit8 => 0x25, KeyCode::Digit9 => 0x26,
        KeyCode::Digit0 => 0x27,
        // Enter, Escape, Backspace, Tab, Space.
        KeyCode::Enter => 0x28, KeyCode::Escape => 0x29, KeyCode::Backspace => 0x2A,
        KeyCode::Tab => 0x2B, KeyCode::Space => 0x2C,
        // Punctuation: - = [ ] \ ; ' ` , . /  and CapsLock.
        KeyCode::Minus => 0x2D, KeyCode::Equal => 0x2E, KeyCode::BracketLeft => 0x2F,
        KeyCode::BracketRight => 0x30, KeyCode::Backslash => 0x31, KeyCode::Semicolon => 0x33,
        KeyCode::Quote => 0x34, KeyCode::Backquote => 0x35, KeyCode::Comma => 0x36,
        KeyCode::Period => 0x37, KeyCode::Slash => 0x38, KeyCode::CapsLock => 0x39,
        // Arrows: right, left, down, up.
        KeyCode::ArrowRight => 0x4F, KeyCode::ArrowLeft => 0x50, KeyCode::ArrowDown => 0x51,
        KeyCode::ArrowUp => 0x52,
        // Navigation: PageUp/PageDown (slide back/forward), Home, End, Insert, Delete.
        KeyCode::PageUp => 0x4B, KeyCode::PageDown => 0x4E, KeyCode::Home => 0x4A,
        KeyCode::End => 0x4D, KeyCode::Insert => 0x49, KeyCode::Delete => 0x4C,
        // Function keys F1–F12 (F5 starts a slideshow in PowerPoint).
        KeyCode::F1 => 0x3A, KeyCode::F2 => 0x3B, KeyCode::F3 => 0x3C, KeyCode::F4 => 0x3D,
        KeyCode::F5 => 0x3E, KeyCode::F6 => 0x3F, KeyCode::F7 => 0x40, KeyCode::F8 => 0x41,
        KeyCode::F9 => 0x42, KeyCode::F10 => 0x43, KeyCode::F11 => 0x44, KeyCode::F12 => 0x45,
        // Modifiers: L/R control, shift, alt(option), super(command).
        KeyCode::ControlLeft => 0xE0, KeyCode::ShiftLeft => 0xE1, KeyCode::AltLeft => 0xE2,
        KeyCode::SuperLeft => 0xE3, KeyCode::ControlRight => 0xE4, KeyCode::ShiftRight => 0xE5,
        KeyCode::AltRight => 0xE6, KeyCode::SuperRight => 0xE7,
        _ => return None,
    };
    Some(usage)
}

// ---- rendering (mirrors the host's loopback_viewer example) --------------

const SHADER: &str = r#"
struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) vi: u32) -> VsOut {
    var out: VsOut;
    let uv = vec2<f32>(f32((vi << 1u) & 2u), f32(vi & 2u));
    out.uv = uv;
    var p = uv * 2.0 - vec2<f32>(1.0, 1.0);
    out.pos = vec4<f32>(p.x, -p.y, 0.0, 1.0);
    return out;
}

@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, in.uv);
}
"#;

struct FrameTexture {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
    width: u32,
    height: u32,
}

struct Gpu {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    bgl: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    frame_tex: Option<FrameTexture>,
}

impl Gpu {
    async fn new(window: Arc<Window>) -> Self {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::default(),
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .expect("no suitable GPU adapter");
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("extender-client-device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                memory_hints: wgpu::MemoryHints::default(),
                experimental_features: wgpu::ExperimentalFeatures::disabled(),
                trace: wgpu::Trace::Off,
            })
            .await
            .expect("request device");

        let caps = surface.get_capabilities(&adapter);
        let format = caps.formats[0];
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("frame-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("frame-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
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

        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("frame-pl"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("frame-shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("frame-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(format.into())],
            }),
            multiview_mask: None,
            cache: None,
        });

        Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            bgl,
            sampler,
            frame_tex: None,
        }
    }

    fn ensure_texture(&mut self, w: u32, h: u32) {
        if let Some(ft) = &self.frame_tex {
            if ft.width == w && ft.height == h {
                return;
            }
        }
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame-tex"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("frame-bg"),
            layout: &self.bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        self.frame_tex = Some(FrameTexture {
            texture,
            _view: view,
            bind_group,
            width: w,
            height: h,
        });
    }

    fn render(&mut self, shared: &Shared) {
        // Pull the latest frame (if any) without holding the lock during upload.
        let next = shared.lock().ok().and_then(|mut g| g.take());
        if let Some(frame) = next {
            self.ensure_texture(frame.width, frame.height);
            let tex = &self.frame_tex.as_ref().unwrap().texture;
            self.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: tex,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                &frame.rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(frame.width * 4),
                    rows_per_image: Some(frame.height),
                },
                wgpu::Extent3d {
                    width: frame.width,
                    height: frame.height,
                    depth_or_array_layers: 1,
                },
            );
        }

        let surface_tex = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.device, &self.config);
                return;
            }
            _ => return,
        };
        let view = surface_tex
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("frame-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(ft) = &self.frame_tex {
                pass.set_pipeline(&self.pipeline);
                pass.set_bind_group(0, &ft.bind_group, &[]);
                pass.draw(0..3, 0..1);
            }
        }
        self.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        surface_tex.present();
    }

    fn resize(&mut self, w: u32, h: u32) {
        if w == 0 || h == 0 {
            return;
        }
        self.config.width = w;
        self.config.height = h;
        self.surface.configure(&self.device, &self.config);
    }
}

/// Pick the monitor whose native (physical-pixel) size to base the resolution
/// on. `index` selects from `available_monitors()` (in order); otherwise the
/// primary monitor is used. Falls back to the window's current monitor, then
/// [`FALLBACK_RES`], if enumeration comes up empty.
fn native_resolution(
    event_loop: &ActiveEventLoop,
    window: &Window,
    index: Option<usize>,
) -> (u32, u32) {
    let monitors: Vec<_> = event_loop.available_monitors().collect();
    if monitors.is_empty() {
        if let Some(m) = window.current_monitor() {
            let s = m.size();
            return (s.width, s.height);
        }
        eprintln!("no monitors detected; reporting {}x{}", FALLBACK_RES.0, FALLBACK_RES.1);
        return FALLBACK_RES;
    }

    let primary = event_loop.primary_monitor();
    println!("available monitors:");
    for (i, m) in monitors.iter().enumerate() {
        let s = m.size();
        let name = m.name().unwrap_or_else(|| "<unnamed>".to_string());
        let tag = if primary.as_ref() == Some(m) { " (primary)" } else { "" };
        println!("  [{i}] {name} {}x{}{tag}", s.width, s.height);
    }

    let chosen = match index {
        Some(i) if i < monitors.len() => &monitors[i],
        Some(i) => {
            eprintln!("monitor index {i} out of range; using primary");
            primary.as_ref().unwrap_or(&monitors[0])
        }
        None => primary.as_ref().unwrap_or(&monitors[0]),
    };
    let s = chosen.size();
    (s.width, s.height)
}

/// Scale `native` by `percent`, preserving aspect ratio. Both dimensions are
/// rounded down to even numbers (H.264 encoders require even width/height).
fn scaled_even(native: (u32, u32), percent: u32) -> (u32, u32) {
    let w = (native.0 * percent / 100 & !1).max(2);
    let h = (native.1 * percent / 100 & !1).max(2);
    (w, h)
}

/// Resolve the resolution to advertise: pick the monitor's native size, then
/// apply the chosen [`SCALE_PRESETS`] entry (default `[0]`, native). Prints the
/// preset menu so the user can re-run with a different `--res N`.
fn resolve_resolution(
    event_loop: &ActiveEventLoop,
    window: &Window,
    monitor_index: Option<usize>,
    res_index: Option<usize>,
) -> (u32, u32) {
    let native = native_resolution(event_loop, window, monitor_index);

    println!("resolution presets (pick with --res N):");
    for (i, &pct) in SCALE_PRESETS.iter().enumerate() {
        let (w, h) = scaled_even(native, pct);
        println!("  [{i}] {w}x{h} ({pct}%)");
    }

    let idx = match res_index {
        Some(i) if i < SCALE_PRESETS.len() => i,
        Some(i) => {
            eprintln!("res index {i} out of range; using [0] (native)");
            0
        }
        None => 0,
    };
    let pct = SCALE_PRESETS[idx];
    let (w, h) = scaled_even(native, pct);
    println!("reporting [{idx}] {w}x{h} ({pct}%) to host");
    (w, h)
}

struct App {
    shared: Shared,
    gpu: Option<Gpu>,
    input_tx: Sender<Input>,
    /// Taken in `resumed` once the resolution is known, then moved to the network
    /// thread. `None` after the network thread has started.
    input_rx: Option<Receiver<Input>>,
    /// Host address to connect to, plus the optional monitor and resolution-preset
    /// indices chosen on the command line.
    addr: String,
    monitor_index: Option<usize>,
    res_index: Option<usize>,
    /// Which display the host should stream: a virtual second screen (default) or
    /// a mirror of the host's primary display (`--mirror`, remote-control mode).
    capture_mode: CaptureMode,
    /// Whether the pointer is locked and we're forwarding input to the host.
    grabbed: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("ExtenderScreen client")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));

        // Now that winit can enumerate monitors, pick the resolution to advertise
        // and start the network thread (deferred from main until this is known).
        if let Some(input_rx) = self.input_rx.take() {
            let (width, height) =
                resolve_resolution(event_loop, &window, self.monitor_index, self.res_index);
            let hello = ClientHello {
                protocol_version: protocol::PROTOCOL_VERSION,
                width,
                height,
                capture_mode: self.capture_mode,
            };
            let addr = self.addr.clone();
            let shared = self.shared.clone();
            std::thread::spawn(move || run_network(addr, shared, input_rx, hello));
        }

        let gpu = pollster::block_on(Gpu::new(window.clone()));
        self.gpu = Some(gpu);
        window.request_redraw();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gpu) = self.gpu.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Focused(false) if self.grabbed => {
                // Lost keyboard focus (e.g. a forwarded click focused an app on the
                // virtual screen) — release control so the cursor is never stuck and
                // local Esc/keys work again.
                let _ = gpu.window.set_cursor_grab(CursorGrabMode::None);
                gpu.window.set_cursor_visible(true);
                self.grabbed = false;
                println!("control mode OFF (focus lost)");
            }
            WindowEvent::Resized(size) => gpu.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                gpu.render(&self.shared);
                gpu.window.request_redraw();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if self.grabbed {
                    if let Some(button) = map_button(button) {
                        let _ = self.input_tx.send(Input::MouseButton {
                            button,
                            pressed: state == ElementState::Pressed,
                        });
                    }
                } else if state == ElementState::Pressed && button == MouseButton::Left {
                    // First click grabs the pointer to enter control mode.
                    match gpu.window.set_cursor_grab(CursorGrabMode::Locked) {
                        Ok(()) => {
                            gpu.window.set_cursor_visible(false);
                            self.grabbed = true;
                            println!("control mode ON (pointer locked) — press Esc to release");
                        }
                        Err(e) => eprintln!("could not lock the pointer: {e}"),
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } if self.grabbed => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(x, y) => (x, y),
                    MouseScrollDelta::PixelDelta(p) => ((p.x / 10.0) as f32, (p.y / 10.0) as f32),
                };
                let _ = self.input_tx.send(Input::Scroll { dx, dy });
            }
            WindowEvent::KeyboardInput { event, .. } => {
                let pressed = event.state == ElementState::Pressed;
                let key = match event.physical_key {
                    PhysicalKey::Code(code) => Some(code),
                    PhysicalKey::Unidentified(_) => None,
                };
                if self.grabbed {
                    if key == Some(KeyCode::Escape) && pressed {
                        // Release control back to the local machine.
                        let _ = gpu.window.set_cursor_grab(CursorGrabMode::None);
                        gpu.window.set_cursor_visible(true);
                        self.grabbed = false;
                        println!("control mode OFF");
                    } else if let Some(hid) = key.and_then(key_to_hid) {
                        // In control mode every other key goes to the host.
                        let _ = self.input_tx.send(Input::Key { code: hid, pressed });
                    }
                } else if pressed {
                    // Local window controls (F11 is reserved by macOS, so F/Esc).
                    match key {
                        Some(KeyCode::KeyF) => {
                            let fs = gpu.window.fullscreen().is_some();
                            gpu.window
                                .set_fullscreen((!fs).then(|| Fullscreen::Borderless(None)));
                        }
                        Some(KeyCode::Escape) => gpu.window.set_fullscreen(None),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _device_id: DeviceId, event: DeviceEvent) {
        // In control mode, raw mouse motion drives the virtual cursor as deltas
        // (the OS cursor is locked, so WindowEvent::CursorMoved won't fire).
        if self.grabbed {
            if let DeviceEvent::MouseMotion { delta } = event {
                let _ = self
                    .input_tx
                    .send(Input::MouseMoveRelative { dx: delta.0 as f32, dy: delta.1 as f32 });
            }
        }
    }
}

/// Parse the command line: one positional `HOST_ADDR`, plus optional
/// `--monitor N` / `-m N`, `--res N` / `-r N`, and `--mirror` flags.
fn parse_args() -> (String, Option<usize>, Option<usize>, CaptureMode) {
    let mut addr: Option<String> = None;
    let mut monitor_index = None;
    let mut res_index = None;
    let mut capture_mode = CaptureMode::VirtualDisplay;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--monitor" | "-m" => monitor_index = args.next().and_then(|s| s.parse().ok()),
            "--res" | "-r" => res_index = args.next().and_then(|s| s.parse().ok()),
            "--mirror" => capture_mode = CaptureMode::MirrorPrimary,
            _ if arg.starts_with('-') => eprintln!("ignoring unknown flag: {arg}"),
            _ if addr.is_none() => addr = Some(arg),
            _ => eprintln!("ignoring extra argument: {arg}"),
        }
    }
    (
        addr.unwrap_or_else(|| DEFAULT_ADDR.to_string()),
        monitor_index,
        res_index,
        capture_mode,
    )
}

fn main() {
    // The network thread starts in `resumed` once winit can enumerate monitors
    // (we need the chosen monitor's size before we can send the hello).
    let (addr, monitor_index, res_index, capture_mode) = parse_args();
    let shared: Shared = Arc::new(Mutex::new(None));
    let (input_tx, input_rx) = mpsc::channel::<Input>();

    if capture_mode == CaptureMode::MirrorPrimary {
        println!("mirror mode: requesting the host's primary display (remote control)");
    }
    println!("controls: click to grab control · Esc to release · F (when not grabbed) = fullscreen");
    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        shared,
        gpu: None,
        input_tx,
        input_rx: Some(input_rx),
        addr,
        monitor_index,
        res_index,
        capture_mode,
        grabbed: false,
    };
    event_loop.run_app(&mut app).expect("run app");
}
