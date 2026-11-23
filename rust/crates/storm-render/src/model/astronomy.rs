use super::{Gpu, OrbitCamera, depth_view, opaque_depth, upload_texture};
use anyhow::{Result, ensure};
use glam::Vec3;
use std::rc::Rc;
use storm_formats::{astronomy::MAX_STARS, texture::Texture};
use storm_sim::sail_lod::VERTICAL_FOV;
use wgpu::util::DeviceExt;

#[derive(Clone)]
pub struct CelestialDraw {
    pub texture: Rc<Texture>,
    pub sprites: Rc<[CelestialSprite]>,
    /// Telescope and visual magnitude limits for the point-sprite catalog.
    pub magnitudes: Option<[f32; 2]>,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CelestialSprite {
    offset_size: [f32; 4],
    rgb: [u8; 4],
    brightness: [f32; 2],
}

impl CelestialSprite {
    pub fn new(offset: [f32; 3], size: f32, rgb: [u8; 3], opacity: f32, magnitude: f32) -> Self {
        let [x, y, z] = offset;
        let [r, g, b] = rgb;
        Self {
            offset_size: [x, y, z, size],
            rgb: [r, g, b, 255],
            brightness: [opacity, magnitude],
        }
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct View {
    project: [[f32; 4]; 4],
    eye_ratio: [f32; 4],
    right: [f32; 4],
    up: [f32; 4],
}

struct Batch {
    texture: Rc<Texture>,
    sprites: Rc<[CelestialSprite]>,
    vertices: wgpu::Buffer,
    parameters: wgpu::Buffer,
    bind: wgpu::BindGroup,
}

pub struct AstronomyRenderer {
    pipeline: wgpu::RenderPipeline,
    view: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    batches: Vec<Batch>,
    depth: Option<([u32; 2], wgpu::TextureView)>,
}

impl AstronomyRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("../astronomy.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Astronomy sprites"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<CelestialSprite>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Unorm8x4, 2 => Float32x2
                    ],
                })],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
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
                    blend: Some(wgpu::BlendState {
                        alpha: wgpu::BlendComponent::REPLACE,
                        ..wgpu::BlendState::ALPHA_BLENDING
                    }),
                    format,
                    write_mask: wgpu::ColorWrites::COLOR,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let view = uniform_buffer(device, "Astronomy view", size_of::<View>() as u64);
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Astronomy view"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        Self {
            pipeline,
            view,
            view_group,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            batches: Vec::new(),
            depth: None,
        }
    }

    fn bind(
        &self,
        gpu: &Gpu,
        source: &Texture,
        parameters: &wgpu::Buffer,
    ) -> Result<wgpu::BindGroup> {
        let texture = upload_texture(gpu, source)?;
        Ok(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Astronomy sprite parameters"),
            layout: &self.pipeline.get_bind_group_layout(1),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: parameters.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&texture),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        }))
    }

    pub(super) fn prepare(
        &mut self,
        gpu: &Gpu,
        size: [u32; 2],
        seconds: f32,
        camera: &OrbitCamera,
        draws: &[CelestialDraw],
        standalone: bool,
    ) -> Result<()> {
        ensure!(
            draws.len() <= 128,
            "astronomy draw count exceeds scene budget"
        );
        let uniform = camera.uniform(size[0], size[1], seconds)?;
        let [x, y, z, _] = uniform.eye_time;
        let eye = Vec3::new(x, y, z);
        let forward = (camera.target - eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let aspect = size[0] as f32 / size[1] as f32;
        let view = View {
            project: uniform.view_projection,
            eye_ratio: eye.extend(1.0 / aspect).to_array(),
            right: right.extend(0.0).to_array(),
            up: right.cross(forward).extend(0.0).to_array(),
        };
        gpu.queue
            .write_buffer(&self.view, 0, bytemuck::bytes_of(&view));
        let fov = 2.0 * ((VERTICAL_FOV / 2.0).tan() * aspect).atan();
        let fov_factor = ((fov - 0.14) / (1.285 - 0.14)).clamp(0.0, 1.0);
        self.batches.truncate(draws.len());
        for (index, draw) in draws.iter().enumerate() {
            ensure!(
                (1..=MAX_STARS).contains(&draw.sprites.len()),
                "astronomy sprite count exceeds scene budget"
            );
            let reusable = self
                .batches
                .get(index)
                .is_some_and(|batch| Rc::ptr_eq(&batch.texture, &draw.texture));
            if !reusable {
                let parameters =
                    uniform_buffer(&gpu.device, "Astronomy magnitude and point mode", 16);
                let batch = Batch {
                    texture: draw.texture.clone(),
                    sprites: draw.sprites.clone(),
                    vertices: sprite_buffer(gpu, draw),
                    bind: self.bind(gpu, &draw.texture, &parameters)?,
                    parameters,
                };
                if index == self.batches.len() {
                    self.batches.push(batch);
                } else {
                    self.batches[index] = batch;
                }
            }
            let batch = &mut self.batches[index];
            if !Rc::ptr_eq(&batch.sprites, &draw.sprites) {
                let data = bytemuck::cast_slice(&draw.sprites);
                if batch.vertices.size() >= data.len() as u64 {
                    gpu.queue.write_buffer(&batch.vertices, 0, data);
                } else {
                    batch.vertices = sprite_buffer(gpu, draw);
                }
                batch.sprites = draw.sprites.clone();
            }
            let parameters = if let Some([telescope, visual]) = draw.magnitudes {
                [telescope + (visual - telescope) * fov_factor, 1.0, 0.0, 0.0]
            } else {
                [0.0; 4]
            };
            gpu.queue
                .write_buffer(&batch.parameters, 0, bytemuck::bytes_of(&parameters));
        }
        if standalone && self.depth.as_ref().is_none_or(|(old, _)| *old != size) {
            self.depth = Some((size, depth_view(&gpu.device, size[0], size[1])));
        }
        Ok(())
    }

    pub(super) fn depth(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|(_, depth)| depth)
    }

    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.view_group, &[]);
        for batch in &self.batches {
            pass.set_bind_group(1, &batch.bind, &[]);
            pass.set_vertex_buffer(0, batch.vertices.slice(..));
            pass.draw(0..6, 0..batch.sprites.len() as u32);
        }
    }
}

