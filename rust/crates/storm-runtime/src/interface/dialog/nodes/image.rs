use super::{FULL_UV, NEUTRAL, Nodes, Shared, Texture, ini_color, solid_mask, tuple};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_render::interface_text::{InterfaceDraw, PictureColor, PictureDraw};

pub(super) struct InterfaceImage {
    texture: Rc<Texture>,
    uv: ImageUv,
    rgba: [u8; 4],
    color: PictureColor,
    width: i32,
}

enum ImageUv {
    Fixed([f32; 4]),
    Crop {
        origin: [f32; 2],
        atlas_size: [i32; 2],
    },
}

impl InterfaceImage {
    pub(super) fn load(nodes: &mut Nodes, shared: &Shared, value: &str) -> Result<Option<Self>> {
        let mut group = None;
        let mut name = None;
        let mut file = None;
        let mut color = None;
        let mut origin = [0.0_f32; 2];
        let mut uv = FULL_UV;
        let mut width = 32;
        let mut depth = 0i32;
        for field in value.split(|c| match c {
            '(' | '[' | '{' => {
                depth += 1;
                false
            }
            ')' | ']' | '}' => {
                depth -= 1;
                false
            }
            ',' => depth == 0,
            _ => false,
        }) {
            let Some((key, param)) = field.split_once('=') else {
                continue;
            };
            let param = param.trim_ascii();
            let list = param.trim_matches(['(', ')', '[', ']', '{', '}']);
            match key.trim_ascii().to_ascii_lowercase().as_str() {
                "piclist" => group = Some(param),
                "picname" => name = Some(param),
                "file" => file = Some(param),
                "color" => color = Some(ini_color(Some(list), NEUTRAL)?),
                "piccutuv" => origin = tuple(list)?,
                "rectuv" => uv = tuple(list)?,
                "size" => {
                    let size: [i32; 2] = tuple(list)?;
                    width = size[0].max(0);
                }
                _ => {}
            }
        }
        ensure!(
            origin.iter().chain(&uv).all(|value| value.is_finite()),
            "invalid interface image UV"
        );
        let (texture, uv) = if let Some(group) = group {
            let atlas = nodes.atlas(shared, group)?;
            let uv = match atlas.named_part(name) {
                Some((rect, _)) => ImageUv::Fixed(rect),
                None => ImageUv::Crop {
                    origin,
                    atlas_size: atlas.size,
                },
            };
            (atlas.texture, uv)
        } else if let Some(rgba) = color {
            return Ok(Some(Self {
                texture: solid_mask(),
                uv: ImageUv::Fixed(FULL_UV),
                rgba,
                color: PictureColor::AlphaMask,
                width,
            }));
        } else if let Some(file) = file {
            (nodes.texture(shared, file)?, ImageUv::Fixed(uv))
        } else {
            return Ok(None);
        };
        Ok(texture.map(|texture| Self {
            texture,
            uv,
            rgba: color.unwrap_or(NEUTRAL),
            color: PictureColor::Modulate,
            width,
        }))
    }

    pub(super) fn texture_width(&self) -> Option<i32> {
        matches!(self.color, PictureColor::Modulate).then_some(self.width)
    }

    pub(super) fn draw(&self, rect: [i32; 4], canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        let uv = match self.uv {
            ImageUv::Fixed(uv) => uv,
            ImageUv::Crop {
                origin: [u, v],
                atlas_size: [w, h],
            } => [
                u,
                v,
                (u + rect[2].saturating_sub(rect[0]) as f32 / w as f32).min(1.0),
                (v + rect[3].saturating_sub(rect[1]) as f32 / h as f32).min(1.0),
            ],
        };
        out.push(InterfaceDraw::Picture(PictureDraw {
            texture: self.texture.clone(),
            canvas,
            rect: rect.map(|value| value as f32),
            uv,
            rgba: self.rgba,
            color: self.color,
        }));
    }
}
