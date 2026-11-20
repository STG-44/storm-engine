//! The DX9 display ramp, applied only to the game's output.
use crate::Gpu;
use wgpu::util::DeviceExt;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorCorrection {
    pub contrast: f32,
    pub gamma: f32,
    pub brightness: f32,
}

impl ColorCorrection {
    pub fn ramp(self) -> [u16; 256] {
        std::array::from_fn(|index| {
            (self.contrast * 255.0 * 256.0 * (index as f32 / 255.0).powf(1.0 / self.gamma)
                + self.brightness * 256.0)
                .clamp(0.0, 65535.0) as u16
        })
    }
}

pub struct ColorPass {
    format: wgpu::TextureFormat,
    pipeline: Option<wgpu::RenderPipeline>,
    source: Option<wgpu::TextureView>,
    ramp: Option<(ColorCorrection, wgpu::Buffer)>,
}

impl ColorPass {
    pub fn new(format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            pipeline: None,
            source: None,
            ramp: None,
        }
    }

    pub fn source(&mut self, gpu: &Gpu, [width, height]: [u32; 2]) -> wgpu::TextureView {
        let size = wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        };
        if self
            .source
            .as_ref()
            .is_none_or(|view| view.texture().size() != size)
        {
            let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Scene before color correction"),
                size,
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });
            self.source = Some(texture.create_view(&Default::default()));
        }
        self.source
            .as_ref()
            .expect("uncorrected scene color")
            .clone()
    }

    pub fn encode(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        correction: ColorCorrection,
    ) {
        let pipeline = self.pipeline.get_or_insert_with(|| {
            let shader = gpu
                .device
                .create_shader_module(wgpu::include_wgsl!("color.wgsl"));
            gpu.device
                .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("Display color correction"),
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
                        targets: &[Some(self.format.into())],
                        compilation_options: Default::default(),
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                })
        });
        if self
            .ramp
            .as_ref()
            .is_none_or(|(previous, _)| *previous != correction)
        {
            let levels = correction.ramp().map(|value| f32::from(value) / 65535.0);
            let buffer = gpu
                .device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("Display color ramp"),
                    contents: bytemuck::cast_slice(&levels),
                    usage: wgpu::BufferUsages::UNIFORM,
                });
            self.ramp = Some((correction, buffer));
        }
        let bindings = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Display color inputs"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self
                        .ramp
                        .as_ref()
                        .expect("color ramp")
                        .1
                        .as_entire_binding(),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Display color correction"),
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
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    const ADJUSTED: ColorCorrection = ColorCorrection {
        contrast: 0.5,
        gamma: 2.0,
        brightness: 32.0,
    };

    #[test]
    fn legacy_color_ramp_uses_gamma_and_clamps_to_sixteen_bits() {
        let ramp = ADJUSTED.ramp();
        assert_eq!(ramp[0], 8192);
        assert_eq!(ramp[255], 40832);
        assert!(ramp[128] > 8192 + (40832 - 8192) / 2);
        let clamped = ColorCorrection {
            brightness: -1.0,
            gamma: 1.0,
            contrast: 2.0,
        }
        .ramp();
        assert_eq!(clamped[0], 0);
        assert_eq!(clamped[255], 65535);
        let module = naga::front::wgsl::parse_str(include_str!("color.wgsl")).unwrap();
        naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::empty(),
        )
        .validate(&module)
        .unwrap();
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn color_correction_applies_the_display_ramp_to_encoded_channels() -> Result<()> {
        let instance = crate::create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let mut correction = ColorPass::new(wgpu::TextureFormat::Rgba8UnormSrgb);
        let size = [16, 8];
        let frame =
            crate::render_offscreen_with(&gpu, size[0], size[1], 0.0, |encoder, target| {
                let source = correction.source(&gpu, size);
                drop(encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &source,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.0,
                                g: 0.2158605,
                                b: 1.0,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    ..Default::default()
                }));
                correction.encode(&gpu, encoder, &source, target, ADJUSTED);
                Ok(())
            })?;
        let ramp = ADJUSTED.ramp();
        let expected = [0, 128, 255].map(|i| (f32::from(ramp[i]) / 257.0).round() as u8);
        for pixel in frame.rgba.chunks_exact(4) {
            for (&actual, expected) in pixel.iter().zip(expected) {
                assert!(
                    actual.abs_diff(expected) <= 1,
                    "{pixel:?}, expected {expected}"
                );
            }
            assert_eq!(pixel[3], 255);
        }
        Ok(())
    }
}
