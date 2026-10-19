//! Native model preview: original mesh hierarchy/textures, preview cloth and water.
mod assets;
mod crew;
mod effects;
mod rigging;
mod sail_damage;
mod sails;
pub use sail_damage::{SailDamageMaterial, SailDamageSettings, SailImpact};
mod ship;
pub use crew::{CrewPatch, Patrol, Skeleton};
pub use effects::{DynamicMesh, ParticlePatch};
pub use rigging::{FlagOrigin, FlagPatch, RigMesh, RiggingConfig, RiggingStats};
pub use ship::{
    FireEmission, FirePoint, ShipAsset, ShipNode, ShipState, mast_falls_with, part_number,
};
#[cfg(test)]
mod presentation_tests;
#[cfg(test)]
mod tests;
use crate::Gpu;
use anyhow::{Result, ensure};
pub use assets::{LoadOptions, Mesh, ModelAsset, read_asset_bytes};
use glam::{Mat4, Vec3};
pub use sails::{SailOrigin, SailPatch, SailShape, SailWind};
use wgpu::util::DeviceExt;

pub const MODEL_SHADER: &str = include_str!("model.wgsl");
pub const OCEAN_SHADER: &str =
    concat!(include_str!("waves.wgsl"), "\n", include_str!("ocean.wgsl"));

const MODEL_ATTRIBUTES: [wgpu::VertexAttribute; 4] =
    wgpu::vertex_attr_array![0=>Float32x3,1=>Float32x3,2=>Float32x2,3=>Float32x4];

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ModelVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

/// Rigid transform in renderer coordinates; no ship physics is implemented here.
#[derive(Clone, Copy, Default)]
pub struct ModelPose {
    pub translation: [f32; 3],
    pub yaw: f32,
    /// Right-handed rotations about local X then Z: T * Ry * Rx * Rz.
    pub pitch: f32,
    pub roll: f32,
}
impl ModelPose {
    pub fn matrix(self) -> Result<Mat4> {
        ensure!(
            self.translation.iter().all(|v| v.is_finite())
                && [self.yaw, self.pitch, self.roll]
                    .iter()
                    .all(|v| v.is_finite()),
            "invalid model pose"
        );
        Ok(Mat4::from_translation(Vec3::from_array(self.translation))
            * Mat4::from_rotation_y(self.yaw)
            * Mat4::from_rotation_x(self.pitch)
            * Mat4::from_rotation_z(self.roll))
    }
    pub fn transform_point(self, point: Vec3) -> Result<Vec3> {
        Ok(self.matrix()?.transform_point3(point))
    }
}

