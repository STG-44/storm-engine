use super::{Gpu, OrbitCamera, depth_view, opaque_depth};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use std::{ops::Range, sync::Arc};

#[derive(Clone)]
pub struct RainDraw {
    pub starts: Arc<[[f32; 3]]>,
    pub length: f32,
    pub rgb: [u8; 3],
    pub blocks: [RainBlock; 4],
}

#[derive(Clone, Copy)]
pub struct RainBlock {
    pub origin: [f32; 3],
    pub yaw: f32,
    pub tilt: f32,
    pub wind_yaw: f32,
    pub alpha: u8,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    position_and_opacity: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    world: [[f32; 4]; 4],
    color: [u8; 4],
}

struct Geometry {
    starts: Arc<[[f32; 3]]>,
    length: f32,
    vertices: Range<u32>,
}

pub struct RainRenderer {
    pipeline: wgpu::RenderPipeline,
    group: wgpu::BindGroup,
    view: wgpu::Buffer,
    vertices: Option<wgpu::Buffer>,
    instances: Option<wgpu::Buffer>,
    geometry: Vec<Geometry>,
    blocks: Vec<Instance>,
    depth: Option<([u32; 2], wgpu::TextureView)>,
}

impl RainRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("../rain.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Rain streaks"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![0 => Float32x4],
                    }),
                    Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Instance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &wgpu::vertex_attr_array![
                            1 => Float32x4, 2 => Float32x4, 3 => Float32x4, 4 => Float32x4,
                            5 => Unorm8x4
                        ],
                    }),
                ],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                depth_write_enabled: Some(false),
                ..opaque_depth()
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState {
                        color: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::SrcAlpha,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                        alpha: wgpu::BlendComponent {
                            src_factor: wgpu::BlendFactor::Zero,
                            dst_factor: wgpu::BlendFactor::One,
                            operation: wgpu::BlendOperation::Add,
                        },
                    }),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rain view projection"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Rain view"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            group,
            view,
            vertices: None,
            instances: None,
            geometry: Vec::new(),
            blocks: Vec::new(),
            depth: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        gpu: &Gpu,
        size: [u32; 2],
        seconds: f32,
        camera: &OrbitCamera,
        rain: &[RainDraw],
        standalone: bool,
    ) -> Result<()> {
        let view = camera.uniform(size[0], size[1], seconds)?;
        let eye = Vec3::new(view.eye_time[0], view.eye_time[1], -view.eye_time[2]);
        let reflect = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        let instance_limit = gpu.device.limits().max_buffer_size / size_of::<Instance>() as u64;
        ensure!(
            rain.len() as u64 <= instance_limit.min(u32::MAX.into()) / 4,
            "rain instance buffer limit"
        );
        if self.geometry.len() != rain.len()
            || self.geometry.iter().zip(rain).any(|(geometry, draw)| {
                !Arc::ptr_eq(&geometry.starts, &draw.starts) || geometry.length != draw.length
            })
        {
            self.prepare_geometry(gpu, rain)?;
        }
        self.blocks.clear();
        for draw in rain {
            let [r, g, b] = draw.rgb;
            for block in &draw.blocks {
                // The orbit camera supplies an eye-relative vertical band until
                // the configured scene exposes its authored camera to RAIN.
                let origin = Vec3::from_array(block.origin) + eye;
                let world = reflect
                    * Mat4::from_translation(origin)
                    * Mat4::from_rotation_y(block.wind_yaw)
                    * Mat4::from_rotation_x(block.tilt)
                    * Mat4::from_rotation_y(block.yaw);
                self.blocks.push(Instance {
                    world: world.to_cols_array_2d(),
                    color: [r, g, b, block.alpha],
                });
            }
        }
        if standalone && self.depth.as_ref().is_none_or(|(old, _)| *old != size) {
            self.depth = Some((size, depth_view(&gpu.device, size[0], size[1])));
        }
        gpu.queue
            .write_buffer(&self.view, 0, bytemuck::bytes_of(&view.view_projection));
        upload(
            gpu,
            &mut self.instances,
            "Rain block instances",
            bytemuck::cast_slice(&self.blocks),
        );
        Ok(())
    }

    fn prepare_geometry(&mut self, gpu: &Gpu, rain: &[RainDraw]) -> Result<()> {
        let limit =
            (gpu.device.limits().max_buffer_size / size_of::<Vertex>() as u64).min(u32::MAX.into());
        let mut vertices = Vec::new();
        let mut geometry = Vec::with_capacity(rain.len());
        for draw in rain {
            let first = vertices.len();
            ensure!(
                draw.starts.len() as u64 <= (limit - first as u64) / 2,
                "rain vertex buffer limit"
            );
            for &[x, y, z] in draw.starts.iter() {
                vertices.extend([
                    Vertex {
                        position_and_opacity: [x, y, z, 1.0],
                    },
                    Vertex {
                        position_and_opacity: [x, y + draw.length, z, 0.0],
                    },
                ]);
            }
            geometry.push(Geometry {
                starts: draw.starts.clone(),
                length: draw.length,
                vertices: first as u32..vertices.len() as u32,
            });
        }
        upload(
            gpu,
            &mut self.vertices,
            "Rain streak vertices",
            bytemuck::cast_slice(&vertices),
        );
        self.geometry = geometry;
        Ok(())
    }

    pub(super) fn depth(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|(_, view)| view)
    }

    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let (Some(vertices), Some(instances)) = (&self.vertices, &self.instances) {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.group, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_vertex_buffer(1, instances.slice(..));
            for (index, geometry) in self.geometry.iter().enumerate() {
                let first = index as u32 * 4;
                pass.draw(geometry.vertices.clone(), first..first + 4);
            }
        }
    }
}

