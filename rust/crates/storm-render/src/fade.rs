//! Scene transitions over an encoded scene.
use crate::{
    Gpu,
    loading::{LoadingPictures, LoadingScreen},
};
use anyhow::{Result, ensure};
use std::sync::{Arc, Weak};
use wgpu::util::{TextureBlitter, TextureBlitterBuilder};

/// Identity of one transition's saved start frame.
pub struct FrozenFrame;

pub struct FadeDraw {
    pub pictures: Option<Arc<LoadingPictures>>,
    pub opacity: f32,
    pub frozen_frame: Option<Arc<FrozenFrame>>,
}

struct CapturedFrame {
    key: Weak<FrozenFrame>,
    view: wgpu::TextureView,
}

pub struct FadeCompositor {
    black: FadeOverlay,
    format: wgpu::TextureFormat,
    screens: Vec<Option<(Arc<LoadingPictures>, LoadingScreen)>>,
    captures: Vec<CapturedFrame>,
    scratch: Option<wgpu::TextureView>,
    blitter: Option<TextureBlitter>,
}

impl FadeCompositor {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        Self {
            black: FadeOverlay::new(gpu, format),
            format,
            screens: Vec::new(),
            captures: Vec::new(),
            scratch: None,
            blitter: None,
        }
    }

    pub fn needs_capture(&self, draws: &[FadeDraw]) -> bool {
        draws
            .iter()
            .filter_map(|draw| draw.frozen_frame.as_ref())
            .any(|key| self.captured_view(key).is_none())
    }

    /// A missing output view renders only to update the frozen-frame snapshots.
    pub fn encode_scene(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        output: Option<&wgpu::TextureView>,
        size: [u32; 2],
        draws: &[FadeDraw],
        draw_scene: impl FnOnce(&mut wgpu::CommandEncoder, &wgpu::TextureView) -> Result<()>,
    ) -> Result<()> {
        self.captures
            .retain(|capture| capture.key.strong_count() != 0);
        let needs_scratch =
            output.is_none() || draws.iter().any(|draw| draw.frozen_frame.is_some());
        let scratch = if needs_scratch {
            Some(self.scratch_view(gpu, size)?)
        } else {
            self.scratch = None;
            None
        };
        let target = scratch.as_ref().or(output).expect("fade render target");
        draw_scene(encoder, target)?;

        // Each draw uses a separate uniform buffer, even when several faders
        // share the same images in one command encoder.
        self.screens.resize_with(draws.len(), || None);
        for (index, draw) in draws.iter().enumerate() {
            if let Some(key) = &draw.frozen_frame {
                self.encode_frozen_frame(gpu, encoder, target, key);
            }
            let cache = &mut self.screens[index];
            let Some(pictures) = &draw.pictures else {
                *cache = None;
                self.black.encode(encoder, target, draw.opacity);
                continue;
            };
            if cache
                .as_ref()
                .is_none_or(|(old, _)| !Arc::ptr_eq(old, pictures))
            {
                let screen = LoadingScreen::new(gpu, self.format, pictures)?;
                *cache = Some((pictures.clone(), screen));
            }
            let (_, screen) = cache.as_ref().expect("fade screen prepared");
            screen.encode_fade(gpu, encoder, target, size, draw.opacity);
        }
        if let (Some(scratch), Some(output)) = (&scratch, output) {
            self.blit(gpu, encoder, scratch, output);
        }
        Ok(())
    }

    fn captured_view(&self, key: &Arc<FrozenFrame>) -> Option<&wgpu::TextureView> {
        self.captures
            .iter()
            .find(|capture| capture.key.as_ptr() == Arc::as_ptr(key))
            .map(|capture| &capture.view)
    }

    fn encode_frozen_frame(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        key: &Arc<FrozenFrame>,
    ) {
        if let Some(saved) = self.captured_view(key).cloned() {
            self.blit(gpu, encoder, &saved, target);
        } else {
            let source = target.texture();
            let snapshot = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Frozen fade frame"),
                size: source.size(),
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            encoder.copy_texture_to_texture(
                source.as_image_copy(),
                snapshot.as_image_copy(),
                source.size(),
            );
            self.captures.push(CapturedFrame {
                key: Arc::downgrade(key),
                view: snapshot.create_view(&Default::default()),
            });
        }
    }

    fn scratch_view(&mut self, gpu: &Gpu, [width, height]: [u32; 2]) -> Result<wgpu::TextureView> {
        let limit = gpu.device.limits().max_texture_dimension_2d;
        ensure!(
            width > 0 && height > 0 && width <= limit && height <= limit,
            "invalid fade render dimensions"
        );
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        if self
            .scratch
            .as_ref()
            .is_none_or(|view| view.texture().size() != size)
        {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Fade scene color"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.scratch = Some(texture.create_view(&Default::default()));
        }
        Ok(self.scratch.as_ref().expect("fade scene color").clone())
    }

    fn blit(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
    ) {
        let blitter = self.blitter.get_or_insert_with(|| {
            TextureBlitterBuilder::new(&gpu.device, self.format)
                .sample_type(wgpu::FilterMode::Linear)
                .build()
        });
        blitter.copy(&gpu.device, encoder, source, target);
    }
}

