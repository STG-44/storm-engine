use super::{Gpu, OrbitCamera, depth_view, opaque_depth, upload_texture};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use std::{f32::consts::TAU, rc::Rc, sync::LazyLock};
use storm_formats::texture::Texture;
use wgpu::util::DeviceExt;

#[derive(Default)]
pub struct SkyFaces(pub [Option<Rc<Texture>>; 5]);

#[derive(Clone)]
pub struct SkyDraw {
    pub current: Rc<SkyFaces>,
    pub next: Rc<SkyFaces>,
    pub size: f32,
    pub angle: f32,
    pub blend: f32,
    pub celestial: bool,
    pub fog: Option<Rc<[[u8; 4]]>>,
}

impl SkyDraw {
    pub fn has_textures(&self) -> bool {
        self.size > 0.0
            && self
                .current
                .0
                .iter()
                .chain(&self.next.0)
                .any(Option::is_some)
    }

    pub(super) fn transmittance(&self, world: Vec3, eye: Vec3) -> f32 {
        if !self.has_textures() {
            return 1.0;
        }
        let offset = world - Vec3::new(eye.x, eye.y / 6.0, eye.z);
        let local = Mat4::from_rotation_y(-self.angle)
            .transform_vector3(Vec3::new(offset.x, offset.y, -offset.z));
        let (x, y, z) = (local.x, local.y.max(0.0), local.z);
        let max = x.abs().max(y).max(z.abs());
        if max == 0.0 {
            return 1.0;
        }
        let (face, u, v) = if y == max {
            (4, -x / max, z / max)
        } else {
            let (face, u) = if z == -max {
                (0, x)
            } else if z == max {
                (2, -x)
            } else if x == max {
                (1, z)
            } else {
                (3, -z)
            };
            (face, u / max, 1.0 - 2.0 * y / max)
        };
        let sample = |faces: &SkyFaces| {
            let Some(mip) = faces.0[face]
                .as_ref()
                .and_then(|texture| texture.mips.first())
            else {
                return 1.0;
            };
            let col = ((u * 0.5 + 0.5) * (mip.width - 1) as f32) as usize;
            let row = ((v * 0.5 + 0.5) * (mip.height - 1) as f32) as usize;
            mip.rgba[(row * mip.width as usize + col) * 4 + 3] as f32 / 255.0
        };
        let previous = sample(&self.current);
        previous + (sample(&self.next) - previous) * self.blend
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct FaceVertex {
    position: [f32; 3],
    uv: [f32; 2],
}

const SKY_FACE_VERTICES: u32 = 30;

fn face_vertices() -> [FaceVertex; 54] {
    let corners = [
        [-1.0, 0.0, -1.0],
        [1.0, 0.0, -1.0],
        [1.0, 0.0, 1.0],
        [-1.0, 0.0, 1.0],
        [-1.0, 1.0, -1.0],
        [1.0, 1.0, -1.0],
        [1.0, 1.0, 1.0],
        [-1.0, 1.0, 1.0],
    ];
    let faces = [
        [1, 5, 4, 0],
        [2, 6, 5, 1],
        [3, 7, 6, 2],
        [0, 4, 7, 3],
        [5, 6, 7, 4],
    ];
    let inset = 1.0 / 1024.0;
    let right = 1.0 - inset;
    let side_uv = [[right, 1.0], [right, inset], [inset, inset], [inset, 1.0]];
    let top_uv = [
        [inset, inset],
        [inset, right],
        [right, right],
        [right, inset],
    ];
    std::array::from_fn(|i| {
        let below = i >= SKY_FACE_VERTICES as usize;
        let face = (i % SKY_FACE_VERTICES as usize) / 6;
        let corner = [0, 1, 2, 0, 2, 3][i % 6];
        let mut vertex = FaceVertex {
            position: corners[faces[face][corner]],
            uv: if face == 4 {
                top_uv[corner]
            } else {
                side_uv[corner]
            },
        };
        if below {
            // Continue each side's bottom texel behind the distant water.
            vertex.position[1] -= 1.0;
            vertex.uv[1] = 1.0;
        }
        vertex
    })
}

const FOG_RINGS: u16 = 64;
const FOG_SEGMENTS: u16 = 8;
const FOG_RADIUS: f32 = 2000.0;
const FOG_TOP: u16 = FOG_RINGS * FOG_SEGMENTS;

pub fn sky_fog_positions() -> &'static [[f32; 3]] {
    static POSITIONS: LazyLock<Vec<[f32; 3]>> = LazyLock::new(|| {
        let mut vertices = Vec::with_capacity(FOG_TOP as usize + 1);
        for ring in 0..FOG_RINGS {
            let y = ring as f32 * FOG_RADIUS / FOG_RINGS as f32;
            let radius = (FOG_RADIUS.powi(2) - y.powi(2)).sqrt();
            for segment in 0..FOG_SEGMENTS {
                let angle = segment as f32 / FOG_SEGMENTS as f32 * TAU;
                let (sin, cos) = angle.sin_cos();
                vertices.push([radius * cos, y, radius * sin]);
            }
        }
        vertices.push([0.0, FOG_RADIUS, 0.0]);
        vertices
    });
    &POSITIONS
}

fn fog_mesh_indices() -> (Vec<u16>, u32) {
    let mut indices = Vec::new();
    for ring in 0..FOG_RINGS {
        for segment in 0..FOG_SEGMENTS {
            let a = ring * FOG_SEGMENTS + segment;
            let b = ring * FOG_SEGMENTS + (segment + 1) % FOG_SEGMENTS;
            if ring == FOG_RINGS - 1 {
                indices.extend([FOG_TOP, a, b]);
            } else {
                indices.extend([
                    a,
                    b,
                    a + FOG_SEGMENTS,
                    b,
                    a + FOG_SEGMENTS,
                    b + FOG_SEGMENTS,
                ]);
            }
        }
    }
    let upper_count = indices.len() as u32;
    // Carry the fog at the horizon down with the sky. These visual vertices
    // share the bottom ring's colors and need no additional script samples.
    let first_lower = FOG_TOP + 1;
    for a in 0..FOG_SEGMENTS {
        let b = (a + 1) % FOG_SEGMENTS;
        indices.extend([a, first_lower + a, first_lower + b, a, first_lower + b, b]);
    }
    (indices, upper_count)
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct View {
    sky: [[f32; 4]; 4],
    fog: [[f32; 4]; 4],
    blend: [f32; 4],
}

struct BoundFaces {
    source: Rc<SkyFaces>,
    views: [Option<wgpu::TextureView>; 5],
}

impl BoundFaces {
    fn upload(gpu: &Gpu, source: Rc<SkyFaces>, existing: &[Self]) -> Result<Self> {
        if let Some(old) = existing.iter().find(|old| Rc::ptr_eq(&old.source, &source)) {
            return Ok(Self {
                source,
                views: old.views.clone(),
            });
        }
        let mut views = [const { None }; 5];
        for (view, face) in views.iter_mut().zip(&source.0) {
            *view = face
                .as_deref()
                .map(|face| upload_texture(gpu, face))
                .transpose()?;
        }
        Ok(Self { source, views })
    }
}

pub struct SkyRenderer {
    base: wgpu::RenderPipeline,
    clouds: wgpu::RenderPipeline,
    fog: wgpu::RenderPipeline,
    view: wgpu::Buffer,
    view_group: wgpu::BindGroup,
    faces_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    faces: Vec<BoundFaces>,
    face_groups: [Option<wgpu::BindGroup>; 5],
    positions: wgpu::Buffer,
    fog_positions: wgpu::Buffer,
    fog_colors: wgpu::Buffer,
    fog_indices: wgpu::Buffer,
    fog_count: u32,
    fog_upper_count: u32,
    active_fog: Option<Rc<[[u8; 4]]>>,
    depth: Option<([u32; 2], wgpu::TextureView)>,
}

impl SkyRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("../sky.wgsl"));
        let view_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sky view"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let faces_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Sky texture pair"),
            entries: &std::array::from_fn::<_, 3, _>(|binding| wgpu::BindGroupLayoutEntry {
                binding: binding as u32,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: if binding == 2 {
                    wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering)
                } else {
                    wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    }
                },
                count: None,
            }),
        });
        let sky_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Sky pipeline"),
            bind_group_layouts: &[Some(&view_layout), Some(&faces_layout)],
            immediate_size: 0,
        });
        let fog_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Horizon fog pipeline"),
            bind_group_layouts: &[Some(&view_layout)],
            immediate_size: 0,
        });
        let face_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: size_of::<FaceVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        })];
        let fog_buffers = [
            Some(wgpu::VertexBufferLayout {
                array_stride: 12,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![0 => Float32x3],
            }),
            Some(wgpu::VertexBufferLayout {
                array_stride: 4,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![1 => Unorm8x4],
            }),
        ];
        let pipeline = |label, layout, vertex, fragment, buffers, blend| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vertex),
                    buffers,
                    compilation_options: Default::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::COLOR,
                    })],
                }),
                primitive: Default::default(),
                depth_stencil: Some(wgpu::DepthStencilState {
                    depth_write_enabled: Some(false),
                    depth_compare: Some(wgpu::CompareFunction::Always),
                    ..opaque_depth()
                }),
                multisample: Default::default(),
                multiview_mask: None,
                cache: None,
            })
        };
        let base = pipeline(
            "Sky background",
            &sky_layout,
            "face_vertex",
            "face_fragment",
            &face_buffers[..],
            None,
        );
        let clouds = pipeline(
            "Sky cloud layer",
            &sky_layout,
            "face_vertex",
            "face_fragment",
            &face_buffers[..],
            Some(wgpu::BlendState {
                color: wgpu::BlendComponent {
                    src_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                    dst_factor: wgpu::BlendFactor::SrcAlpha,
                    operation: wgpu::BlendOperation::Add,
                },
                alpha: wgpu::BlendComponent::REPLACE,
            }),
        );
        let fog = pipeline(
            "Sky horizon fog",
            &fog_layout,
            "fog_vertex",
            "fog_fragment",
            &fog_buffers[..],
            Some(wgpu::BlendState::ALPHA_BLENDING),
        );
        let view = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Sky matrices"),
            size: size_of::<View>() as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let view_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Sky matrices"),
            layout: &view_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: view.as_entire_binding(),
            }],
        });
        let buffer = |label, contents, usage| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents,
                usage,
            })
        };
        let (indices, fog_upper_count) = fog_mesh_indices();
        let mut fog_positions = sky_fog_positions().to_vec();
        fog_positions.extend(
            sky_fog_positions()[..FOG_SEGMENTS as usize]
                .iter()
                .map(|&[x, _, z]| [x, -FOG_RADIUS, z]),
        );
        Self {
            base,
            clouds,
            fog,
            view,
            view_group,
            faces_layout,
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            faces: Vec::new(),
            face_groups: [const { None }; 5],
            positions: buffer(
                "Sky cube",
                bytemuck::cast_slice(&face_vertices()),
                wgpu::BufferUsages::VERTEX,
            ),
            fog_positions: buffer(
                "Sky horizon mesh",
                bytemuck::cast_slice(&fog_positions),
                wgpu::BufferUsages::VERTEX,
            ),
            fog_colors: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("Sky horizon colors"),
                size: 4 * fog_positions.len() as u64,
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::VERTEX,
                mapped_at_creation: false,
            }),
            fog_indices: buffer(
                "Sky horizon indices",
                bytemuck::cast_slice(&indices),
                wgpu::BufferUsages::INDEX,
            ),
            fog_count: indices.len() as u32,
            fog_upper_count,
            active_fog: None,
            depth: None,
        }
    }

    pub(super) fn prepare(
        &mut self,
        gpu: &Gpu,
        size: [u32; 2],
        seconds: f32,
        camera: &OrbitCamera,
        draw: &SkyDraw,
        standalone: bool,
    ) -> Result<()> {
        let camera = camera.uniform(size[0], size[1], seconds)?;
        let [x, y, z, _] = camera.eye_time;
        let view = Mat4::from_cols_array_2d(&camera.view_projection);
        let position = Mat4::from_translation(Vec3::new(x, y / 6.0, z));
        let reflect = Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        let sky = view
            * position
            * reflect
            * Mat4::from_rotation_y(draw.angle)
            * Mat4::from_scale(Vec3::splat(draw.size));
        let fog = view * position * reflect;
        let matrices = View {
            sky: sky.to_cols_array_2d(),
            fog: fog.to_cols_array_2d(),
            blend: [draw.blend, 0.0, 0.0, 0.0],
        };
        gpu.queue
            .write_buffer(&self.view, 0, bytemuck::bytes_of(&matrices));
        if !draw.has_textures() {
            self.face_groups = [const { None }; 5];
            self.faces.clear();
        } else if self.faces.len() != 2
            || !Rc::ptr_eq(&self.faces[0].source, &draw.current)
            || !Rc::ptr_eq(&self.faces[1].source, &draw.next)
        {
            let first = BoundFaces::upload(gpu, draw.current.clone(), &self.faces)?;
            let second = if Rc::ptr_eq(&draw.current, &draw.next) {
                BoundFaces {
                    source: first.source.clone(),
                    views: first.views.clone(),
                }
            } else {
                BoundFaces::upload(gpu, draw.next.clone(), &self.faces)?
            };
            self.face_groups = std::array::from_fn(|face| {
                let current = first.views[face].as_ref().or(second.views[face].as_ref())?;
                let next = second.views[face].as_ref().unwrap_or(current);
                Some(gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Sky texture pair"),
                    layout: &self.faces_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(current),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::TextureView(next),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                }))
            });
            self.faces = vec![first, second];
        }
        if let Some(colors) = &draw.fog {
            ensure!(
                colors.len() == sky_fog_positions().len(),
                "invalid Sky fog vertex colors"
            );
            if self
                .active_fog
                .as_ref()
                .is_none_or(|old| !Rc::ptr_eq(old, colors))
            {
                gpu.queue
                    .write_buffer(&self.fog_colors, 0, bytemuck::cast_slice(colors));
                gpu.queue.write_buffer(
                    &self.fog_colors,
                    colors.len() as u64 * 4,
                    bytemuck::cast_slice(&colors[..FOG_SEGMENTS as usize]),
                );
            }
        }
        self.active_fog = draw.fog.clone();
        if standalone && self.depth.as_ref().is_none_or(|(old, _)| *old != size) {
            self.depth = Some((size, depth_view(&gpu.device, size[0], size[1])));
        }
        Ok(())
    }

    pub(super) fn depth(&self) -> Option<&wgpu::TextureView> {
        self.depth.as_ref().map(|(_, view)| view)
    }

    pub(super) fn encode_faces(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        clouds: bool,
        below_horizon: bool,
    ) {
        pass.set_pipeline(if clouds { &self.clouds } else { &self.base });
        pass.set_bind_group(0, &self.view_group, &[]);
        pass.set_vertex_buffer(0, self.positions.slice(..));
        for (index, group) in self.face_groups.iter().enumerate() {
            if let Some(group) = group {
                pass.set_bind_group(1, group, &[]);
                let first = index as u32 * 6;
                pass.draw(first..first + 6, 0..1);
                if below_horizon && index < 4 {
                    let lower = SKY_FACE_VERTICES + first;
                    pass.draw(lower..lower + 6, 0..1);
                }
            }
        }
    }

    pub(super) fn encode_fog(&self, pass: &mut wgpu::RenderPass<'_>, below_horizon: bool) {
        if self.active_fog.is_some() {
            pass.set_pipeline(&self.fog);
            pass.set_bind_group(0, &self.view_group, &[]);
            pass.set_vertex_buffer(0, self.fog_positions.slice(..));
            pass.set_vertex_buffer(1, self.fog_colors.slice(..));
            pass.set_index_buffer(self.fog_indices.slice(..), wgpu::IndexFormat::Uint16);
            let count = if below_horizon {
                self.fog_count
            } else {
                self.fog_upper_count
            };
            pass.draw_indexed(0..count, 0, 0..1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        create_instance,
        model::{
            AstronomyRenderer, Atmosphere, CelestialDraw, CelestialSprite, ModelScene,
            OceanRenderer,
        },
        render_offscreen_with,
    };
    use std::sync::Arc;
    use storm_sim::sea::{Frames, Parameters, Sea};

    fn tex([r, g, b, a]: [u8; 4]) -> Result<Rc<Texture>> {
        let mut tx = [0u32, 1, 1, 1, 21, 4].map(u32::to_le_bytes).concat();
        tx.extend([b, g, r, a]);
        Ok(Rc::new(Texture::read(&tx)?))
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn configured_water_fades_into_the_sky_horizon_with_and_without_fog() -> Result<()> {
        let instance = create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut sky = SkyRenderer::new(&gpu, format);
        let mut water = OceanRenderer::new(&gpu, format);
        let mut camera = OrbitCamera::from_bounds(Vec3::new(0.0, 12.0, 0.0), 2.0);
        camera.pitch = 0.0;
        camera.yaw = 0.0;
        let ocean = Sea::new(
            Arc::new(Frames::fixture()),
            Parameters {
                amplitude: [0.0; 2],
                ..Parameters::default()
            },
        )?;
        let texture = tex([87, 32, 123, 255])?;
        let faces = Rc::new(SkyFaces(std::array::from_fn(|_| Some(texture.clone()))));
        let mut draw = SkyDraw {
            current: faces.clone(),
            next: faces,
            size: 2048.0,
            angle: 0.0,
            blend: 0.0,
            celestial: false,
            fog: None,
        };
        let size = [96, 400];
        let mut render = |draw: &SkyDraw| {
            render_offscreen_with(&gpu, size[0], size[1], 0.0, |encoder, target| {
                ModelScene::draw_configured_with_atmosphere(
                    &mut [],
                    &gpu,
                    encoder,
                    target,
                    size,
                    0.0,
                    &camera,
                    Some((&mut water, ocean.surface())),
                    Atmosphere {
                        sky: Some((&mut sky, draw)),
                        ..Atmosphere::default()
                    },
                )
            })
        };
        for fog in [
            None,
            Some(vec![[120, 61, 30, 255]; sky_fog_positions().len()].into()),
        ] {
            draw.fog = fog;
            let frame = render(&draw)?;
            let pixel = |row: usize| {
                let offset = 4 * (row * size[0] as usize + size[0] as usize / 2);
                &frame.rgba[offset..offset + 4]
            };
            let horizon = pixel(198);
            if draw.fog.is_none() {
                assert_eq!(horizon, [87, 32, 123, 255]);
            } else {
                assert!(horizon[0] > 120 && horizon[1] > 61 && horizon[2] < 123);
            }
            // Row 204 is below the original texture and fog boundaries.
            for (actual, expected) in pixel(204).iter().zip(horizon) {
                assert!(
                    actual.abs_diff(*expected) <= 10,
                    "{:?} vs {:?}",
                    pixel(204),
                    horizon
                );
            }
            assert_ne!(pixel(350), horizon, "nearby waves remain visible");
        }
        Ok(())
    }

    #[test]
    #[ignore = "requires a native GPU or software Vulkan"]
    fn sky_blends_texture_pairs_and_cloud_alpha_masks_celestial_draws() -> Result<()> {
        let instance = create_instance(wgpu::Backends::PRIMARY)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let format = wgpu::TextureFormat::Rgba8UnormSrgb;
        let mut sky = SkyRenderer::new(&gpu, format);
        let mut astro = AstronomyRenderer::new(&gpu, format);
        let mut camera = OrbitCamera::from_bounds(Vec3::ZERO, 1.0);
        camera.yaw = 0.0;
        camera.pitch = -0.15;
        let stars = [CelestialDraw {
            texture: tex([0, 255, 0, 255])?,
            sprites: Rc::from([CelestialSprite::new(
                [0.0, 30.0, 200.0],
                10.0,
                [255; 3],
                1.0,
                0.0,
            )]),
            magnitudes: None,
        }];
        let make_sky = |alpha| -> Result<SkyDraw> {
            let red = tex([255, 0, 0, alpha])?;
            let blue = tex([0, 0, 255, alpha])?;
            Ok(SkyDraw {
                current: Rc::new(SkyFaces(std::array::from_fn(|_| Some(red.clone())))),
                next: Rc::new(SkyFaces(std::array::from_fn(|_| Some(blue.clone())))),
                size: 2048.0,
                angle: 0.0,
                blend: 0.5,
                celestial: true,
                fog: None,
            })
        };
        let mut render = |draw: &SkyDraw| {
            render_offscreen_with(&gpu, 96, 96, 0.0, |encoder, target| {
                ModelScene::draw_configured_with_atmosphere(
                    &mut [],
                    &gpu,
                    encoder,
                    target,
                    [96, 96],
                    0.0,
                    &camera,
                    None,
                    Atmosphere {
                        sky: Some((&mut sky, draw)),
                        astronomy: Some((&mut astro, &stars)),
                        ..Atmosphere::default()
                    },
                )
            })
        };
        let cloudy = make_sky(0)?;
        assert_eq!(
            cloudy.transmittance(Vec3::new(0.0, 25.0, -100.0), Vec3::ZERO),
            0.0
        );
        let hidden = render(&cloudy)?;
        let center = 4 * (48 * 96 + 48);
        let mixed = &hidden.rgba[center..center + 4];
        assert!((185..=190).contains(&mixed[0]));
        assert_eq!(mixed[0], mixed[2]);
        assert_eq!((mixed[1], mixed[3]), (0, 255));
        let clear = make_sky(255)?;
        assert_eq!(
            clear.transmittance(Vec3::new(0.0, 25.0, -100.0), Vec3::ZERO),
            1.0
        );
        let visible = render(&clear)?;
        assert_eq!(&visible.rgba[center..center + 4], &[0, 255, 0, 255]);
        Ok(())
    }
}
