use super::{Gpu, OrbitCamera, opaque_depth, upload_texture};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use std::{
    collections::BTreeMap,
    ops::Range,
    rc::{Rc, Weak},
};
use storm_formats::texture::Texture;
use storm_sim::{
    coast_foam::{MAX_DRAW_VERTICES, Strip},
    sea::Surface,
    ship_rocking::SeaSurface,
};

#[derive(Clone)]
pub struct CoastFoamDraw {
    pub texture: Rc<Texture>,
    pub strip: Strip,
    pub height: f32,
    pub max_distance: f32,
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    uv: [f32; 2],
    alpha: f32,
}

struct Batch {
    texture: usize,
    indices: Range<u32>,
    vertex: i32,
}

pub struct CoastFoamRenderer {
    pipeline: wgpu::RenderPipeline,
    camera: wgpu::Buffer,
    view: wgpu::BindGroup,
    sampler: wgpu::Sampler,
    textures: BTreeMap<usize, (Weak<Texture>, wgpu::BindGroup)>,
    vertices: Option<wgpu::Buffer>,
    indices: Option<wgpu::Buffer>,
    batches: Vec<Batch>,
}

impl CoastFoamRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("../coast_foam.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Coastal foam"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<GpuVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2, 2 => Float32],
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
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let camera = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Coastal foam view projection"),
            size: 64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let view = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Coastal foam view"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera.as_entire_binding(),
            }],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            camera,
            view,
            sampler,
            textures: BTreeMap::new(),
            vertices: None,
            indices: None,
            batches: Vec::new(),
        }
    }

    pub(super) fn prepare(
        &mut self,
        gpu: &Gpu,
        size: [u32; 2],
        seconds: f32,
        camera: &OrbitCamera,
        draws: &[CoastFoamDraw],
        mut sea: Surface,
    ) -> Result<()> {
        let view = camera.uniform(size[0], size[1], seconds)?;
        let eye = Vec3::from_slice(&view.eye_time[..3]);
        let native_projection = Mat4::from_cols_array_2d(&view.view_projection)
            * Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0));
        self.batches.clear();
        self.textures
            .retain(|_, (source, _)| source.strong_count() != 0);
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for draw in draws {
            if !visible(draw, eye, native_projection) {
                continue;
            }
            let layers = draw.strip.layers(draw.height, |x, z| sea.wave_xz(x, z))?;
            if layers.is_empty() {
                continue;
            }
            let first = indices.len() as u32;
            indices.extend(draw.strip.layout().indices.iter().copied());
            let indices = first..indices.len() as u32;
            let key = Rc::as_ptr(&draw.texture) as usize;
            if let std::collections::btree_map::Entry::Vacant(entry) = self.textures.entry(key) {
                let texture = upload_texture(gpu, &draw.texture)?;
                let group = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Coastal foam material"),
                    layout: &self.pipeline.get_bind_group_layout(1),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&texture),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                entry.insert((Rc::downgrade(&draw.texture), group));
            }
            for layer in layers {
                ensure!(
                    layer.len() <= MAX_DRAW_VERTICES - vertices.len(),
                    "coastal foam frame vertex budget"
                );
                self.batches.push(Batch {
                    texture: key,
                    indices: indices.clone(),
                    vertex: vertices.len() as i32,
                });
                vertices.extend(layer.iter().map(|vertex| GpuVertex {
                    position: vertex.position,
                    uv: vertex.uv,
                    alpha: f32::from(vertex.alpha) / 255.0,
                }));
            }
        }
        gpu.queue.write_buffer(
            &self.camera,
            0,
            bytemuck::bytes_of(&native_projection.to_cols_array_2d()),
        );
        upload(
            gpu,
            &mut self.vertices,
            wgpu::BufferUsages::VERTEX,
            bytemuck::cast_slice(&vertices),
        )?;
        upload(
            gpu,
            &mut self.indices,
            wgpu::BufferUsages::INDEX,
            bytemuck::cast_slice(&indices),
        )?;
        Ok(())
    }

    pub(super) fn encode(&self, pass: &mut wgpu::RenderPass<'_>) {
        if let (Some(vertices), Some(indices)) = (&self.vertices, &self.indices) {
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.view, &[]);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint16);
            for batch in &self.batches {
                pass.set_bind_group(1, &self.textures[&batch.texture].1, &[]);
                pass.draw_indexed(batch.indices.clone(), batch.vertex, 0..1);
            }
        }
    }
}

fn visible(draw: &CoastFoamDraw, eye: Vec3, projection: Mat4) -> bool {
    let layout = draw.strip.layout();
    let [x, z] = layout.origin;
    if eye.distance(Vec3::new(x, 0.0, -z)) > draw.max_distance {
        return false;
    }
    let mut outside = [true; 4];
    for &[x, z] in layout.boundary.iter() {
        let p = projection * Vec3::new(x, 0.0, z).extend(1.0);
        let planes = [p.w + p.x, p.w - p.x, p.w + p.y, p.w - p.y];
        for (outside, distance) in outside.iter_mut().zip(planes) {
            *outside &= distance < 0.0;
        }
    }
    !outside.into_iter().any(|plane| plane)
}

fn upload(
    gpu: &Gpu,
    slot: &mut Option<wgpu::Buffer>,
    usage: wgpu::BufferUsages,
    bytes: &[u8],
) -> Result<()> {
    if bytes.is_empty() {
        return Ok(());
    }
    let size = (bytes.len() as u64).next_power_of_two();
    ensure!(
        size <= gpu.device.limits().max_buffer_size,
        "coastal foam GPU buffer limit"
    );
    if slot.as_ref().is_none_or(|buffer| buffer.size() < size) {
        *slot = Some(gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Coastal foam geometry"),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        }));
    }
    if let Some(buffer) = slot {
        gpu.queue.write_buffer(buffer, 0, bytes);
    }
    Ok(())
}
