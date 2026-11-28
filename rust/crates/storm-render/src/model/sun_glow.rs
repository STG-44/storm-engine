use super::{Gpu, OrbitCamera, SkyDraw, depth_view, opaque_depth, upload_texture};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use std::{
    ops::Range,
    rc::{Rc, Weak},
};
use storm_formats::texture::Texture;

#[derive(Clone)]
pub struct SunDraw {
    pub direction: [f32; 3],
    pub distance: f32,
    pub disc: Option<SunSprite>,
    pub halo: Option<SunSprite>,
    pub halo_alpha: f32,
    pub overflow: Option<OverflowDraw>,
    pub flares: Option<FlareDraw>,
}

#[derive(Clone)]
pub struct LightningDraw {
    pub bolt: Option<Rc<Texture>>,
    pub flash: Option<Rc<Texture>>,
    pub position: [f32; 3],
    pub bolt_size: [f32; 2],
    pub flash_size: f32,
    pub tile: u32,
    pub tiles: [u32; 2],
    pub intensity: u8,
}

#[derive(Clone)]
pub struct SunSprite {
    pub texture: Rc<Texture>,
    pub size: f32,
    pub color: [u8; 3],
}

#[derive(Clone)]
pub struct OverflowDraw {
    pub sprite: SunSprite,
    pub start: f32,
    pub alpha: f32,
}

#[derive(Clone, Copy)]
pub struct Flare {
    pub distance: f32,
    pub size: f32,
    pub tile: u32,
    pub color: [u8; 3],
}

#[derive(Clone)]
pub struct FlareDraw {
    pub atlas: Rc<Texture>,
    pub entries: Vec<Flare>,
    pub distance: f32,
    pub scale: f32,
    pub grid: [u32; 2],
    pub alpha: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    clip: [f32; 4],
    uv: [f32; 2],
    color: [u8; 4],
    height: f32,
}

struct Batch {
    range: Range<u32>,
    texture: usize,
    depth_test: bool,
}

struct Billboard {
    center: Vec3,
    size: [f32; 2],
    angle: f32,
    color: [u8; 3],
    uv: [f32; 4],
    horizon: bool,
    background: bool,
}

struct View {
    eye: Vec3,
    forward: Vec3,
    right: Vec3,
    up: Vec3,
    projection: Mat4,
}

impl View {
    fn new(camera: &OrbitCamera, size: [u32; 2], seconds: f32) -> Result<Self> {
        let uniform = camera.uniform(size[0], size[1], seconds)?;
        let [x, y, z, _] = uniform.eye_time;
        let eye = Vec3::new(x, y, z);
        let forward = (camera.target - eye).normalize();
        let right = forward.cross(Vec3::Y).normalize();
        let up = right.cross(forward);
        Ok(Self {
            eye,
            forward,
            right,
            up,
            projection: Mat4::from_cols_array_2d(&uniform.view_projection),
        })
    }

    fn project(&self, billboard: &Billboard) -> [Vertex; 6] {
        let (sin, cos) = billboard.angle.sin_cos();
        let (right, up) = if billboard.background {
            (self.right, self.up)
        } else {
            let transpose = self.projection.transpose();
            (
                transpose.x_axis.truncate().normalize(),
                transpose.y_axis.truncate().normalize(),
            )
        };
        let right_offset = billboard.size[0] * (right * cos + up * sin);
        let up_offset = billboard.size[1] * (up * cos - right * sin);
        let [u0, v0, u1, v1] = billboard.uv;
        let [r, g, b] = billboard.color;
        let corners = [
            (-right_offset + up_offset, [u0, v0]),
            (-right_offset - up_offset, [u0, v1]),
            (right_offset - up_offset, [u1, v1]),
            (right_offset + up_offset, [u1, v0]),
        ]
        .map(|(offset, uv)| {
            let world = billboard.center + offset;
            let mut clip = self.projection * world.extend(1.0);
            if billboard.background {
                clip.z = 0.999_999 * clip.w;
            }
            Vertex {
                clip: clip.to_array(),
                uv,
                color: [r, g, b, 255],
                height: if billboard.horizon { world.y } else { 0.0 },
            }
        });
        [0, 1, 2, 0, 2, 3].map(|index| corners[index])
    }
}

pub struct SunRenderer {
    depth_tested: wgpu::RenderPipeline,
    additive: wgpu::RenderPipeline,
    textures: Vec<(Weak<Texture>, wgpu::BindGroup)>,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    buffer: Option<wgpu::Buffer>,
    vertices: Vec<Vertex>,
    batches: Vec<Batch>,
    depth: Option<([u32; 2], wgpu::TextureView)>,
}

