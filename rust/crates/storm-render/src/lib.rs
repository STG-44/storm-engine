//! Native graphics foundation for the Storm Engine Rust port.

use anyhow::{Context, Result, ensure};
use std::{fs::File, io::BufWriter, path::Path, sync::mpsc, time::Duration};
use wgpu::util::DeviceExt;

pub const DIAGNOSTIC_SHADER: &str = include_str!("sea.wgsl");

/// Reject unavailable backends instead of silently selecting a different API.
pub fn create_instance(backends: wgpu::Backends) -> Result<wgpu::Instance> {
    let enabled = backends & wgpu::Instance::enabled_backend_features();
    ensure!(
        !enabled.is_empty(),
        "requested graphics API is unavailable on this platform"
    );
    let mut descriptor = wgpu::InstanceDescriptor::new_without_display_handle();
    descriptor.backends = enabled;
    Ok(wgpu::Instance::new(descriptor))
}

pub struct Gpu {
    pub adapter: wgpu::Adapter,
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
}

impl Gpu {
    pub async fn request(
        instance: &wgpu::Instance,
        surface: Option<&wgpu::Surface<'_>>,
    ) -> Result<Self> {
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: surface,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .context("no compatible graphics adapter; check the selected API and GPU driver")?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Storm Rust device"),
                ..Default::default()
            })
            .await
            .context("creating the graphics device")?;
        Ok(Self {
            adapter,
            device,
            queue,
        })
    }
}

/// A resource-free scene to exercise the pipeline before original assets are available.
/// This is separate from the future Storm sea simulation.
pub struct DiagnosticScene {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
}

impl DiagnosticScene {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Diagnostic sea WGSL"),
            source: wgpu::ShaderSource::Wgsl(DIAGNOSTIC_SHADER.into()),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Diagnostic sea pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let uniforms = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Frame dimensions and time"),
            contents: bytemuck::cast_slice(&[1.0_f32, 1.0, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Diagnostic frame"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: uniforms.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            uniforms,
            bind_group,
        }
    }

    pub fn draw(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        seconds: f32,
    ) {
        queue.write_buffer(
            &self.uniforms,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, seconds, 0.0]),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Diagnostic sea pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Frame {
    pub fn save_png(&self, path: &Path) -> Result<()> {
        let file = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        let mut encoder = png::Encoder::new(BufWriter::new(file), self.width, self.height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("writing PNG header")?;
        writer
            .write_image_data(&self.rgba)
            .context("writing PNG pixels")?;
        writer.finish().context("finishing PNG")
    }
}

/// Render through the window's scene pipeline, then read back RGBA8 without row padding.
pub fn render_offscreen(gpu: &Gpu, width: u32, height: u32, seconds: f32) -> Result<Frame> {
    let limits = gpu.device.limits();
    ensure!(width > 0 && height > 0, "frame dimensions must be nonzero");
    ensure!(
        width <= limits.max_texture_dimension_2d && height <= limits.max_texture_dimension_2d,
        "frame dimensions exceed this GPU's texture limit"
    );
    ensure!(seconds.is_finite(), "frame time must be finite");
    let row_bytes = width.checked_mul(4).context("frame row is too large")?;
    let padded_row = row_bytes
        .div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        .checked_mul(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
        .context("frame row is too large")?;
    let buffer_size = u64::from(padded_row) * u64::from(height);
    ensure!(
        buffer_size <= limits.max_buffer_size,
        "frame exceeds this GPU's readback buffer limit"
    );
    let extent = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Offscreen diagnostic frame"),
        size: extent,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let readback = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Frame readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let scene = DiagnosticScene::new(&gpu.device, wgpu::TextureFormat::Rgba8UnormSrgb);
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    scene.draw(
        &gpu.queue,
        &mut encoder,
        &texture.create_view(&Default::default()),
        width,
        height,
        seconds,
    );
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_row),
                rows_per_image: Some(height),
            },
        },
        extent,
    );
    gpu.queue.submit([encoder.finish()]);
    let (sender, receiver) = mpsc::sync_channel(1);
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = sender.send(result);
        });
    gpu.device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .context("waiting for GPU frame readback")?;
    receiver
        .recv_timeout(Duration::from_secs(5))
        .context("GPU map callback did not complete")?
        .context("mapping frame readback")?;
    let mapped = readback
        .slice(..)
        .get_mapped_range()
        .context("reading mapped frame")?;
    let mut rgba = Vec::with_capacity((u64::from(row_bytes) * u64::from(height)) as usize);
    for row in mapped.chunks_exact(padded_row as usize) {
        rgba.extend_from_slice(&row[..row_bytes as usize]);
    }
    drop(mapped);
    readback.unmap();
    ensure!(
        rgba.len() == row_bytes as usize * height as usize,
        "GPU readback has an unexpected length"
    );
    Ok(Frame {
        width,
        height,
        rgba,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_shader_validates_without_a_gpu() {
        let module = naga::front::wgsl::parse_str(DIAGNOSTIC_SHADER).expect("valid WGSL");
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .expect("portable shader capabilities");
    }

    #[test]
    #[ignore = "requires a native GPU or a software Vulkan adapter"]
    fn offscreen_frame_has_pixels_animation_and_correct_row_alignment() {
        let instance = create_instance(wgpu::Backends::PRIMARY).unwrap();
        let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
        // 65 RGBA pixels require GPU row padding; CPU output must omit it.
        let first = render_offscreen(&gpu, 65, 33, 0.0).unwrap();
        let later = render_offscreen(&gpu, 65, 33, 8.0).unwrap();
        assert_eq!(first.rgba.len(), 65 * 33 * 4);
        assert!(first.rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert!(first.rgba.chunks_exact(4).any(|p| p != &first.rgba[..4]));
        assert_ne!(
            first.rgba, later.rgba,
            "wave animation must affect the image"
        );
        assert!(render_offscreen(&gpu, 0, 33, 0.0).is_err());
        assert!(render_offscreen(&gpu, 65, 33, f32::NAN).is_err());
        assert!(render_offscreen(&gpu, u32::MAX, 33, 0.0).is_err());
    }
}
