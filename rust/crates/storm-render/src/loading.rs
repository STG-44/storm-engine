//! Loading picture composition for window and offscreen targets.
use crate::{Gpu, model::upload_texture};
use anyhow::{Result, ensure};
use std::sync::Arc;
use storm_formats::texture::Texture;
use wgpu::util::DeviceExt;

mod text;
pub use text::LoadingText;

#[derive(Clone, Default)]
pub struct LoadingPictures {
    pub background: Option<Arc<Texture>>,
    pub foreground: Option<Arc<Texture>>,
    pub border: Option<Arc<Texture>>,
    pub text: Option<Arc<LoadingText>>,
}

pub struct LoadingScreen {
    pipeline: wgpu::RenderPipeline,
    pictures: wgpu::BindGroup,
    uniforms: wgpu::Buffer,
    picture_size: [u32; 2],
    text_size: Option<[u32; 2]>,
}

enum Frame {
    Pictures,
    Progress(f32),
    Fade(f32),
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
        let texture = |source: Option<&Texture>| {
            source.map_or_else(|| Ok(empty.clone()), |source| upload_texture(gpu, source))
        };
        let background = texture(pictures.background.as_deref())?;
        let foreground = texture(pictures.foreground.as_deref())?;
        let border = texture(pictures.border.as_deref())?;
        let text_size = pictures
            .text
            .as_ref()
            .map(|text| [text.width as u32, text.height as u32]);
        let mask = if let Some(text) = &pictures.text {
            let limit = device.limits().max_texture_dimension_2d;
            ensure!(
                text.width as u32 <= limit && text.height as u32 <= limit,
                "loading message exceeds GPU texture dimensions"
            );
            device
                .create_texture_with_data(
                    &gpu.queue,
                    &wgpu::TextureDescriptor {
                        label: Some("Loading message coverage"),
                        size: wgpu::Extent3d {
                            width: text.width as u32,
                            height: text.height as u32,
                            depth_or_array_layers: 1,
                        },
                        mip_level_count: 1,
                        sample_count: 1,
                        dimension: wgpu::TextureDimension::D2,
                        format: wgpu::TextureFormat::R8Unorm,
                        usage: wgpu::TextureUsages::TEXTURE_BINDING,
                        view_formats: &[],
                    },
                    wgpu::util::TextureDataOrder::LayerMajor,
                    &text.coverage,
                )
                .create_view(&Default::default())
        } else {
            empty
        };
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Loading screen uniforms"),
            size: 48,
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
                    resource: uniforms.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: wgpu::BindingResource::TextureView(&mask),
                },
            ],
        });
        Ok(Self {
            pipeline,
            pictures,
            uniforms,
            picture_size,
            text_size,
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
        self.encode_frame(gpu, encoder, target, size, Frame::Pictures);
    }

    /// A progress frame starts on a clean background and can be presented
    /// while the script VM is still loading the next scene.
    pub fn encode_progress(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        phase: f32,
    ) {
        self.encode_frame(gpu, encoder, target, size, Frame::Progress(phase));
    }

    pub(crate) fn encode_fade(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        opacity: f32,
    ) {
        self.encode_frame(gpu, encoder, target, size, Frame::Fade(opacity));
    }

    fn encode_frame(
        &self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        frame: Frame,
    ) {
        if size.contains(&0) {
            return;
        }
        let rectangle = picture_rect(size, self.picture_size);
        let aspect = size[0] as f32 / size[1] as f32;
        let text = self
            .text_size
            .map_or([0.0, 0.0, 1.0, 1.0], |text| caption_rect(size, text));
        let has_text = if self.text_size.is_some() { 1.0 } else { 0.0 };
        let (progress, fade) = match frame {
            Frame::Pictures => (-1.0, -1.0),
            Frame::Progress(phase) => (phase, -1.0),
            Frame::Fade(opacity) => (-1.0, opacity.clamp(0.0, 1.0)),
        };
        let uniforms = [rectangle, [progress, aspect, has_text, fade], text];
        gpu.queue
            .write_buffer(&self.uniforms, 0, bytemuck::cast_slice(&uniforms));
        let load = if matches!(frame, Frame::Progress(_)) {
            wgpu::LoadOp::Clear(wgpu::Color {
                r: 0.008,
                g: 0.015,
                b: 0.032,
                a: 1.0,
            })
        } else {
            wgpu::LoadOp::Load
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Loading pictures"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load,
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

fn caption_rect([width, height]: [u32; 2], [text_width, text_height]: [u32; 2]) -> [f32; 4] {
    let [width, height, text_width, text_height] =
        [width, height, text_width, text_height].map(|value| value as f32);
    let scale = (height / 600.0)
        .max(0.65)
        .min(0.9 * width / text_width)
        .min(0.25 * height / text_height);
    let half_width = 0.5 * text_width * scale / width;
    let half_height = 0.5 * text_height * scale / height;
    [
        0.5 - half_width,
        0.78 - half_height,
        0.5 + half_width,
        0.78 + half_height,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fade::{FadeCompositor, FadeDraw};

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
            background: Some(Arc::new(pixel([0, 255, 0, 255])?)),
            foreground: Some(Arc::new(pixel([255, 0, 0, 255])?)),
            border: Some(Arc::new(pixel([0, 0, 255, 128])?)),
            text: None,
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

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn textured_fades_blend_the_backdrop_and_foreground_in_draw_order() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let pictures = Arc::new(LoadingPictures {
            background: Some(Arc::new(pixel([0, 255, 0, 255])?)),
            foreground: Some(Arc::new(pixel([255, 0, 0, 255])?)),
            ..LoadingPictures::default()
        });
        let mut fades = FadeCompositor::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let mut render = |draws: &[FadeDraw]| {
            crate::render_offscreen_with(&gpu, 320, 180, 0.0, |encoder, view| {
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                }));
                fades.encode(&gpu, encoder, view, [320, 180], draws)
            })
        };
        let draws = [0.5, 0.25].map(|opacity| FadeDraw {
            pictures: Some(pictures.clone()),
            opacity,
        });
        let single = render(&draws[..1])?;
        let stacked = render(&draws)?;
        let sample = |frame: &crate::Frame, x, expected: [u8; 4]| {
            let offset = ((90 * 320 + x) * 4) as usize;
            let actual = &frame.rgba[offset..offset + 4];
            assert!(
                actual
                    .iter()
                    .zip(expected)
                    .all(|(&a, e)| a.abs_diff(e) <= 2),
                "pixel at {x}: expected {expected:?}, got {actual:?}",
            );
        };
        sample(&single, 20, [0, 188, 188, 255]);
        sample(&single, 160, [188, 137, 137, 255]);
        sample(&stacked, 20, [0, 207, 165, 255]);
        sample(&stacked, 160, [193, 155, 105, 255]);
        Ok(())
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn loading_indicator_moves_and_renders_without_picture_assets() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let screen = LoadingScreen::new(
            &gpu,
            wgpu::TextureFormat::Rgba8UnormSrgb,
            &LoadingPictures::default(),
        )?;
        let render = |phase| {
            crate::render_offscreen_with(&gpu, 320, 240, 0.0, |encoder, view| {
                screen.encode_progress(&gpu, encoder, view, [320, 240], phase);
                Ok(())
            })
        };
        let first = render(0.0)?;
        let later = render(0.5)?;
        let offset = 210 * 320 * 4;
        assert_eq!(first.rgba[..offset], later.rgba[..offset]);
        assert_ne!(first.rgba[offset..], later.rgba[offset..]);
        Ok(())
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn loading_caption_draws_glyph_coverage_without_picture_assets() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let mut coverage = vec![0; 16 * 16];
        for row in coverage.chunks_exact_mut(16).take(12).skip(4) {
            row[4..12].fill(255);
        }
        let pictures = LoadingPictures {
            text: Some(Arc::new(LoadingText {
                width: 16,
                height: 16,
                coverage,
            })),
            ..LoadingPictures::default()
        };
        let screen = LoadingScreen::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb, &pictures)?;
        let frame = crate::render_offscreen_with(&gpu, 320, 240, 0.0, |encoder, view| {
            screen.encode(&gpu, encoder, view, [320, 240]);
            Ok(())
        })?;
        let pixel = |x: usize, y: usize| &frame.rgba[(y * 320 + x) * 4..][..4];
        assert!(pixel(160, 187)[0] > 220);
        assert!(pixel(155, 187)[0] < 100);
        assert_eq!(pixel(140, 187), [0, 0, 0, 0]);
        Ok(())
    }
}
