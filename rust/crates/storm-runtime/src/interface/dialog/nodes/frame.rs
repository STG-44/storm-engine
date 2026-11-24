use super::{
    Atlas, FULL_UV, NEUTRAL, Nodes, Setting, Shared, Texture, ini_color, ini_integer, tuple,
};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_formats::{TxHeader, texture::Mip};
use storm_render::interface_text::{InterfaceDraw, PictureColor, PictureDraw};

pub(super) struct Frame {
    background: Option<FrameImage>,
    caption: Option<Caption>,
    border_texture: Option<Rc<Texture>>,
    border_color: [u8; 4],
    corner_sizes: [[i32; 2]; 4],
    edge_sizes: [i32; 4],
    border_uv: [[f32; 4]; 8],
}

struct Caption {
    image: Option<FrameImage>,
    offset: [i32; 4],
    height: i32,
    divider: i32,
}

struct FrameImage {
    texture: Rc<Texture>,
    uv: ImageUv,
    rgba: [u8; 4],
    color: PictureColor,
}

enum ImageUv {
    Fixed([f32; 4]),
    Crop {
        origin: [f32; 2],
        atlas_size: [i32; 2],
    },
}

impl Frame {
    pub(super) fn load(nodes: &mut Nodes, shared: &Shared, setting: &Setting<'_>) -> Result<Self> {
        let background = match setting("backimage") {
            Some(value) => FrameImage::load(nodes, shared, value)?,
            None => None,
        };
        let caption_height = setting("captionheight").map_or(Ok(0), ini_integer)?;
        let caption = match setting("captionimage").filter(|_| caption_height > 0) {
            Some(value) => Some(Caption {
                image: FrameImage::load(nodes, shared, value)?,
                offset: setting("captionoffset").map_or(Ok([0; 4]), tuple)?,
                height: caption_height,
                divider: setting("captiondividerheight").map_or(Ok(2), ini_integer)?,
            }),
            None => None,
        };
        let atlas = match setting("groupName") {
            Some(group) => nodes.atlas(shared, group)?,
            None => Atlas::default(),
        };
        let border_uv = [
            "topline_pic",
            "bottomline_pic",
            "leftline_pic",
            "rightline_pic",
            "lefttop_pic",
            "righttop_pic",
            "leftbottom_pic",
            "rightbottom_pic",
        ]
        .map(|key| {
            atlas
                .named_part(setting(key))
                .map_or(FULL_UV, |part| part.0)
        });
        Ok(Self {
            background,
            caption,
            border_texture: atlas.texture,
            border_color: ini_color(setting("color"), [255; 4])?,
            corner_sizes: [
                setting("lefttop_size").map_or(Ok([9, 9]), tuple)?,
                setting("righttop_size").map_or(Ok([9, 9]), tuple)?,
                setting("leftbottom_size").map_or(Ok([9, 9]), tuple)?,
                setting("rightbottom_size").map_or(Ok([9, 9]), tuple)?,
            ],
            edge_sizes: [
                setting("topline_height").map_or(Ok(5), ini_integer)?,
                setting("bottomline_height").map_or(Ok(5), ini_integer)?,
                setting("leftline_width").map_or(Ok(5), ini_integer)?,
                setting("rightline_width").map_or(Ok(5), ini_integer)?,
            ],
            border_uv,
        })
    }

    pub(super) fn set_border_color(&mut self, rgba: [u8; 4]) {
        self.border_color = rgba;
    }

    pub(super) fn draw(&self, rect: [i32; 4], canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        let [left, top, right, bottom] = rect;
        if let Some(image) = &self.background {
            image.draw(rect, canvas, out);
        }
        if let Some(caption) = &self.caption
            && let Some(image) = &caption.image
        {
            let [offset_left, offset_top, offset_right, offset_bottom] = caption.offset;
            let caption_top = top.saturating_add(offset_top);
            image.draw(
                [
                    left.saturating_add(offset_left),
                    caption_top,
                    right.saturating_sub(offset_right),
                    caption_top
                        .saturating_add(caption.height)
                        .saturating_sub(offset_bottom),
                ],
                canvas,
                out,
            );
        }

        let Some(texture) = &self.border_texture else {
            return;
        };
        let [top_height, bottom_height, left_width, right_width] = self.edge_sizes;
        let [[lt_w, lt_h], [rt_w, rt_h], [lb_w, lb_h], [rb_w, rb_h]] = self.corner_sizes;
        let top_left_x = left.saturating_add(lt_w);
        let top_right_x = right.saturating_sub(rt_w);
        let top_left_y = top.saturating_add(lt_h);
        let top_right_y = top.saturating_add(rt_h);
        let bottom_left_y = bottom.saturating_sub(lb_h);
        let bottom_right_y = bottom.saturating_sub(rb_h);
        let left_edge = left.saturating_add(left_width);
        let right_edge = right.saturating_sub(right_width);
        let mut draw_tile = |rect: [i32; 4], uv| {
            out.push(InterfaceDraw::Picture(PictureDraw {
                texture: texture.clone(),
                canvas,
                rect: rect.map(|value| value as f32),
                uv,
                rgba: self.border_color,
                color: PictureColor::Modulate,
            }));
        };

        // The native bottom edge spans the same horizontal endpoints as the top edge.
        let tiles = [
            [top_left_x, top, top_right_x, top.saturating_add(top_height)],
            [
                top_left_x,
                bottom.saturating_sub(bottom_height),
                top_right_x,
                bottom,
            ],
            [left, top_left_y, left_edge, bottom_left_y],
            [right_edge, top_right_y, right, bottom_right_y],
            [left, top, top_left_x, top_left_y],
            [top_right_x, top, right, top_right_y],
            [left, bottom_left_y, left.saturating_add(lb_w), bottom],
            [right.saturating_sub(rb_w), bottom_right_y, right, bottom],
        ];
        for (rect, uv) in tiles.into_iter().zip(self.border_uv) {
            draw_tile(rect, uv);
        }
        if let Some(caption) = &self.caption {
            let divider_top = top.saturating_add(caption.height);
            draw_tile(
                [
                    left_edge,
                    divider_top,
                    right_edge,
                    divider_top.saturating_add(caption.divider),
                ],
                self.border_uv[0],
            );
        }
    }
}

impl FrameImage {
    fn load(nodes: &mut Nodes, shared: &Shared, value: &str) -> Result<Option<Self>> {
        let mut group = None;
        let mut name = None;
        let mut file = None;
        let mut color = None;
        let mut origin = [0.0_f32; 2];
        let mut uv = FULL_UV;
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
                // The containing frame supplies the image size and position.
                _ => {}
            }
        }
        ensure!(
            origin.iter().chain(&uv).all(|value| value.is_finite()),
            "invalid interface frame image UV"
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
        }))
    }

    fn draw(&self, rect: [i32; 4], canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
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

fn solid_mask() -> Rc<Texture> {
    Rc::new(Texture {
        header: TxHeader {
            flags: 0,
            width: 1,
            height: 1,
            mips: 1,
            format: 21,
            mip0_bytes: 4,
        },
        mips: vec![Mip {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
    })
}
