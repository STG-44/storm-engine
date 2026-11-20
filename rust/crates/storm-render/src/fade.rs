//! Full-screen dimming over an encoded scene.
use crate::Gpu;

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

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn fade_overlays_the_existing_frame_and_preserves_opaque_alpha() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let fade = FadeOverlay::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let render = |opacity| {
            crate::render_offscreen_with(&gpu, 16, 8, 0.0, |encoder, view| {
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view,
                        resolve_target: None,
                        depth_slice: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 1.0,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                }));
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
}