impl SunRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("../sun_glow.wgsl"));
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sun glow sprite"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sun glow layout"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |label, compare| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex"),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: size_of::<Vertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &wgpu::vertex_attr_array![
                            0 => Float32x4, 1 => Float32x2, 2 => Unorm8x4, 3 => Float32
                        ],
                    })],
                    compilation_options: Default::default(),
                },
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    depth_write_enabled: Some(false),
                    depth_compare: Some(compare),
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
                                src_factor: wgpu::BlendFactor::One,
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
            })
        };
        Self {
            depth_tested: pipeline("Sun and moon discs", wgpu::CompareFunction::LessEqual),
            additive: pipeline("Sun glow and lens flares", wgpu::CompareFunction::Always),
            textures: Vec::new(),
            layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            buffer: None,
            vertices: Vec::new(),
            batches: Vec::new(),
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
        sun: &[SunDraw],
        standalone: bool,
        sky: Option<&SkyDraw>,
    ) -> Result<()> {
        ensure!(sun.len() <= 16, "sun glow scene draw limit");
        let view = View::new(camera, size, seconds)?;
        self.clear();
        for draw in sun {
            self.prepare_draw(gpu, &view, draw, sky)?;
        }
        self.upload(gpu, size, standalone)
    }

    pub(super) fn prepare_lightning(
        &mut self,
        gpu: &Gpu,
        size: [u32; 2],
        seconds: f32,
        camera: &OrbitCamera,
        strikes: &[LightningDraw],
        standalone: bool,
    ) -> Result<()> {
        ensure!(strikes.len() <= 128, "lightning scene draw limit");
        let view = View::new(camera, size, seconds)?;
        self.clear();
        let deform_y = size[1] as f32 * 4.0 / (size[0] as f32 * 3.0);
        for draw in strikes {
            let [x, y, z] = draw.position;
            let billboard = |[width, height]: [f32; 2], uv| Billboard {
                center: Vec3::new(x, y, -z),
                size: [width, height * deform_y],
                angle: 0.0,
                color: [draw.intensity; 3],
                uv,
                horizon: false,
                background: false,
            };
            if let Some(bolt) = &draw.bolt {
                let uv = atlas_uv(draw.tile, draw.tiles);
                self.push(gpu, &view, bolt, billboard(draw.bolt_size, uv), true)?;
            }
            if let Some(flash) = &draw.flash {
                let uv = [0.0, 0.0, 1.0, 1.0];
                self.push(
                    gpu,
                    &view,
                    flash,
                    billboard([draw.flash_size; 2], uv),
                    false,
                )?;
            }
        }
        self.upload(gpu, size, standalone)
    }

    fn clear(&mut self) {
        self.textures
            .retain(|(source, _)| source.strong_count() != 0);
        self.vertices.clear();
        self.batches.clear();
    }

    fn upload(&mut self, gpu: &Gpu, size: [u32; 2], standalone: bool) -> Result<()> {
        if standalone && self.depth.as_ref().is_none_or(|(old, _)| *old != size) {
            self.depth = Some((size, depth_view(&gpu.device, size[0], size[1])));
        }
        let data = bytemuck::cast_slice(&self.vertices);
        if !data.is_empty() {
            let bytes = data.len() as u64;
            ensure!(
                bytes <= gpu.device.limits().max_buffer_size,
                "sun glow vertex buffer limit"
            );
            if self
                .buffer
                .as_ref()
                .is_none_or(|buffer| buffer.size() < bytes)
            {
                self.buffer = Some(
                    gpu.device.create_buffer(&wgpu::BufferDescriptor {
                        label: Some("Sun glow vertices"),
                        size: bytes
                            .next_power_of_two()
                            .min(gpu.device.limits().max_buffer_size),
                        usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                        mapped_at_creation: false,
                    }),
                );
            }
            gpu.queue
                .write_buffer(self.buffer.as_ref().expect("sun glow buffer"), 0, data);
        }
        Ok(())
    }

    fn prepare_draw(
        &mut self,
        gpu: &Gpu,
        view: &View,
        draw: &SunDraw,
        sky: Option<&SkyDraw>,
    ) -> Result<()> {
        let [x, y, z] = draw.direction;
        let Some(direction) = Vec3::new(x, y, -z).try_normalize() else {
            return Ok(());
        };
        if !draw.distance.is_finite() || draw.distance <= 0.0 {
            return Ok(());
        }
        let center = view.eye + draw.distance * direction;
        let transmission = sky.map_or(1.0, |sky| sky.transmittance(center, view.eye));
        let sprite = |sprite: &SunSprite, alpha, angle, horizon| Billboard {
            center,
            size: [sprite.size; 2],
            angle,
            color: scaled(sprite.color, alpha),
            uv: [0.0, 0.0, 1.0, 1.0],
            horizon,
            background: true,
        };
        if let Some(disc) = &draw.disc {
            self.push(gpu, view, &disc.texture, sprite(disc, 1.0, 0.0, true), true)?;
        }
        if let Some(halo) = &draw.halo {
            let angle = direction.dot(view.right);
            let cloud_fade = (transmission - 1.5 * (1.0 - transmission)).max(0.0);
            self.push(
                gpu,
                view,
                &halo.texture,
                sprite(halo, draw.halo_alpha * cloud_fade, angle, true),
                false,
            )?;
        }
        if let Some(overflow) = &draw.overflow {
            let dot = view.forward.dot(center.normalize_or_zero());
            let facing = if overflow.start < 1.0 && dot > overflow.start {
                (dot - overflow.start) / (1.0 - overflow.start)
            } else {
                0.0
            };
            let billboard = sprite(
                &overflow.sprite,
                transmission * overflow.alpha.min(facing),
                0.0,
                false,
            );
            self.push(gpu, view, &overflow.sprite.texture, billboard, false)?;
        }
        let projected = view.projection * center.extend(1.0);
        if let Some(flares) = &draw.flares
            && projected.w > 0.0
            && projected.x.abs() <= projected.w
            && projected.y.abs() <= projected.w
        {
            ensure!(flares.entries.len() <= 128, "sun glow flare draw limit");
            let midpoint = view.eye + view.forward * (flares.distance / 2.0);
            let delta = flares.distance * (midpoint - center).normalize_or_zero();
            for flare in &flares.entries {
                let billboard = Billboard {
                    center: center + delta * (1.0 - flare.distance),
                    size: [flare.size * flares.scale; 2],
                    angle: 0.0,
                    color: scaled(flare.color, transmission * flares.alpha),
                    uv: atlas_uv(flare.tile, flares.grid),
                    horizon: false,
                    background: true,
                };
                self.push(gpu, view, &flares.atlas, billboard, false)?;
            }
        }
        Ok(())
    }

    fn push(
        &mut self,
        gpu: &Gpu,
        view: &View,
        texture: &Rc<Texture>,
        quad: Billboard,
        depth_test: bool,
    ) -> Result<()> {
        if quad
            .size
            .iter()
            .any(|size| !size.is_finite() || *size <= 0.0)
            || quad.color == [0; 3]
        {
            return Ok(());
        }
        let source = Rc::downgrade(texture);
        let index = if let Some(index) = self
            .textures
            .iter()
            .position(|(key, _)| key.ptr_eq(&source))
        {
            index
        } else {
            let view = upload_texture(gpu, texture)?;
            let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Sun glow source texture"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.sampler),
                    },
                ],
            });
            let index = self.textures.len();
            self.textures.push((source, group));
            index
        };
        let first = self.vertices.len() as u32;
        self.vertices.extend(view.project(&quad));
        self.batches.push(Batch {
            range: first..first + 6,
            texture: index,
            depth_test,
        });
        Ok(())
    }

    pub(super) fn depth(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|(_, depth)| depth)
    }

    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>, depth_test: bool) {
        if let Some(buffer) = &self.buffer {
            pass.set_vertex_buffer(0, buffer.slice(..));
            pass.set_pipeline(if depth_test {
                &self.depth_tested
            } else {
                &self.additive
            });
            for batch in self
                .batches
                .iter()
                .filter(|batch| batch.depth_test == depth_test)
            {
                pass.set_bind_group(0, &self.textures[batch.texture].1, &[]);
                pass.draw(batch.range.clone(), 0..1);
            }
        }
    }
}

