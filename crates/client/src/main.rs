//! M1d client: connect to an `extender-host`, decode the streamed H.264, and
//! render it live in a window. The receiver half of the network loopback.
//!
//! A network thread reconstructs a decodable `CMSampleBuffer` from each frame's
//! bytes plus the SPS/PPS sent in `StreamStart`, decodes it (VideoToolbox emits
//! NV12), converts NV12 -> BGRA via a `PixelTransferSession`, and stashes the
//! packed pixels for the wgpu render loop on the main thread — the same render
//! path validated by the host's `loopback_viewer` example.
//!
//! Run: cargo run -p extender-client [-- HOST_ADDR]   (default 127.0.0.1:9000)

use std::io::BufReader;
use std::net::TcpStream;
use std::ptr;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex};

use apple_cf::cm::{CMBlockBuffer, CMFormatDescription, CMSampleBuffer};
use apple_cf::cv::{CVPixelBuffer, CVPixelBufferLockFlags};
use apple_cf::raw;
use extender_protocol::{self as protocol, Button, Codec as WireCodec, Input, Message};
use videotoolbox::{DecodedFrame, DecompressionSession, PixelTransferSession};

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const DEFAULT_ADDR: &str = "127.0.0.1:9000";

/// One decoded frame, tightly packed BGRA (stride == width * 4).
struct Frame {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

type Shared = Arc<Mutex<Option<Frame>>>;

// ---- networking + decode ------------------------------------------------

/// Connect to the host and feed decoded frames into `shared` until the stream
/// ends. Runs on its own thread; errors are reported, not propagated.
fn run_network(addr: String, shared: Shared, input_rx: Receiver<Input>) {
    match connect_and_stream(&addr, &shared, input_rx) {
        Ok(()) => println!("stream ended"),
        Err(e) => eprintln!("client error: {e}"),
    }
}

fn connect_and_stream(
    addr: &str,
    shared: &Shared,
    input_rx: Receiver<Input>,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("connecting to {addr}...");
    let stream = TcpStream::connect(addr)?;
    println!("connected to {addr}");
    // A second handle on the same socket carries our input upstream.
    let input_stream = stream.try_clone()?;
    std::thread::spawn(move || input_writer(input_stream, input_rx));
    let mut reader = BufReader::new(stream);

    let mut format: Option<CMFormatDescription> = None;
    let mut decoder: Option<DecompressionSession> = None;

    loop {
        let message: Message = match protocol::read_framed(&mut reader) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                println!("host closed the stream");
                return Ok(());
            }
            Err(e) => return Err(e.into()),
        };
        match message {
            Message::StreamStart { width, height, codec, parameter_sets } => {
                println!("stream: {width}x{height} {codec:?}");
                let fmt = build_format_description(codec, &parameter_sets)
                    .ok_or("failed to rebuild format description from parameter sets")?;
                decoder = Some(make_decoder(&fmt, width, height, shared.clone())?);
                format = Some(fmt);
            }
            Message::Frame { pts_value, pts_timescale, data, .. } => {
                let (Some(format), Some(decoder)) = (format.as_ref(), decoder.as_ref()) else {
                    return Err("Frame arrived before StreamStart".into());
                };
                let sample = reassemble_sample(format, &data, (pts_value, pts_timescale))
                    .ok_or("failed to reassemble CMSampleBuffer")?;
                decoder.decode(&sample)?;
            }
        }
    }
}

