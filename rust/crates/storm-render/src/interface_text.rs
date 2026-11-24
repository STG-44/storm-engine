//! Original bitmap-font quads shared by the configured window and PNG capture.
use crate::{Gpu, model::upload_texture};
use anyhow::{Result, ensure};
use std::{
    collections::BTreeMap,
    ops::Range,
    rc::{Rc, Weak},
};
use storm_formats::texture::Texture;

const MAX_GLYPHS: usize = 512;
const MAX_INSTANCES: usize = 65_536;

pub struct BitmapFont {
    pub texture: Texture,
    pub sheet_size: [u32; 2],
    pub glyphs: BTreeMap<char, [u32; 4]>,
    pub height: u32,
    pub pc_scale: f32,
    pub interval: i32,
    pub space: i32,
    pub shadow: [i32; 2],
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Alignment {
    Left,
    Right,
    Center,
}

#[derive(Clone)]
pub struct TextDraw {
    pub font: Rc<BitmapFont>,
    pub text: String,
    pub canvas: [i32; 2],
    pub position: [i32; 2],
    pub scale: f32,
    pub alignment: Alignment,
    pub rgba: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    uv: [f32; 4],
    rgba: [u8; 4],
}

impl TextDraw {
    fn layout(&self, size: [u32; 2], out: &mut Vec<Instance>) -> Result<()> {
        if self.text.is_empty() || self.scale == 0.0 || self.rgba[3] == 0 {
            return Ok(());
        }
        ensure!(
            self.canvas.into_iter().all(|n| n > 0)
                && size.into_iter().all(|n| n > 0)
                && self.scale.is_finite()
                && self.scale > 0.0,
            "invalid interface text viewport or scale"
        );
        let font = &self.font;
        ensure!(
            font.sheet_size.into_iter().all(|n| n > 0) && (0.01..=16.0).contains(&font.pc_scale),
            "invalid interface font metrics"
        );
        let [ratio_x, ratio_y] = std::array::from_fn(|i| size[i] as f32 / self.canvas[i] as f32);
        let scale = self.scale * ratio_x;
        let y = (self.position[1] as f32 * ratio_y).trunc();
        let glyph_width = |glyph: &[u32; 4]| (glyph[2] as f32 * font.pc_scale).trunc();
        let advance = |c: char| {
            font.glyphs.get(&c).map_or(0.0, glyph_width)
                + font.interval as f32
                + if c == ' ' {
                    (font.space as f32 * font.pc_scale).trunc()
                } else {
                    0.0
                }
        };
        let symbols = self
            .text
            .chars()
            .take(MAX_GLYPHS)
            .filter(|&c| (c as u32) < 0x2070);
        let width = symbols.clone().map(advance).sum::<f32>() as i32;
        let offset = match self.alignment {
            Alignment::Left => 0,
            Alignment::Right => width,
            Alignment::Center => width.saturating_add(1) / 2,
        };
        let x = (self.position[0] as f32 * ratio_x).trunc() - (offset as f32 * scale).trunc();
        let [atlas_x, atlas_y] = font.sheet_size.map(|n| n as f32);
        let height = (font.height as f32 * font.pc_scale).trunc();
        let clip_x = |v: f32| v * 2.0 / size[0] as f32 - 1.0;
        let clip_y = |v: f32| 1.0 - v * 2.0 / size[1] as f32;
        let draw = |out: &mut Vec<Instance>, origin: [f32; 2], rgba| {
            let mut offset = 0.0;
            for c in symbols.clone() {
                if c != ' '
                    && let Some(&[u, v, w, h]) = font.glyphs.get(&c)
                    && w > 0
                    && h > 0
                {
                    let left = origin[0] + offset;
                    let top = origin[1] + (height - (h as f32 * font.pc_scale).trunc()) * scale;
                    out.push(Instance {
                        rect: [
                            clip_x(left),
                            clip_y(top),
                            clip_x(left + glyph_width(&[u, v, w, h]) * scale),
                            clip_y(origin[1] + height * scale),
                        ],
                        uv: [
                            (u as f32 + 0.5) / atlas_x,
                            (v as f32 + 0.5) / atlas_y,
                            (u as f32 + w as f32 - 0.5) / atlas_x,
                            (v as f32 + h as f32 - 0.5) / atlas_y,
                        ],
                        rgba,
                    });
                }
                offset += advance(c) * scale;
            }
        };
        // XINTERFACE ExtPrint always requests the bitmap shadow.
        draw(
            out,
            [x + font.shadow[0] as f32, y + font.shadow[1] as f32],
            [0, 0, 0, self.rgba[3]],
        );
        draw(out, [x, y], self.rgba);
        ensure!(
            out.len() <= MAX_INSTANCES,
            "interface text frame glyph budget"
        );
        Ok(())
    }
}

pub struct TextRenderer {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    atlases: BTreeMap<usize, (Weak<BitmapFont>, wgpu::BindGroup)>,
    instances: Option<wgpu::Buffer>,
}

impl TextRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("interface_text.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Interface bitmap text"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Float32x4, 2 => Unorm8x4
                    ],
                })],
                compilation_options: Default::default(),
            },
            primitive: Default::default(),
            depth_stencil: None,
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
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Self {
            pipeline,
            sampler,
            atlases: BTreeMap::new(),
            instances: None,
        }
    }

    pub fn encode(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        draws: &[TextDraw],
    ) -> Result<()> {
        self.atlases.retain(|_, (font, _)| font.strong_count() != 0);
        let mut instances = Vec::new();
        let mut batches: Vec<(usize, Range<u32>)> = Vec::new();
        for draw in draws {
            let first = instances.len() as u32;
            draw.layout(size, &mut instances)?;
            let end = instances.len() as u32;
            if first == end {
                continue;
            }
            let key = Rc::as_ptr(&draw.font) as usize;
            if let std::collections::btree_map::Entry::Vacant(entry) = self.atlases.entry(key) {
                let atlas = upload_texture(gpu, &draw.font.texture)?;
                let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Interface font atlas"),
                    layout: &self.pipeline.get_bind_group_layout(0),
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&atlas),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
                entry.insert((Rc::downgrade(&draw.font), bind));
            }
            batches.push((key, first..end));
        }
        if batches.is_empty() {
            return Ok(());
        }
        let bytes: &[u8] = bytemuck::cast_slice(&instances);
        let capacity = (bytes.len() as u64).next_power_of_two();
        ensure!(
            capacity <= gpu.device.limits().max_buffer_size,
            "interface text GPU buffer limit"
        );
        let buffer = match &self.instances {
            Some(buffer) if buffer.size() >= capacity => buffer,
            _ => self
                .instances
                .insert(gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Interface glyphs"),
                    size: capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })),
        };
        gpu.queue.write_buffer(buffer, 0, bytes);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Interface strings"),
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
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, buffer.slice(..));
        for (key, range) in batches {
            pass.set_bind_group(0, &self.atlases[&key].1, &[]);
            pass.draw(0..6, range);
        }
        Ok(())
    }
}