#[derive(Clone)]
pub struct OrbitCamera {
    pub target: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub distance: f32,
    pub radius: f32,
}
impl OrbitCamera {
    pub fn new(asset: &ModelAsset) -> Self {
        Self {
            target: asset.center,
            yaw: 0.85,
            pitch: 0.27,
            distance: asset.radius * 2.2,
            radius: asset.radius,
        }
    }
    /// Centre-screen swept test shot; the camera and target are world positions.
    pub fn shot_segment(&self) -> (Vec3, Vec3) {
        let direction = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        (
            self.target + direction * self.distance,
            self.target - direction * self.radius * 2.0,
        )
    }
    pub fn orbit(&mut self, dx: f32, dy: f32) {
        self.yaw += dx * 0.008;
        self.pitch = (self.pitch + dy * 0.008).clamp(0.08, 1.4);
    }
    pub fn zoom(&mut self, delta: f32) {
        self.distance =
            (self.distance * (-delta * 0.12).exp()).clamp(self.radius * 0.25, self.radius * 12.0);
    }
    fn uniform(&self, width: u32, height: u32, seconds: f32) -> Result<CameraUniform> {
        ensure!(
            width > 0 && height > 0 && seconds.is_finite(),
            "invalid model frame parameters"
        );
        ensure!(
            self.target.is_finite()
                && self.yaw.is_finite()
                && self.pitch.is_finite()
                && self.distance.is_finite()
                && self.distance > 0.0
                && self.radius.is_finite()
                && self.radius > 0.0,
            "invalid orbit camera"
        );
        let direction = Vec3::new(
            self.yaw.sin() * self.pitch.cos(),
            self.pitch.sin(),
            self.yaw.cos() * self.pitch.cos(),
        );
        let eye = self.target + direction * self.distance;
        let view = glam::camera::rh::view::look_at_mat4(eye, self.target, Vec3::Y);
        let projection = glam::camera::rh::proj::directx::perspective(
            45_f32.to_radians(),
            width as f32 / height as f32,
            (self.radius * 0.002).max(0.01),
            self.radius * 100.0,
        );
        let matrix = projection * view;
        ensure!(
            matrix.is_finite() && matrix.inverse().is_finite(),
            "invalid camera projection"
        );
        Ok(CameraUniform {
            view_projection: matrix.to_cols_array_2d(),
            inverse_view_projection: matrix.inverse().to_cols_array_2d(),
            eye_time: [eye.x, eye.y, eye.z, seconds],
            light: [-0.45, 0.8, 0.4, self.radius],
        })
    }
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
    inverse_view_projection: [[f32; 4]; 4],
    eye_time: [f32; 4],
    light: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct ObjectUniform {
    matrix: [[f32; 4]; 4],
    damage: [u32; 4],
}
impl ObjectUniform {
    fn new(matrix: Mat4, mask: u16, triangle: bool) -> Self {
        Self {
            matrix: matrix.to_cols_array_2d(),
            damage: [u32::from(mask), u32::from(triangle), 0, 0],
        }
    }
}
const OBJECT_BYTES: usize = std::mem::size_of::<ObjectUniform>();

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WaveUniform {
    // X/Z wave numbers, angular frequency, amplitude; phase in a separate vec4.
    components: [[f32; 4]; 2],
    phases: [f32; 4],
}
impl From<storm_sim::Waves> for WaveUniform {
    fn from(waves: storm_sim::Waves) -> Self {
        let components = waves.components();
        Self {
            components: components.map(|c| {
                [
                    c.k.x as f32,
                    c.k.y as f32,
                    c.omega as f32,
                    c.amplitude as f32,
                ]
            }),
            phases: [
                components[0].phase as f32,
                components[1].phase as f32,
                0.0,
                0.0,
            ],
        }
    }
}
struct DrawMesh {
    damage_mask: u16,
    normal_count: u32,
    ship_node: Option<String>,
    dynamic: Option<(DynamicMesh, Vec<ModelVertex>)>,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    texture: usize,
    sail: Option<(SailPatch, Vec<ModelVertex>)>,
    rigging: Option<(RigMesh, Vec<ModelVertex>)>,
}

pub struct ModelScene {
    sail_pipelines: [wgpu::RenderPipeline; 2],
    particle_pipeline: wgpu::RenderPipeline,
    ship: ShipAsset,
    ship_state: ShipState,
    pose_stride: u32,
    pose_bytes: Vec<u8>,
    pipeline: wgpu::RenderPipeline,
    ocean: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    pose_buffer: wgpu::Buffer,
    pose: ModelPose,
    waves: storm_sim::Waves,
    rig: storm_sim::RigState,
    wave_buffer: wgpu::Buffer,
    camera_group: wgpu::BindGroup,
    textures: Vec<wgpu::BindGroup>,
    meshes: Vec<DrawMesh>,
    depth: wgpu::TextureView,
    size: (u32, u32),
}

fn depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("Model depth"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

fn upload_model_texture(
    gpu: &Gpu,
    source: &storm_formats::texture::Texture,
) -> Result<wgpu::TextureView> {
    let h = source.header;
    ensure!(
        h.width <= gpu.device.limits().max_texture_dimension_2d
            && h.height <= gpu.device.limits().max_texture_dimension_2d,
        "model texture exceeds GPU dimensions"
    );
    let texture = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Decoded original texture"),
        size: wgpu::Extent3d {
            width: h.width,
            height: h.height,
            depth_or_array_layers: 1,
        },
        mip_level_count: source.mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, mip) in source.mips.iter().enumerate() {
        gpu.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            &mip.rgba,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(mip.width * 4),
                rows_per_image: Some(mip.height),
            },
            wgpu::Extent3d {
                width: mip.width,
                height: mip.height,
                depth_or_array_layers: 1,
            },
        );
    }
    Ok(texture.create_view(&Default::default()))
}