fn upload(gpu: &Gpu, buffer: &mut Option<wgpu::Buffer>, label: &str, data: &[u8]) {
    if data.is_empty() {
        return;
    }
    let bytes = data.len() as u64;
    let buffer = if let Some(buffer) = buffer.as_ref().filter(|buffer| buffer.size() >= bytes) {
        buffer
    } else {
        buffer.insert(
            gpu.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: bytes
                    .next_power_of_two()
                    .min(gpu.device.limits().max_buffer_size),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
        )
    };
    gpu.queue.write_buffer(buffer, 0, data);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_instance,
        model::{ModelScene, OceanRenderer},
        render_offscreen_with,
    };
    use storm_sim::sea::{Frames, Parameters, Sea};

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn rain_is_additive_and_occluded_by_configured_water() -> Result<()> {
        let instance = create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut ocean = OceanRenderer::new(&gpu, format);
        let mut rain = RainRenderer::new(&gpu, format);
        let mut camera = OrbitCamera::from_bounds(Vec3::ZERO, 10.0);
        camera.yaw = 0.0;
        camera.pitch = 0.4;
        let eye = camera.eye()?;
        let sea = Sea::new(
            Arc::new(Frames::fixture()),
            Parameters {
                amplitude: [0.0; 2],
                ..Parameters::default()
            },
        )?;
        let mut capture = |y: f32, with_rain: bool, with_sea: bool| {
            let block = RainBlock {
                origin: [-eye.x, y - eye.y, eye.z],
                yaw: 0.0,
                tilt: 0.0,
                wind_yaw: 0.0,
                alpha: 255,
            };
            let draws = [RainDraw {
                starts: Arc::from([[0.0; 3]]),
                length: 1.0,
                rgb: [200, 120, 60],
                blocks: [block; 4],
            }];
            render_offscreen_with(&gpu, 101, 71, 0.0, |encoder, target| {
                ModelScene::draw_configured(
                    &mut [],
                    &gpu,
                    encoder,
                    target,
                    [101, 71],
                    0.0,
                    &camera,
                    with_sea.then_some((&mut ocean, sea.surface())),
                    with_rain.then_some((&mut rain, draws.as_slice())),
                )
            })
        };
        let rain_only = capture(3.0, true, false)?;
        assert!(rain_only.rgba.chunks_exact(4).any(|pixel| pixel[0] > 0));
        let water_only = capture(0.0, false, true)?;
        let under_water = capture(-4.0, true, true)?;
        assert_eq!(water_only.rgba, under_water.rgba);
        let above_water = capture(3.0, true, true)?;
        let mut brighter = false;
        for (dry, wet) in water_only.rgba.iter().zip(above_water.rgba) {
            assert!(wet >= *dry);
            brighter |= wet > *dry;
        }
        assert!(brighter);
        Ok(())
    }
}