fn uniform_buffer(device: &wgpu::Device, label: &str, size: u64) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: Some(label),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

fn sprite_buffer(gpu: &Gpu, draw: &CelestialDraw) -> wgpu::Buffer {
    gpu.device
        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Astronomy sprites"),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            contents: bytemuck::cast_slice(&draw.sprites),
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_instance,
        model::{Atmosphere, ModelScene},
        render_offscreen_with,
    };

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn astronomy_projects_textured_points_and_planets_with_alpha_blending() -> Result<()> {
        let instance = create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let mut renderer = AstronomyRenderer::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let mut camera = OrbitCamera::from_bounds(Vec3::ZERO, 1.0);
        camera.yaw = 0.0;
        camera.pitch = 0.0;
        let mut tx = [0u32, 1, 1, 1, 21, 4].map(u32::to_le_bytes).concat();
        tx.extend([255; 4]);
        let texture = Rc::new(Texture::read(&tx)?);
        let size = [128, 96];
        let mut render = |opacity| {
            let draws = [
                CelestialDraw {
                    texture: texture.clone(),
                    sprites: Rc::from([CelestialSprite::new(
                        [-30.0, 0.0, 200.0],
                        14.0,
                        [255, 0, 0],
                        opacity,
                        -2.0,
                    )]),
                    magnitudes: Some([10.0, 8.0]),
                },
                CelestialDraw {
                    texture: texture.clone(),
                    sprites: Rc::from([CelestialSprite::new(
                        [30.0, 0.0, 200.0],
                        10.0,
                        [0, 255, 0],
                        opacity,
                        0.0,
                    )]),
                    magnitudes: None,
                },
            ];
            render_offscreen_with(&gpu, size[0], size[1], 0.0, |encoder, target| {
                ModelScene::draw_configured_with_atmosphere(
                    &mut [],
                    &gpu,
                    encoder,
                    target,
                    size,
                    0.0,
                    &camera,
                    None,
                    Atmosphere {
                        astronomy: Some((&mut renderer, &draws)),
                        ..Atmosphere::default()
                    },
                )
            })
        };
        let frame = render(0.5)?;
        let mut seen = [false; 2];
        for (index, pixel) in frame.rgba.chunks_exact(4).enumerate() {
            let x = index % size[0] as usize;
            assert_eq!(pixel[3], 255);
            if x < 64 && pixel[0] != 0 {
                assert!((128..=191).contains(&pixel[0]));
                assert_eq!(pixel[1], 0);
                seen[0] = true;
            } else if x >= 64 && pixel[1] != 0 {
                assert!((128..=191).contains(&pixel[1]));
                assert_eq!(pixel[0], 0);
                seen[1] = true;
            }
        }
        assert_eq!(seen, [true, true]);
        assert!(
            render(0.0)?
                .rgba
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 0, 255])
        );
        Ok(())
    }
}