impl ModelScene {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, asset: &ModelAsset) -> Result<Self> {
        asset.ship_state.validate(&asset.ship)?;
        sail_damage::validate_holes(
            &asset.ship_state,
            asset
                .meshes
                .iter()
                .enumerate()
                .filter_map(|(i, m)| m.sail.as_ref().map(|p| (i, p))),
        )?;
        ensure!(asset.meshes.len() <= 4096, "too many model draw meshes");
        let pose_stride = (OBJECT_BYTES as u32)
            .div_ceil(gpu.device.limits().min_uniform_buffer_offset_alignment)
            * gpu.device.limits().min_uniform_buffer_offset_alignment;
        let pose_bytes = vec![0u8; pose_stride as usize * (asset.meshes.len() + 1)];
        let device = &gpu.device;
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Model camera layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: true,
                        min_binding_size: None,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let texture_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Model texture layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Orbit camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let pose_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Rigid model pose"),
            size: pose_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let wave_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Shared wave coefficients"),
            contents: bytemuck::bytes_of(&WaveUniform::from(storm_sim::Waves::default())),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model camera"),
            layout: &camera_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: camera_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &pose_buffer,
                        offset: 0,
                        size: std::num::NonZeroU64::new(OBJECT_BYTES as u64),
                    }),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wave_buffer.as_entire_binding(),
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Original texture sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let mut textures = Vec::new();
        let flag_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Flag atlas sampler without cross-cell mip bleeding"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            lod_max_clamp: 0.0,
            ..Default::default()
        });
        let fallback_mask = SailDamageMaterial::fixture();
        let mask_view = upload_model_texture(
            gpu,
            asset
                .sail_damage
                .as_ref()
                .map_or(&fallback_mask.texture, |m| &m.texture),
        )?;
        let hole_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Original damaged sail mirror-U sampler"),
            address_mode_u: wgpu::AddressMode::MirrorRepeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        for (texture_id, source) in asset.textures.iter().enumerate() {
            let is_flag = asset
                .meshes
                .iter()
                .any(|m| m.texture == texture_id && matches!(m.rigging, Some(RigMesh::Flag(_))));
            let view = upload_model_texture(gpu, source)?;
            textures.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Model material"),
                layout: &texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(if is_flag {
                            &flag_sampler
                        } else {
                            &sampler
                        }),
                    },
                    wgpu::BindGroupEntry {
                        binding: 2,
                        resource: wgpu::BindingResource::TextureView(&mask_view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 3,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                    wgpu::BindGroupEntry {
                        binding: 4,
                        resource: wgpu::BindingResource::Sampler(&hole_sampler),
                    },
                ],
            }));
        }
        let depth = Some(wgpu::DepthStencilState {
            format: wgpu::TextureFormat::Depth32Float,
            depth_write_enabled: Some(true),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            bias: Default::default(),
        });
        let target = Some(wgpu::ColorTargetState {
            format,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Original model preview"),
            source: wgpu::ShaderSource::Wgsl(MODEL_SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Model pipeline layout"),
            bind_group_layouts: &[Some(&camera_layout), Some(&texture_layout)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Original model pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ModelVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &MODEL_ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: depth.clone(),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: std::slice::from_ref(&target),
            }),
            multiview_mask: None,
            cache: None,
        });
        let make_sail_pipeline = |entry: &str| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vertex_main"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<ModelVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &MODEL_ATTRIBUTES,
                    })],
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: None,
                    ..Default::default()
                },
                depth_stencil: depth.clone(),
                multisample: Default::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
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
        };
        let sail_pipelines = [
            make_sail_pipeline("sail_intact_fragment"),
            make_sail_pipeline("sail_hole_fragment"),
        ];
        let particle_target = Some(wgpu::ColorTargetState {
            format,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        });
        let mut particle_depth = depth.clone();
        particle_depth.as_mut().unwrap().depth_write_enabled = Some(false);
        let particle_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Ship fire and smoke"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<ModelVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &MODEL_ATTRIBUTES,
                })],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: particle_depth,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("particle_fragment"),
                compilation_options: Default::default(),
                targets: &[particle_target],
            }),
            multiview_mask: None,
            cache: None,
        });
        let ocean_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Preview ocean"),
            source: wgpu::ShaderSource::Wgsl(OCEAN_SHADER.into()),
        });
        let ocean_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Ocean camera layout"),
            bind_group_layouts: &[Some(&camera_layout)],
            immediate_size: 0,
        });
        let ocean = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Preview ocean pipeline"),
            layout: Some(&ocean_layout),
            vertex: wgpu::VertexState {
                module: &ocean_shader,
                entry_point: Some("vertex_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: depth,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &ocean_shader,
                entry_point: Some("fragment_main"),
                compilation_options: Default::default(),
                targets: &[target],
            }),
            multiview_mask: None,
            cache: None,
        });
        let mut meshes = Vec::new();
        for source in &asset.meshes {
            let vertices = bytemuck::cast_slice(&source.vertices);
            let indices = bytemuck::cast_slice(&source.indices);
            ensure!(
                vertices.len() as u64 <= device.limits().max_buffer_size
                    && indices.len() as u64 <= device.limits().max_buffer_size,
                "model buffer exceeds GPU limit"
            );
            ensure!(
                source.indices.len() <= u32::MAX as usize && source.texture < textures.len(),
                "invalid model draw"
            );
            meshes.push(DrawMesh {
                damage_mask: 0,
                normal_count: source.indices.len() as u32,
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("GM vertices"),
                    contents: vertices,
                    usage: wgpu::BufferUsages::VERTEX
                        | if source.sail.is_some()
                            || source.rigging.is_some()
                            || source.dynamic.is_some()
                        {
                            wgpu::BufferUsages::COPY_DST
                        } else {
                            wgpu::BufferUsages::empty()
                        },
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("GM triangles"),
                    contents: indices,
                    usage: wgpu::BufferUsages::INDEX
                        | if source.sail.is_some() {
                            wgpu::BufferUsages::COPY_DST
                        } else {
                            wgpu::BufferUsages::empty()
                        },
                }),
                count: source.indices.len() as u32,
                texture: source.texture,
                sail: source.sail.clone().map(|s| (s, source.vertices.clone())),
                rigging: source.rigging.clone().map(|r| (r, source.vertices.clone())),
                dynamic: source.dynamic.clone().map(|d| (d, source.vertices.clone())),
                ship_node: source.ship_node.clone(),
            });
        }
        Ok(Self {
            sail_pipelines,
            particle_pipeline,
            ship: asset.ship.clone(),
            ship_state: asset.ship_state.clone(),
            pose_stride,
            pose_bytes,
            pipeline,
            ocean,
            camera_buffer,
            pose_buffer,
            pose: ModelPose::default(),
            waves: storm_sim::Waves::default(),
            rig: storm_sim::RigState::uniform(1.0),
            wave_buffer,
            camera_group,
            textures,
            meshes,
            depth: depth_view(device, 1, 1),
            size: (1, 1),
        })
    }
    pub fn set_pose(&mut self, pose: ModelPose) -> Result<()> {
        pose.matrix()?;
        self.pose = pose;
        Ok(())
    }
    pub fn set_ship_state(&mut self, state: ShipState) -> Result<()> {
        state.validate(&self.ship)?;
        sail_damage::validate_holes(
            &state,
            self.meshes
                .iter()
                .enumerate()
                .filter_map(|(i, m)| m.sail.as_ref().map(|(p, _)| (i, p))),
        )?;
        self.ship_state = state;
        Ok(())
    }
    pub fn set_waves(&mut self, waves: storm_sim::Waves) -> Result<()> {
        waves.validate()?;
        self.waves = waves;
        Ok(())
    }
    pub fn set_flags(&mut self, selection: storm_sim::FlagSelection) -> Result<()> {
        rigging::select_flags(
            self.meshes
                .iter_mut()
                .filter_map(|m| m.rigging.as_mut().map(|(r, _)| r)),
            selection,
        )
    }
    pub fn set_rig_state(&mut self, rig: storm_sim::RigState) -> Result<()> {
        rig.validate()?;
        self.rig = rig;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        width: u32,
        height: u32,
        seconds: f32,
        camera: &OrbitCamera,
        wind: SailWind,
    ) -> Result<()> {
        let uniform = camera.uniform(width, height, seconds)?;
        wind.validate()?;
        // Cloth currently accepts horizontal wind only: project the inverse rigid
        // rotation into local XZ, including pitch/roll, rather than subtracting yaw.
        let direction = Vec3::new(wind.direction.sin(), 0.0, wind.direction.cos());
        let local = self.pose.matrix()?.inverse().transform_vector3(direction) * wind.speed;
        let local_wind = SailWind {
            direction: local.x.atan2(local.z),
            speed: local.x.hypot(local.z).min(40.0),
        };
        let eye = self.pose.matrix()?.inverse().transform_point3(Vec3::new(
            uniform.eye_time[0],
            uniform.eye_time[1],
            uniform.eye_time[2],
        ));
        for mesh in &mut self.meshes {
            if let Some((dynamic, vertices)) = &mut mesh.dynamic {
                dynamic.deform(
                    vertices,
                    seconds,
                    local_wind,
                    eye,
                    &self.ship,
                    &self.ship_state,
                )?;
                gpu.queue
                    .write_buffer(&mesh.vertices, 0, bytemuck::cast_slice(vertices));
            }
        }
        for (index, mesh) in self.meshes.iter_mut().enumerate() {
            if let Some((patch, vertices)) = &mut mesh.sail {
                let mask = self.ship_state.sail_holes.get(&index).copied().unwrap_or(0);
                if mask != mesh.damage_mask {
                    let topology = patch.original_indices(mask);
                    mesh.normal_count = topology.normal_count;
                    mesh.damage_mask = mask;
                    gpu.queue.write_buffer(
                        &mesh.indices,
                        0,
                        bytemuck::cast_slice(&topology.indices),
                    );
                }
                patch.deform_with_damage(
                    vertices,
                    local_wind,
                    seconds,
                    self.rig.deployed(patch.tier) as f32,
                    mask,
                )?;
                gpu.queue
                    .write_buffer(&mesh.vertices, 0, bytemuck::cast_slice(vertices));
            }
        }
        if self.size != (width, height) {
            self.depth = depth_view(&gpu.device, width, height);
            self.size = (width, height);
        }
        for mesh in &mut self.meshes {
            if let Some((rigging, vertices)) = &mut mesh.rigging {
                rigging.deform(vertices, local_wind, seconds, self.rig)?;
                rigging.follow_damage(vertices, &self.ship, &self.ship_state, seconds);
                gpu.queue
                    .write_buffer(&mesh.vertices, 0, bytemuck::cast_slice(vertices));
            }
        }
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
        gpu.queue.write_buffer(
            &self.wave_buffer,
            0,
            bytemuck::bytes_of(&WaveUniform::from(self.waves)),
        );
        let base = self.pose.matrix()?;
        self.pose_bytes[..OBJECT_BYTES]
            .copy_from_slice(bytemuck::bytes_of(&ObjectUniform::new(base, 0, false)));
        for (i, mesh) in self.meshes.iter().enumerate() {
            let transform = mesh
                .ship_node
                .as_deref()
                .and_then(|n| self.ship.transform(n, &self.ship_state, seconds))
                .unwrap_or_default();
            let offset = (i + 1) * self.pose_stride as usize;
            self.pose_bytes[offset..offset + OBJECT_BYTES].copy_from_slice(bytemuck::bytes_of(
                &ObjectUniform::new(
                    base * transform,
                    self.ship_state.sail_holes.get(&i).copied().unwrap_or(0),
                    mesh.sail.as_ref().is_some_and(|(p, _)| p.is_triangle()),
                ),
            ));
        }
        gpu.queue
            .write_buffer(&self.pose_buffer, 0, &self.pose_bytes);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Original model and water"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                view: &self.depth,
                depth_ops: Some(wgpu::Operations {
                    load: wgpu::LoadOp::Clear(1.0),
                    store: wgpu::StoreOp::Store,
                }),
                stencil_ops: None,
            }),
            ..Default::default()
        });
        pass.set_bind_group(0, &self.camera_group, &[0]);
        pass.set_pipeline(&self.ocean);
        pass.draw(0..3, 0..1);
        for particles in [false, true] {
            pass.set_pipeline(if particles {
                &self.particle_pipeline
            } else {
                &self.pipeline
            });
            for (i, mesh) in self.meshes.iter().enumerate() {
                if mesh.dynamic.as_ref().is_some_and(|(d, _)| d.particle()) != particles {
                    continue;
                }
                if mesh
                    .dynamic
                    .as_ref()
                    .is_some_and(|(d, _)| !d.visible(&self.ship_state, seconds))
                {
                    continue;
                }
                if mesh
                    .ship_node
                    .as_deref()
                    .is_some_and(|n| self.ship.hidden(n, &self.ship_state, seconds))
                {
                    continue;
                }
                if mesh
                    .rigging
                    .as_ref()
                    .is_some_and(|(rig, _)| matches!(rig,RigMesh::Flag(f) if !f.is_visible()))
                {
                    continue;
                }
                pass.set_bind_group(1, &self.textures[mesh.texture], &[]);
                pass.set_bind_group(
                    0,
                    &self.camera_group,
                    &[((i + 1) as u32) * self.pose_stride],
                );
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                if mesh.sail.is_some() {
                    // SAIL::Realize draws the damaged region with mirror U first.
                    if mesh.normal_count < mesh.count {
                        pass.set_pipeline(&self.sail_pipelines[1]);
                        pass.draw_indexed(mesh.normal_count..mesh.count, 0, 0..1);
                    }
                    if mesh.normal_count > 0 {
                        pass.set_pipeline(&self.sail_pipelines[0]);
                        pass.draw_indexed(0..mesh.normal_count, 0, 0..1);
                    }
                } else {
                    pass.set_pipeline(if particles {
                        &self.particle_pipeline
                    } else {
                        &self.pipeline
                    });
                    pass.draw_indexed(0..mesh.count, 0, 0..1);
                }
            }
        }
        Ok(())
    }
}
