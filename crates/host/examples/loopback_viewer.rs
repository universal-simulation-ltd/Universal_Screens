//! M1b loopback viewer: capture the main display and render it live into a
//! wgpu window on the same Mac (no network). Validates the capture -> CPU copy
//! -> GPU texture -> present path that the real client will reuse.
//!
//! Run: cargo run -p extender-host --example loopback_viewer
//! Requires Screen Recording permission (System Settings > Privacy & Security).
//! Note: capturing the same display you're viewing produces the classic
//! "infinite mirror" effect in the window — that's expected and confirms it works.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use screencapturekit::cv::CVPixelBufferLockFlags;
use screencapturekit::prelude::*;

use winit::application::ApplicationHandler;
use winit::dpi::LogicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// One captured frame, tightly packed BGRA (stride == width * 4).
struct Frame {
    width: u32,
    height: u32,
    bgra: Vec<u8>,
}

type Shared = Arc<Mutex<Option<Frame>>>;

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

fn start_capture(shared: Shared) -> Option<SCStream> {
    let content = match SCShareableContent::get() {
        Ok(c) => c,
        Err(e) => {
            eprintln!(
                "capture unavailable: {e}\n\
                 Grant Screen Recording permission (System Settings > Privacy & Security \
                 > Screen & System Audio Recording), then quit/reopen this terminal and rerun."
            );
            return None;
        }
    };
    let display = content.displays().into_iter().next()?;
    println!(
        "capturing display {} ({}x{} pts)",
        display.display_id(),
        display.width(),
        display.height()
    );

    let filter = SCContentFilter::create()
        .with_display(&display)
        .with_excluding_windows(&[])
        .build();
    let config = SCStreamConfiguration::new()
        .with_width(display.width())
        .with_height(display.height())
        .with_fps(60);

    let logged = Arc::new(AtomicUsize::new(0));
    let mut stream = SCStream::new(&filter, &config);
    stream.add_output_handler(
        move |sample: CMSampleBuffer, _ty: SCStreamOutputType| {
            let Some(buffer) = sample.image_buffer() else {
                return;
            };
            let Ok(guard) = buffer.lock(CVPixelBufferLockFlags::READ_ONLY) else {
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
            if logged.fetch_add(1, Ordering::Relaxed) == 0 {
                println!("first frame: {w}x{h} px (stride {stride})");
            }
            if let Ok(mut g) = shared.lock() {
                *g = Some(Frame {
                    width: w as u32,
                    height: h as u32,
                    bgra: packed,
                });
            }
        },
        SCStreamOutputType::Screen,
    );

    if let Err(e) = stream.start_capture() {
        eprintln!("start_capture failed: {e}");
        return None;
    }
    Some(stream)
}

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
                label: Some("extender-loopback-device"),
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
    _stream: Option<SCStream>,
    gpu: Option<Gpu>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gpu.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("ExtenderScreen loopback (M1b)")
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
            _ => {}
        }
    }
}

fn main() {
    let shared: Shared = Arc::new(Mutex::new(None));
    let stream = start_capture(shared.clone());
    if stream.is_none() {
        eprintln!("(opening window anyway; it will stay black until capture works)");
    }

    let event_loop = EventLoop::new().expect("create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        shared,
        _stream: stream,
        gpu: None,
    };
    event_loop.run_app(&mut app).expect("run app");
}
