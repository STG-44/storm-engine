//! The original 128-square screenshot samples from the uncorrected back buffer.
use crate::Gpu;
pub use storm_formats::save_thumbnail::SIDE;

pub struct ScreenshotPass(wgpu::RenderPipeline);

impl ScreenshotPass {
    pub fn new(gpu: &Gpu) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::include_wgsl!("screenshot.wgsl"));
        Self(
            gpu.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Save thumbnail"),
                    layout: None,
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vertex"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fragment"),
                        targets: &[Some(wgpu::TextureFormat::Rgba8UnormSrgb.into())],
                        compilation_options: Default::default(),
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                }),
        )
    }

    pub fn encode(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let source = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Save thumbnail source"),
            layout: &self.0.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(source),
            }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Save thumbnail"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.0);
        pass.set_bind_group(0, &source, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn screenshot_samples_legacy_integer_coordinates_and_forces_opaque_alpha() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let size = wgpu::Extent3d {
            width: 67,
            height: 193,
            depth_or_array_layers: 1,
        };
        let source = gpu.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Screenshot coordinate fixture"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let pixels: Vec<u8> = (0..size.height)
            .flat_map(|y| (0..size.width).flat_map(move |x| [x as u8, y as u8, 71, 17]))
            .collect();
        gpu.queue.write_texture(
            source.as_image_copy(),
            &pixels,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(size.width * 4),
                rows_per_image: Some(size.height),
            },
            size,
        );
        let pass = ScreenshotPass::new(&gpu);
        let source = source.create_view(&Default::default());
        let result = crate::render_offscreen_with(&gpu, SIDE, SIDE, 0.0, |encoder, target| {
            pass.encode(&gpu, encoder, &source, target);
            Ok(())
        })?;
        for y in 0..SIDE {
            for x in 0..SIDE {
                let index = ((y * SIDE + x) * 4) as usize;
                let expected = [
                    (x * size.width / SIDE) as u8,
                    (y * size.height / SIDE) as u8,
                    71,
                    255,
                ];
                assert_eq!(result.rgba[index..index + 4], expected);
            }
        }
        Ok(())
    }
}
