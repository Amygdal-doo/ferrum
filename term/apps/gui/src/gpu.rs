//! wgpu surface, a coloured-quad pipeline (backgrounds, cursor, dividers,
//! tab bar) and glyphon text rendering.

use glyphon::{
    Cache, FontSystem, Resolution, SwashCache, TextArea, TextAtlas, TextRenderer, Viewport,
};
use std::sync::Arc;
use wgpu::util::DeviceExt;
use winit::window::Window;

const SHADER: &str = r"
struct VsIn { @location(0) pos: vec2<f32>, @location(1) color: vec4<f32> };
struct VsOut { @builtin(position) pos: vec4<f32>, @location(0) color: vec4<f32> };
@vertex fn vs(i: VsIn) -> VsOut {
    var o: VsOut;
    o.pos = vec4<f32>(i.pos, 0.0, 1.0);
    o.color = i.color;
    return o;
}
@fragment fn fs(i: VsOut) -> @location(0) vec4<f32> { return i.color; }
";

/// A filled rectangle in physical pixels; colour is `0xRRGGBB` + alpha.
#[derive(Debug, Clone, Copy)]
pub struct Quad {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub rgb: u32,
    pub alpha: f32,
}

fn srgb_to_linear(c: u32) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub struct Gpu {
    pub window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    pipeline: wgpu::RenderPipeline,
    pub font_system: FontSystem,
    swash: SwashCache,
    viewport: Viewport,
    atlas: TextAtlas,
    text: TextRenderer,
}

impl Gpu {
    pub async fn new(window: Arc<Window>) -> Result<Self, String> {
        let size = window.inner_size();
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .map_err(|e| e.to_string())?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: Some(&surface),
                ..Default::default()
            })
            .await
            .map_err(|e| format!("no GPU adapter: {e}"))?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default())
            .await
            .map_err(|e| e.to_string())?;
        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            alpha_mode: wgpu::CompositeAlphaMode::Opaque,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &config);

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quads"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[],
            ..Default::default()
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("quads"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: 6 * 4,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x4],
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        let mut font_system = FontSystem::new();
        pick_monospace(&mut font_system);
        let cache = Cache::new(&device);
        let viewport = Viewport::new(&device, &cache);
        let mut atlas = TextAtlas::new(&device, &queue, &cache, format);
        let text = TextRenderer::new(&mut atlas, &device, wgpu::MultisampleState::default(), None);
        Ok(Self {
            window,
            surface,
            device,
            queue,
            config,
            pipeline,
            font_system,
            swash: SwashCache::new(),
            viewport,
            atlas,
            text,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }

    #[must_use]
    pub fn size(&self) -> (f32, f32) {
        (self.config.width as f32, self.config.height as f32)
    }

    pub fn render<'a>(
        &mut self,
        clear: u32,
        quads: &[Quad],
        text: impl IntoIterator<Item = TextArea<'a>>,
    ) -> Result<(), String> {
        self.viewport.update(
            &self.queue,
            Resolution {
                width: self.config.width,
                height: self.config.height,
            },
        );
        self.text
            .prepare(
                &self.device,
                &self.queue,
                &mut self.font_system,
                &mut self.atlas,
                &self.viewport,
                text,
                &mut self.swash,
            )
            .map_err(|e| e.to_string())?;
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Outdated | wgpu::SurfaceError::Lost) => {
                self.surface.configure(&self.device, &self.config);
                return Ok(());
            }
            Err(e) => return Err(e.to_string()),
        };
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());
        let (w, h) = self.size();
        let mut verts: Vec<f32> = Vec::with_capacity(quads.len() * 36);
        for q in quads {
            let (x0, y0) = (q.x / w * 2.0 - 1.0, 1.0 - q.y / h * 2.0);
            let (x1, y1) = ((q.x + q.w) / w * 2.0 - 1.0, 1.0 - (q.y + q.h) / h * 2.0);
            let c = [
                srgb_to_linear((q.rgb >> 16) & 0xff),
                srgb_to_linear((q.rgb >> 8) & 0xff),
                srgb_to_linear(q.rgb & 0xff),
                q.alpha,
            ];
            for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x0, y1), (x1, y0), (x1, y1)] {
                verts.extend_from_slice(&[x, y, c[0], c[1], c[2], c[3]]);
            }
        }
        let buffer = (!verts.is_empty()).then(|| {
            let bytes: Vec<u8> = verts.iter().flat_map(|f| f.to_ne_bytes()).collect();
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("quads"),
                    contents: &bytes,
                    usage: wgpu::BufferUsages::VERTEX,
                })
        });
        let bg = [
            srgb_to_linear((clear >> 16) & 0xff),
            srgb_to_linear((clear >> 8) & 0xff),
            srgb_to_linear(clear & 0xff),
        ];
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: f64::from(bg[0]),
                            g: f64::from(bg[1]),
                            b: f64::from(bg[2]),
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            if let Some(b) = &buffer {
                pass.set_pipeline(&self.pipeline);
                pass.set_vertex_buffer(0, b.slice(..));
                pass.draw(0..(verts.len() / 6) as u32, 0..1);
            }
            self.text
                .render(&self.atlas, &self.viewport, &mut pass)
                .map_err(|e| e.to_string())?;
        }
        self.queue.submit(Some(encoder.finish()));
        self.window.pre_present_notify();
        frame.present();
        self.atlas.trim();
        Ok(())
    }
}

/// Prefers a good installed monospace font for `Family::Monospace`, and drops
/// bitmap-only system faces (e.g. macOS `GB18030Bitmap`): their metrics give
/// infinite glyph widths, which would otherwise be picked for CJK fallback.
pub fn pick_monospace(fs: &mut FontSystem) {
    let broken: Vec<_> = fs
        .db()
        .faces()
        .filter(|f| {
            f.post_script_name.ends_with("Bitmap")
                || f.families.iter().any(|(n, _)| n.ends_with("Bitmap"))
        })
        .map(|f| f.id)
        .collect();
    for id in broken {
        fs.db_mut().remove_face(id);
    }
    let wanted = [
        "JetBrains Mono",
        "SF Mono",
        "Menlo",
        "Monaco",
        "DejaVu Sans Mono",
        "Courier New",
    ];
    let families: Vec<String> = fs
        .db()
        .faces()
        .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
        .collect();
    if let Some(name) = wanted.iter().find(|w| families.iter().any(|f| f == *w)) {
        fs.db_mut().set_monospace_family(*name);
    }
}
