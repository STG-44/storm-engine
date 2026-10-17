//! Native static model preview: original mesh hierarchy/textures and diagnostic water.
mod assets;
use crate::Gpu;
use anyhow::{Result, ensure};
pub use assets::{LoadOptions, ModelAsset, read_asset_bytes};
use glam::Vec3;
use wgpu::util::DeviceExt;

pub const MODEL_SHADER: &str = include_str!("model.wgsl");
pub const OCEAN_SHADER: &str = include_str!("ocean.wgsl");

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
struct DrawMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    count: u32,
    texture: usize,
}

pub struct ModelScene {
    pipeline: wgpu::RenderPipeline,
    ocean: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
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

impl ModelScene {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat, asset: &ModelAsset) -> Result<Self> {
        let device = &gpu.device;
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Model camera layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
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
            ],
        });
        let camera_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Orbit camera"),
            size: std::mem::size_of::<CameraUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let camera_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Model camera"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
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
        for source in &asset.textures {
            let header = source.header;
            ensure!(
                header.width <= device.limits().max_texture_dimension_2d
                    && header.height <= device.limits().max_texture_dimension_2d,
                "model texture exceeds GPU dimensions"
            );
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Decoded Storm TX"),
                size: wgpu::Extent3d {
                    width: header.width,
                    height: header.height,
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
            textures.push(device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("Model material"),
                layout: &texture_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            &texture.create_view(&Default::default()),
                        ),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
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
                vertices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("GM vertices"),
                    contents: vertices,
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                indices: device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("GM triangles"),
                    contents: indices,
                    usage: wgpu::BufferUsages::INDEX,
                }),
                count: source.indices.len() as u32,
                texture: source.texture,
            });
        }
        Ok(Self {
            pipeline,
            ocean,
            camera_buffer,
            camera_group,
            textures,
            meshes,
            depth: depth_view(device, 1, 1),
            size: (1, 1),
        })
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
    ) -> Result<()> {
        let uniform = camera.uniform(width, height, seconds)?;
        if self.size != (width, height) {
            self.depth = depth_view(&gpu.device, width, height);
            self.size = (width, height);
        }
        gpu.queue
            .write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&uniform));
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
        pass.set_bind_group(0, &self.camera_group, &[]);
        pass.set_pipeline(&self.ocean);
        pass.draw(0..3, 0..1);
        pass.set_pipeline(&self.pipeline);
        for mesh in &self.meshes {
            pass.set_bind_group(1, &self.textures[mesh.texture], &[]);
            pass.set_vertex_buffer(0, mesh.vertices.slice(..));
            pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..mesh.count, 0, 0..1);
        }
        Ok(())
    }
}