pub struct FadeOverlay(wgpu::RenderPipeline);

impl FadeOverlay {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let shader = gpu
            .device
            .create_shader_module(wgpu::include_wgsl!("fade.wgsl"));
        let blend = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::Constant,
            dst_factor: wgpu::BlendFactor::OneMinusConstant,
            operation: wgpu::BlendOperation::Add,
        };
        Self(
            gpu.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Scene fade"),
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
                            blend: Some(wgpu::BlendState {
                                color: blend,
                                alpha: blend,
                            }),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
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
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        opacity: f32,
    ) {
        if opacity <= 0.0 {
            return;
        }
        let opacity = f64::from(opacity.clamp(0.0, 1.0));
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Scene fade"),
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
        pass.set_pipeline(&self.0);
        pass.set_blend_constant(wgpu::Color {
            r: opacity,
            g: opacity,
            b: opacity,
            a: opacity,
        });
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    fn clear(
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        color: wgpu::Color,
    ) -> Result<()> {
        drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        }));
        Ok(())
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn fade_overlays_the_existing_frame_and_preserves_opaque_alpha() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let fade = FadeOverlay::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let render = |opacity| {
            crate::render_offscreen_with(&gpu, 16, 8, 0.0, |encoder, view| {
                clear(
                    encoder,
                    view,
                    wgpu::Color {
                        r: 0.0,
                        g: 1.0,
                        b: 1.0,
                        a: 1.0,
                    },
                )?;
                fade.encode(encoder, view, opacity);
                Ok(())
            })
        };
        let open = render(0.0)?;
        let midway = render(0.5)?;
        let closed = render(1.0)?;
        assert!(
            open.rgba
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 255, 255])
        );
        assert!(midway.rgba.chunks_exact(4).all(|pixel| pixel[0] == 0
            && (187..=189).contains(&pixel[1])
            && pixel[1] == pixel[2]
            && pixel[3] == 255));
        assert!(
            closed
                .rgba
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 0, 255])
        );
        Ok(())
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn frozen_fade_reuses_the_composited_start_frame_and_releases_it() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let mut fade = FadeCompositor::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let mut draw = FadeDraw {
            pictures: None,
            opacity: 0.0,
            frozen_frame: Some(Arc::new(FrozenFrame)),
        };
        assert!(fade.needs_capture(std::slice::from_ref(&draw)));
        let mut encoder = gpu.device.create_command_encoder(&Default::default());
        fade.encode_scene(
            &gpu,
            &mut encoder,
            None,
            [16, 8],
            std::slice::from_ref(&draw),
            |encoder, view| clear(encoder, view, wgpu::Color::RED),
        )?;
        gpu.queue.submit([encoder.finish()]);
        assert!(!fade.needs_capture(std::slice::from_ref(&draw)));

        draw.opacity = 0.5;
        let mut render = |draws: &[FadeDraw]| {
            crate::render_offscreen_with(&gpu, 24, 12, 0.0, |encoder, view| {
                fade.encode_scene(
                    &gpu,
                    encoder,
                    Some(view),
                    [24, 12],
                    draws,
                    |encoder, view| clear(encoder, view, wgpu::Color::BLUE),
                )
            })
        };
        let frozen = render(std::slice::from_ref(&draw))?;
        assert!(
            frozen
                .rgba
                .chunks_exact(4)
                .all(|pixel| { (187..=189).contains(&pixel[0]) && pixel[1..] == [0, 0, 255] })
        );
        draw.frozen_frame = None;
        let live = render(&[])?;
        assert!(
            live.rgba
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 255, 255])
        );
        assert!(fade.captures.is_empty());
        Ok(())
    }
}