/// Drain captured input events and write them upstream until the channel closes
/// (window closed) or the socket errors (host gone).
fn input_writer(mut stream: TcpStream, rx: Receiver<Input>) {
    while let Ok(input) = rx.recv() {
        if protocol::write_framed(&mut stream, &input).is_err() {
            break; // host gone
        }
    }
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

/// Build a decompression session whose callback converts each decoded NV12 frame
/// to BGRA and publishes the packed pixels to `shared`.
fn make_decoder(
    format: &CMFormatDescription,
    width: u32,
    height: u32,
    shared: Shared,
) -> Result<DecompressionSession, Box<dyn std::error::Error>> {
    let transfer = PixelTransferSession::new()?;
    transfer.set_real_time(true)?;
    // Reused destination: VideoToolbox decodes to NV12, we present BGRA.
    let bgra = CVPixelBuffer::create(width as usize, height as usize, u32::from_be_bytes(*b"BGRA"))
        .map_err(|e| format!("failed to allocate BGRA pixel buffer: {e}"))?;

    let session = DecompressionSession::new(format, move |f: DecodedFrame| {
        let Some(nv12) = f.image_buffer else {
            return;
        };
        if transfer.transfer(&nv12, &bgra).is_err() {
            return;
        }
        let Ok(guard) = bgra.lock(CVPixelBufferLockFlags::READ_ONLY) else {
            return;
        };
        let w = guard.width();
        let h = guard.height();
        let stride = guard.bytes_per_row();
        let row = w * 4;
        let src = guard.as_slice();
        if w == 0 || h == 0 || stride < row || src.len() < stride * h {
            return;
        }
        let mut packed = vec![0u8; row * h];
        for y in 0..h {
            packed[y * row..y * row + row].copy_from_slice(&src[y * stride..y * stride + row]);
        }
        drop(guard);
        if let Ok(mut slot) = shared.lock() {
            *slot = Some(Frame { width: w as u32, height: h as u32, bgra: packed });
        }
    })?;
    session.set_real_time(true)?;
    Ok(session)
}

/// Rebuild a `CMVideoFormatDescription` from the stream's parameter sets.
fn build_format_description(
    codec: WireCodec,
    parameter_sets: &[Vec<u8>],
) -> Option<CMFormatDescription> {
    if parameter_sets.is_empty() {
        return None;
    }
    let pointers: Vec<*const u8> = parameter_sets.iter().map(|s| s.as_ptr()).collect();
    let sizes: Vec<usize> = parameter_sets.iter().map(Vec::len).collect();
    let mut out: raw::CMFormatDescriptionRef = ptr::null();
    // VideoToolbox emits AVCC with 4-byte NAL length prefixes; tell the decoder
    // the same so it can find NAL boundaries within each sample.
    let status = unsafe {
        match codec {
            WireCodec::H264 => raw::CMVideoFormatDescriptionCreateFromH264ParameterSets(
                ptr::null(),
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                &mut out,
            ),
            WireCodec::Hevc => raw::CMVideoFormatDescriptionCreateFromHEVCParameterSets(
                ptr::null(),
                pointers.len(),
                pointers.as_ptr(),
                sizes.as_ptr(),
                4,
                ptr::null(),
                &mut out,
            ),
        }
    };
    if status != 0 || out.is_null() {
        return None;
    }
    CMFormatDescription::from_raw(out as *mut _)
}

/// Wrap AVCC frame bytes in a `CMBlockBuffer` and assemble a ready
/// `CMSampleBuffer` the decoder can consume.
fn reassemble_sample(
    format: &CMFormatDescription,
    data: &[u8],
    pts: (i64, i32),
) -> Option<CMSampleBuffer> {
    let block = CMBlockBuffer::create(data)?;
    let timing = raw::CMSampleTimingInfo {
        duration: cm_time(1, pts.1),
        presentationTimeStamp: cm_time(pts.0, pts.1),
        // Invalid DTS (flags = 0) tells CoreMedia decode order == PTS order.
        decodeTimeStamp: raw::CMTime { value: 0, timescale: 0, flags: 0, epoch: 0 },
    };
    let size = data.len();
    let mut out: raw::CMSampleBufferRef = ptr::null_mut();
    let status = unsafe {
        raw::CMSampleBufferCreateReady(
            ptr::null(),
            block.as_ptr() as _,
            format.as_ptr() as _,
            1,
            1,
            &timing,
            1,
            &size,
            &mut out,
        )
    };
    if status != 0 || out.is_null() {
        return None;
    }
    CMSampleBuffer::from_raw(out.cast())
}

/// Construct a valid `CMTime` (the `kCMTimeFlags_Valid` bit set).
const fn cm_time(value: i64, timescale: i32) -> raw::CMTime {
    raw::CMTime { value, timescale, flags: 1, epoch: 0 }
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
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
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
                &frame.bgra,
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

struct App {
    shared: Shared,
    gpu: Option<Gpu>,
    input_tx: Sender<Input>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("ExtenderScreen client (M1d)")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
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
            WindowEvent::Resized(size) => gpu.resize(size.width, size.height),
            WindowEvent::RedrawRequested => {
                gpu.render(&self.shared);
                gpu.window.request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let size = gpu.window.inner_size();
                if size.width > 0 && size.height > 0 {
                    let x = (position.x / f64::from(size.width)) as f32;
                    let y = (position.y / f64::from(size.height)) as f32;
                    let _ = self.input_tx.send(Input::MouseMove {
                        x: x.clamp(0.0, 1.0),
                        y: y.clamp(0.0, 1.0),
                    });
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                if let Some(button) = map_button(button) {
                    let _ = self.input_tx.send(Input::MouseButton {
                        button,
                        pressed: state == ElementState::Pressed,
                    });
                }
            }
            _ => {}
        }
    }
}

fn main() {
    let addr = std::env::args().nth(1).unwrap_or_else(|| DEFAULT_ADDR.to_string());
    let shared: Shared = Arc::new(Mutex::new(None));
    let (input_tx, input_rx) = mpsc::channel::<Input>();

    {
        let shared = shared.clone();
        std::thread::spawn(move || run_network(addr, shared, input_rx));
    }

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App { shared, gpu: None, input_tx };
    event_loop.run_app(&mut app).expect("run app");
}
