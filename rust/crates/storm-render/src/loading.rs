//! Loading picture composition for window and offscreen targets.
use crate::{Gpu, model::upload_texture};
use anyhow::Result;
use storm_formats::texture::Texture;
use wgpu::util::DeviceExt;

pub struct LoadingPictures {
    pub background: Option<Texture>,
    pub foreground: Option<Texture>,
    pub border: Option<Texture>,
}

pub struct LoadingScreen {
    pipeline: wgpu::RenderPipeline,
    pictures: wgpu::BindGroup,
    rectangle: wgpu::Buffer,
    picture_size: [u32; 2],
}

impl LoadingScreen {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, pictures: &LoadingPictures) -> Result<Self> {
        let device = &gpu.device;
        let empty = device
            .create_texture_with_data(
                &gpu.queue,
                &wgpu::TextureDescriptor {
                    label: Some("Transparent loading image"),
                    size: wgpu::Extent3d {
                        width: 1,
                        height: 1,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8UnormSrgb,
                    usage: wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                },
                wgpu::util::TextureDataOrder::LayerMajor,
                &[0; 4],
            )
            .create_view(&Default::default());
        let texture = |source: &Option<Texture>| {
            source
                .as_ref()
                .map_or_else(|| Ok(empty.clone()), |source| upload_texture(gpu, source))
        };
        let background = texture(&pictures.background)?;
        let foreground = texture(&pictures.foreground)?;
        let border = texture(&pictures.border)?;
        let rectangle = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Loading picture rectangle"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("loading.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Loading pictures"),
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
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        let picture_size = pictures.foreground.as_ref().map_or([1, 1], |picture| {
            [picture.header.width, picture.header.height]
        });
        let pictures = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Loading pictures"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&background),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::TextureView(&foreground),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&border),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: rectangle.as_entire_binding(),
                },
            ],
        });
        Ok(Self {
            pipeline,
            pictures,
            rectangle,
            picture_size,
        })
    }

    /// Composite over the already encoded scene, including its uncovered areas.
    pub fn encode(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
    ) {
        if size.contains(&0) {
            return;
        }
        gpu.queue.write_buffer(
            &self.rectangle,
            0,
            bytemuck::cast_slice(&picture_rect(size, self.picture_size)),
        );
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Loading pictures"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.pictures, &[]);
        pass.draw(0..3, 0..1);
    }
}

fn picture_rect([width, height]: [u32; 2], [image_width, image_height]: [u32; 2]) -> [f32; 4] {
    let scale_x = width as f32 / image_width as f32;
    let scale_y = height as f32 / image_height as f32;
    let scale = scale_x.min(scale_y) * 0.8;
    let left = (1.0 - scale / scale_x) * 0.5;
    let top = (1.0 - scale / scale_y) * 0.5;
    [left, top, 1.0 - left, 1.0 - top]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pixel([r, g, b, a]: [u8; 4]) -> Result<Texture> {
        let mut bytes = [0u32, 1, 1, 1, 21, 4]
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>();
        bytes.extend([b, g, r, a]);
        Ok(Texture::read(&bytes)?)
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn loading_pictures_composite_an_aspect_fitted_foreground_over_the_background() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let pictures = LoadingPictures {
            background: Some(pixel([0, 255, 0, 255])?),
            foreground: Some(pixel([255, 0, 0, 255])?),
            border: Some(pixel([0, 0, 255, 128])?),
        };
        let screen = LoadingScreen::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &pictures)?;
        let sample = |size: [u32; 2], x: u32, y: u32| -> Result<[u8; 4]> {
            let frame =
                crate::render_offscreen_with(&gpu, size[0], size[1], 0.0, |encoder, target| {
                    screen.encode(&gpu, encoder, target, size);
                    Ok(())
                })?;
            let offset = ((y * size[0] + x) * 4) as usize;
            Ok(frame.rgba[offset..offset + 4].try_into()?)
        };
        assert_eq!(sample([320, 180], 20, 90)?, [0, 255, 0, 255]);
        assert_eq!(sample([320, 180], 160, 17)?, [0, 255, 0, 255]);
        let center = sample([320, 180], 160, 18)?;
        assert_eq!(center[1], 0);
        assert_eq!(center[3], 255);
        assert!((185..=190).contains(&center[0]));
        assert!((185..=190).contains(&center[2]));
        assert_eq!(sample([320, 240], 20, 120)?, [0, 255, 0, 255]);
        assert_eq!(sample([320, 240], 64, 120)?, center);
        Ok(())
    }
}
