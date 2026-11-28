use super::{CameraUniform, Gpu, OCEAN_SHADER, OrbitCamera, WaveUniform, depth_view, native_sea};
use anyhow::Result;
use storm_sim::sea::{CELLS, Surface};
use wgpu::util::DeviceExt;

pub(super) fn pipeline(
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    layout: &wgpu::BindGroupLayout,
    depth: Option<wgpu::DepthStencilState>,
) -> wgpu::RenderPipeline {
    let device = &gpu.device;
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Ocean surface shader"),
        source: wgpu::ShaderSource::Wgsl(OCEAN_SHADER.into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Ocean camera layout"),
        bind_group_layouts: &[Some(layout)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Ocean surface pipeline"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vertex_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: depth,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fragment_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

/// Uses the same scalar maps and depth pass as the fleet's water renderer.
/// A configured sea can also draw by itself before the menu's models exist.
pub struct OceanRenderer {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    camera: wgpu::Buffer,
    maps: wgpu::Buffer,
    parameters: wgpu::Buffer,
    depth: Option<([u32; 2], wgpu::TextureView)>,
}

impl OceanRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Configured SEA bindings"),
            entries: &[0, 2, 3, 4].map(|binding| wgpu::BindGroupLayoutEntry {
                binding,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: if binding == 3 {
                        wgpu::BufferBindingType::Storage { read_only: true }
                    } else {
                        wgpu::BufferBindingType::Uniform
                    },
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }),
        });
        let buffer = |label, bytes, usage| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let camera = buffer(
            "SEA camera",
            std::mem::size_of::<CameraUniform>() as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let maps = buffer(
            "SEA prepared maps",
            (6 * CELLS * 4) as u64,
            wgpu::BufferUsages::STORAGE,
        );
        let parameters = buffer(
            "SEA sampler parameters",
            std::mem::size_of::<native_sea::Uniform>() as u64,
            wgpu::BufferUsages::UNIFORM,
        );
        let analytic = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Ocean shader wave coefficients"),
            contents: bytemuck::bytes_of(&WaveUniform::from(storm_sim::Waves::default())),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Configured SEA uniforms"),
            layout: &layout,
            entries: [(0, &camera), (2, &analytic), (3, &maps), (4, &parameters)]
                .map(|(binding, buffer)| wgpu::BindGroupEntry {
                    binding,
                    resource: buffer.as_entire_binding(),
                })
                .as_slice(),
        });
        Self {
            pipeline: pipeline(gpu, format, &layout, Some(super::opaque_depth())),
            group,
            camera,
            maps,
            parameters,
            depth: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prepare(
        &mut self,
        gpu: &Gpu,
        size: [u32; 2],
        seconds: f32,
        camera: &OrbitCamera,
        surface: &Surface,
        standalone: bool,
        painted_sky: bool,
    ) -> Result<()> {
        surface.validate()?;
        let camera = camera.uniform(size[0], size[1], seconds)?;
        let mut parameters = native_sea::Uniform::from(surface);
        // WaveXZ uses the native camera's X/Z, with the renderer's Z reflected.
        parameters.camera[..2].copy_from_slice(&[camera.eye_time[0], -camera.eye_time[2]]);
        parameters.bounds[1] = if painted_sky { 1.0 } else { 0.0 };
        gpu.queue
            .write_buffer(&self.camera, 0, bytemuck::bytes_of(&camera));
        gpu.queue
            .write_buffer(&self.maps, 0, bytemuck::cast_slice(surface.maps()));
        gpu.queue
            .write_buffer(&self.parameters, 0, bytemuck::bytes_of(&parameters));
        if standalone && self.depth.as_ref().is_none_or(|(old, _)| *old != size) {
            self.depth = Some((size, depth_view(&gpu.device, size[0], size[1])));
        }
        Ok(())
    }

    pub(super) fn depth(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|(_, view)| view)
    }

    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_bind_group(0, &self.group, &[]);
        pass.set_pipeline(&self.pipeline);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{create_instance, render_offscreen_with};
    use storm_sim::sea::{Frames, Parameters, Sea};

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn configured_water_draws_without_models_and_uses_the_current_wave_maps() -> Result<()> {
        use std::sync::Arc;
        let instance = create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let mut sea = Sea::new(Arc::new(Frames::fixture()), Parameters::default())?;
        let mut ocean = OceanRenderer::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let camera = OrbitCamera::from_bounds([0.0; 3].into(), 40.0);
        let size = [101, 71];
        let mut draw = |surface: &Surface| {
            render_offscreen_with(&gpu, size[0], size[1], 0.0, |encoder, target| {
                super::super::ModelScene::draw_models_with_sea(
                    &mut [],
                    &gpu,
                    encoder,
                    target,
                    size,
                    0.0,
                    &camera,
                    &mut ocean,
                    surface,
                )
            })
        };
        let first = draw(sea.surface())?;
        sea.advance(400)?;
        let later = draw(sea.surface())?;
        assert!(first.rgba.chunks_exact(4).all(|pixel| pixel[3] == 255));
        assert!(
            first
                .rgba
                .chunks_exact(4)
                .any(|pixel| pixel != &first.rgba[..4])
        );
        assert_ne!(first.rgba, later.rgba);
        Ok(())
    }
}