fn scaled(rgb: [u8; 3], alpha: f32) -> [u8; 3] {
    rgb.map(|channel| (channel as f32 * alpha.clamp(0.0, 1.0)) as u8)
}

fn atlas_uv(tile: u32, [x, y]: [u32; 2]) -> [f32; 4] {
    let (x, y) = (x.max(1), y.max(1));
    let (column, row) = (tile % x, (tile / x) % y);
    [
        column as f32 / x as f32,
        row as f32 / y as f32,
        (column + 1) as f32 / x as f32,
        (row + 1) as f32 / y as f32,
    ]
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
    fn celestial_sprites_blend_additively_and_clip_to_the_world_horizon() -> Result<()> {
        let instance = create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let mut camera = OrbitCamera::from_bounds(Vec3::ZERO, 1.0);
        camera.pitch = 0.0;
        camera.yaw = 0.0;
        let mut native_texture = [0u32, 1, 1, 1, 21, 4].map(u32::to_le_bytes).concat();
        native_texture.extend([255; 4]);
        let texture = Rc::new(Texture::read(&native_texture)?);
        let mut renderer = SunRenderer::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
        let draw = SunDraw {
            direction: [0.0, 0.0, 1.0],
            distance: 3500.0,
            disc: Some(SunSprite {
                texture: texture.clone(),
                size: 400.0,
                color: [255, 0, 0],
            }),
            halo: Some(SunSprite {
                texture,
                size: 600.0,
                color: [0, 255, 0],
            }),
            halo_alpha: 0.5,
            overflow: None,
            flares: None,
        };
        let size = [96, 96];
        let frame = render_offscreen_with(&gpu, size[0], size[1], 0.0, |encoder, target| {
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
                    sun: Some((&mut renderer, std::slice::from_ref(&draw))),
                    ..Atmosphere::default()
                },
            )
        })?;
        let pixel = |x: usize, y: usize| &frame.rgba[4 * (y * size[0] as usize + x)..][..4];
        let above = pixel(48, 40);
        assert_eq!(above[0], 255);
        assert!(above[1] > 127 && above[1] < 200);
        assert_eq!(pixel(48, 55), [0, 0, 0, 255]);
        assert_eq!(atlas_uv(2, [2, 2]), [0.0, 0.5, 0.5, 1.0]);
        Ok(())
    }
}
