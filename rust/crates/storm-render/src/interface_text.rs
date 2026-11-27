//! Textured interface quads shared by the configured window and PNG capture.
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
    pub texture: Rc<Texture>,
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
    /// Align this height in the font's glyph coordinates to the screen-space Y anchor.
    pub vertical_anchor: Option<f32>,
    pub scale: f32,
    pub alignment: Alignment,
    pub shadow: bool,
    pub rgba: [u8; 4],
}

#[derive(Clone, Copy)]
pub enum PictureColor {
    Modulate,
    AlphaMask,
}

#[derive(Clone)]
pub struct PictureDraw {
    pub texture: Rc<Texture>,
    pub canvas: [i32; 2],
    /// Left, top, right, bottom in the interface coordinate system.
    pub rect: [f32; 4],
    pub uv: [f32; 4],
    pub rgba: [u8; 4],
    pub color: PictureColor,
}

#[derive(Clone)]
pub enum InterfaceDraw {
    Picture(PictureDraw),
    Text(TextDraw),
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Instance {
    rect: [f32; 4],
    uv: [f32; 4],
    rgba: [u8; 4],
    color_mode: u32,
}

impl PictureDraw {
    fn layout(&self, out: &mut Vec<Instance>) -> Result<()> {
        let [width, height] = self.canvas.map(|value| value as f32);
        ensure!(
            width > 0.0
                && height > 0.0
                && self
                    .rect
                    .iter()
                    .chain(&self.uv)
                    .all(|value| value.is_finite()),
            "invalid interface picture geometry"
        );
        let [left, top, right, bottom] = self.rect;
        if self.rgba[3] != 0 && left != right && top != bottom {
            out.push(Instance {
                rect: [
                    left * 2.0 / width - 1.0,
                    1.0 - top * 2.0 / height,
                    right * 2.0 / width - 1.0,
                    1.0 - bottom * 2.0 / height,
                ],
                uv: self.uv,
                rgba: self.rgba,
                color_mode: match self.color {
                    PictureColor::Modulate => 1,
                    PictureColor::AlphaMask => 2,
                },
            });
        }
        Ok(())
    }
}

impl BitmapFont {
    pub fn glyph_advance(&self, c: char) -> f32 {
        if c as u32 >= 0x2070 {
            return 0.0;
        }
        self.glyphs
            .get(&c)
            .map_or(0.0, |glyph| (glyph[2] as f32 * self.pc_scale).trunc())
            + self.interval as f32
            + if c == ' ' {
                (self.space as f32 * self.pc_scale).trunc()
            } else {
                0.0
            }
    }

    /// Matches the native width used to position title plates and glyphs.
    pub fn text_width(&self, text: &str) -> i32 {
        symbols(text).map(|c| self.glyph_advance(c)).sum::<f32>() as i32
    }

    /// Coverage-weighted vertical center of the visible glyphs.
    pub fn ink_center_y(&self, text: &str) -> f32 {
        let line_height = (self.height as f32 * self.pc_scale).trunc();
        let Some(mip) = self.texture.mips.first() else {
            return line_height * 0.5;
        };
        if [mip.width, mip.height] != self.sheet_size {
            return line_height * 0.5;
        }
        let mut coverage = 0.0;
        let mut weighted_y = 0.0;
        for [x, y, w, h] in symbols(text)
            .filter(|&c| c != ' ')
            .filter_map(|c| self.glyphs.get(&c).copied())
        {
            if w == 0
                || h == 0
                || x.saturating_add(w) > mip.width
                || y.saturating_add(h) > mip.height
            {
                continue;
            }
            let glyph_width = (w as f32 * self.pc_scale).trunc();
            let glyph_height = (h as f32 * self.pc_scale).trunc();
            let glyph_top = line_height - glyph_height;
            let row_height = glyph_height / h as f32;
            let texel_area = row_height * glyph_width / w as f32;
            for row in 0..h {
                let start = ((y + row) as usize * mip.width as usize + x as usize) * 4;
                let Some(pixels) = mip.rgba.get(start..start + w as usize * 4) else {
                    break;
                };
                let alpha: u32 = pixels
                    .chunks_exact(4)
                    .map(|pixel| u32::from(pixel[3]))
                    .sum();
                let weight = alpha as f32 * texel_area;
                coverage += weight;
                weighted_y += weight * (glyph_top + (row as f32 + 0.5) * row_height);
            }
        }

        if coverage > 0.0 {
            weighted_y / coverage
        } else {
            line_height * 0.5
        }
    }
}

fn symbols(text: &str) -> impl Iterator<Item = char> + Clone + '_ {
    text.chars()
        .take(MAX_GLYPHS)
        .filter(|&c| (c as u32) < 0x2070)
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
                && self.scale > 0.0
                && self.vertical_anchor.is_none_or(f32::is_finite),
            "invalid interface text viewport or scale"
        );
        let font = &self.font;
        ensure!(
            font.sheet_size.into_iter().all(|n| n > 0) && (0.01..=16.0).contains(&font.pc_scale),
            "invalid interface font metrics"
        );
        let [ratio_x, ratio_y] = std::array::from_fn(|i| size[i] as f32 / self.canvas[i] as f32);
        let scale = self.scale * ratio_x;
        let y = (self.position[1] as f32 * ratio_y).trunc()
            - self.vertical_anchor.unwrap_or(0.0) * scale;
        let glyph_width = |glyph: &[u32; 4]| (glyph[2] as f32 * font.pc_scale).trunc();
        let symbols = symbols(&self.text);
        let width = font.text_width(&self.text);
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
                        color_mode: 0,
                    });
                }
                offset += font.glyph_advance(c) * scale;
            }
        };
        if self.shadow {
            draw(
                out,
                [x + font.shadow[0] as f32, y + font.shadow[1] as f32],
                [0, 0, 0, self.rgba[3]],
            );
        }
        draw(out, [x, y], self.rgba);
        Ok(())
    }
}

pub struct InterfaceRenderer {
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    textures: BTreeMap<usize, (Weak<Texture>, wgpu::BindGroup)>,
    instances: Option<wgpu::Buffer>,
}

impl InterfaceRenderer {
    pub fn new(gpu: &Gpu, format: wgpu::TextureFormat) -> Self {
        let device = &gpu.device;
        let shader = device.create_shader_module(wgpu::include_wgsl!("interface_text.wgsl"));
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Interface images and text"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: size_of::<Instance>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![
                        0 => Float32x4, 1 => Float32x4, 2 => Unorm8x4, 3 => Uint32
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
            textures: BTreeMap::new(),
            instances: None,
        }
    }

    pub fn encode(
        &mut self,
        gpu: &Gpu,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        draws: &[InterfaceDraw],
    ) -> Result<()> {
        self.textures
            .retain(|_, (texture, _)| texture.strong_count() != 0);
        let mut instances = Vec::new();
        let mut batches: Vec<(usize, Range<u32>)> = Vec::new();
        for draw in draws {
            let first = instances.len() as u32;
            let texture = match draw {
                InterfaceDraw::Picture(picture) => {
                    picture.layout(&mut instances)?;
                    &picture.texture
                }
                InterfaceDraw::Text(text) => {
                    text.layout(size, &mut instances)?;
                    &text.font.texture
                }
            };
            ensure!(
                instances.len() <= MAX_INSTANCES,
                "interface frame quad budget"
            );
            let end = instances.len() as u32;
            if first == end {
                continue;
            }
            let key = Rc::as_ptr(texture) as usize;
            if let std::collections::btree_map::Entry::Vacant(entry) = self.textures.entry(key) {
                let atlas = upload_texture(gpu, texture)?;
                let bind = gpu.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Interface texture"),
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
                entry.insert((Rc::downgrade(texture), bind));
            }
            if let Some((last_key, range)) = batches.last_mut()
                && *last_key == key
            {
                range.end = end;
            } else {
                batches.push((key, first..end));
            }
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
                    label: Some("Interface quads"),
                    size: capacity,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })),
        };
        gpu.queue.write_buffer(buffer, 0, bytes);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Interface overlay"),
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
            pass.set_bind_group(0, &self.textures[&key].1, &[]);
            pass.draw(0..6, range);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use storm_formats::{TxHeader, texture::Mip};

    #[test]
    fn text_ink_stays_centered_on_a_stretched_interface() -> Result<()> {
        let mut rgba = vec![0; 2 * 8 * 4];
        rgba[2 * 4 * 4..].fill(255);
        let texture = Rc::new(Texture {
            header: TxHeader {
                flags: 0,
                width: 2,
                height: 8,
                mips: 1,
                format: 21,
                mip0_bytes: 64,
            },
            mips: vec![Mip {
                width: 2,
                height: 8,
                rgba,
            }],
        });
        let font = Rc::new(BitmapFont {
            texture,
            sheet_size: [2, 8],
            glyphs: BTreeMap::from([('A', [0, 0, 2, 8])]),
            height: 8,
            pc_scale: 1.0,
            interval: 0,
            space: 0,
            shadow: [0; 2],
        });
        let draw = TextDraw {
            vertical_anchor: Some(font.ink_center_y("A")),
            font,
            text: "A".to_owned(),
            canvas: [800, 600],
            position: [100, 50],
            scale: 1.0,
            alignment: Alignment::Center,
            shadow: false,
            rgba: [255; 4],
        };
        let mut instances = Vec::new();
        draw.layout([1280, 720], &mut instances)?;
        let [_, top, _, bottom] = instances[0].rect;
        let ink_center = top + (bottom - top) * 0.75;
        assert!((ink_center - (1.0 - 60.0 * 2.0 / 720.0)).abs() < 1e-6);
        Ok(())
    }
}
