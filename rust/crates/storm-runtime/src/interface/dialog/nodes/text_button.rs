use super::{
    Atlas, FULL_UV, Nodes, Setting, Shared, Texture,
    button::{ACTIVATE, Action, CLICK, DOUBLE_CLICK},
    flag, ini_color, ini_integer, load_font, solid_mask, tuple,
};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_render::interface_text::{
    Alignment, BitmapFont, InterfaceDraw, PictureColor, PictureDraw, TextDraw,
};

pub(super) struct TextButton {
    texture: Option<Rc<Texture>>,
    cap_widths: [i32; 2],
    normal_uv: [[f32; 4]; 3],
    focus_uv: [[f32; 4]; 3],
    face_colors: [[u8; 4]; 2],
    shadow: Option<Shadow>,
    shadow_color: [u8; 4],
    background: Option<(Rc<Texture>, [u8; 4])>,
    font: Option<Rc<BitmapFont>>,
    text: String,
    font_colors: [[u8; 4]; 4],
    font_scale: f32,
    text_offset: Option<i32>,
    ink_center_y: f32,
    press_offset: [f32; 2],
    shadow_offset: [f32; 2],
    press_shadow_offset: [f32; 2],
    press_frames: u32,
    remaining_frames: u32,
}

struct Shadow {
    texture: Rc<Texture>,
    uv: [f32; 4],
    scale: f32,
}

impl TextButton {
    pub(super) fn load(
        nodes: &mut Nodes,
        shared: &Shared,
        setting: &Setting<'_>,
        text_offset: Option<&str>,
    ) -> Result<Self> {
        ensure!(
            setting("midVideo").is_none_or(str::is_empty),
            "unported XINTERFACE text button video"
        );
        let atlas = match setting("group") {
            Some(group) => nodes.atlas(shared, group)?,
            None => Atlas::default(),
        };
        let (left_uv, left_width) = atlas.part(setting("buttonLeft"));
        let (right_uv, right_width) = atlas
            .named_part(setting("buttonRight"))
            .unwrap_or_else(|| (flip_u(left_uv), left_width));
        let normal_uv = [left_uv, atlas.part(setting("buttonMiddle")).0, right_uv];
        let focused_left = atlas
            .named_part(setting("selectButtonLeft"))
            .map(|part| part.0);
        let focus_uv = [
            focused_left.unwrap_or(left_uv),
            atlas
                .named_part(setting("selectButtonMiddle"))
                .map_or(normal_uv[1], |part| part.0),
            atlas
                .named_part(setting("selectButtonRight"))
                .map(|part| part.0)
                .or_else(|| focused_left.map(flip_u))
                .unwrap_or(right_uv),
        ];
        let shadow = match setting("shadowTexture") {
            Some(name) => nodes.texture(shared, name)?,
            None => None,
        };
        let shadow = match shadow {
            Some(texture) => Some(Shadow {
                texture,
                uv: setting("shadowUV").map_or(Ok(FULL_UV), tuple)?,
                scale: setting("shadowScale").map_or(Ok(1.0), str::parse)?,
            }),
            None => None,
        };
        let offset = |key| setting(key).map_or(Ok([0.0_f32; 2]), tuple);
        let press_offset = offset("pressPictureOffset")?;
        let shadow_offset = offset("shadowOffset")?;
        let press_shadow_offset = offset("pressShadowOffset")?;
        let font_scale = setting("fontScale").map_or(Ok(1.0_f32), str::parse)?;
        ensure!(
            font_scale >= 0.0
                && font_scale.is_finite()
                && press_offset
                    .into_iter()
                    .chain(shadow_offset)
                    .chain(press_shadow_offset)
                    .all(f32::is_finite)
                && shadow
                    .as_ref()
                    .is_none_or(|s| s.scale.is_finite() && s.uv.iter().all(|n| n.is_finite())),
            "invalid XINTERFACE text button geometry"
        );
        let back = ini_color(setting("backColor"), [255; 4])?;
        let background =
            (back[3] != 0 && flag(setting("videoToBack"), true)).then(|| (solid_mask(), back));
        let font_color = ini_color(setting("fontColor"), [255; 4])?;
        let font = setting("font")
            .map(|name| load_font(shared, name))
            .transpose()?
            .flatten();
        let text = shared
            .language
            .common_string(setting("string").unwrap_or_default())?;
        let ink_center_y = font.as_ref().map_or(0.0, |font| font.ink_center_y(&text));
        Ok(Self {
            texture: atlas.texture,
            cap_widths: [left_width, right_width],
            normal_uv,
            focus_uv,
            face_colors: [
                ini_color(setting("faceColor"), [255; 4])?,
                ini_color(setting("faceColorPressed"), [255; 4])?,
            ],
            shadow,
            shadow_color: ini_color(setting("shadowColor"), [0, 0, 0, 255])?,
            background,
            font,
            text,
            font_colors: [
                font_color,
                ini_color(setting("fontColorSelected"), font_color)?,
                ini_color(setting("fontColorPressed"), font_color)?,
                ini_color(setting("unselectableFontColor"), [128, 128, 128, 255])?,
            ],
            font_scale,
            text_offset: text_offset.map(ini_integer).transpose()?,
            ink_center_y,
            press_offset,
            shadow_offset,
            press_shadow_offset,
            press_frames: setting("pressDelay").map_or(Ok(20), ini_integer)?.max(0) as u32,
            remaining_frames: 0,
        })
    }

    pub(super) fn set_text(&mut self, shared: &Shared, text: &str) -> Result<()> {
        self.text = match text.strip_prefix('#') {
            Some(literal) => literal.to_owned(),
            None => shared.language.common_string(text)?,
        };
        self.ink_center_y = self
            .font
            .as_ref()
            .map_or(0.0, |font| font.ink_center_y(&self.text));
        Ok(())
    }

    pub(super) fn begin_press(&mut self) {
        self.remaining_frames = self.press_frames;
    }

    pub(super) fn reset_press(&mut self) {
        self.remaining_frames = 0;
    }

    pub(super) fn execute(&mut self, action: Action, on_release: bool) {
        if matches!(action, CLICK | ACTIVATE | DOUBLE_CLICK) {
            if on_release {
                self.reset_press();
            } else {
                self.begin_press();
            }
        }
    }

    pub(super) fn draw(
        &mut self,
        rect: [i32; 4],
        canvas: [i32; 2],
        focused: bool,
        selectable: bool,
        on_release: bool,
        out: &mut Vec<InterfaceDraw>,
    ) {
        if !on_release {
            self.remaining_frames = self.remaining_frames.saturating_sub(1);
        }
        let pressed = self.remaining_frames > 0;
        let press = if pressed { self.press_offset } else { [0.0; 2] };
        let uv = if focused {
            self.focus_uv
        } else {
            self.normal_uv
        };
        let [left, top, right, bottom] = rect.map(|n| n as f32);
        let [left_width, right_width] = self.cap_widths.map(|n| n as f32);
        let slices = [
            [left, top, left + left_width, bottom],
            [left + left_width, top, right - right_width, bottom],
            [right - right_width, top, right, bottom],
        ];
        let mut picture = |texture: &Rc<Texture>, rect, uv, rgba, color| {
            out.push(InterfaceDraw::Picture(PictureDraw {
                texture: texture.clone(),
                canvas,
                rect,
                uv,
                rgba,
                color,
            }));
        };
        if self.shadow_color[3] != 0 {
            if let Some(shadow) = &self.shadow {
                let [x, y] = self.shadow_offset;
                let [dx, dy] = if pressed { [0.0; 2] } else { self.press_offset };
                let padx = (right - left) * (shadow.scale - 1.0) * 0.5;
                let pady = (bottom - top) * (shadow.scale - 1.0) * 0.5;
                picture(
                    &shadow.texture,
                    [
                        left - padx + x + dx,
                        top - pady + y + dy,
                        right + padx + x + dx,
                        bottom + pady + y + dy,
                    ],
                    shadow.uv,
                    self.shadow_color,
                    PictureColor::Modulate,
                );
            } else if let Some(texture) = &self.texture {
                let shadow = if pressed {
                    self.press_shadow_offset
                } else {
                    self.shadow_offset
                };
                let shift = [press[0] + shadow[0], press[1] + shadow[1]];
                for (slice, uv) in slices.into_iter().zip(uv) {
                    picture(
                        texture,
                        shift_rect(slice, shift),
                        uv,
                        self.shadow_color,
                        PictureColor::AlphaMask,
                    );
                }
            }
        }
        if let Some((mask, color)) = &self.background {
            picture(
                mask,
                shift_rect([left, top, right, bottom], press),
                FULL_UV,
                *color,
                PictureColor::AlphaMask,
            );
        }
        if let Some(texture) = &self.texture {
            for (slice, uv) in slices.into_iter().zip(uv) {
                picture(
                    texture,
                    shift_rect(slice, press),
                    uv,
                    self.face_colors[usize::from(pressed)],
                    PictureColor::Modulate,
                );
            }
        }
        if let Some(font) = &self.font
            && !self.text.is_empty()
        {
            let font_state = if pressed {
                2
            } else if !selectable {
                3
            } else {
                usize::from(focused)
            };
            let center = |start, end| ((i64::from(start) + i64::from(end)) / 2) as i32;
            let (y, vertical_anchor) = match self.text_offset {
                Some(offset) => (rect[1].saturating_add(offset), None),
                None => (center(rect[1], rect[3]), Some(self.ink_center_y)),
            };
            out.push(InterfaceDraw::Text(TextDraw {
                font: font.clone(),
                text: self.text.clone(),
                canvas,
                position: [
                    center(rect[0], rect[2]).saturating_add(press[0] as i32),
                    y.saturating_add(press[1] as i32),
                ],
                vertical_anchor,
                scale: self.font_scale,
                alignment: Alignment::Center,
                shadow: true,
                rgba: self.font_colors[font_state],
            }));
        }
    }
}

fn flip_u([left, top, right, bottom]: [f32; 4]) -> [f32; 4] {
    [right, top, left, bottom]
}

fn shift_rect([left, top, right, bottom]: [f32; 4], [x, y]: [f32; 2]) -> [f32; 4] {
    [left + x, top + y, right + x, bottom + y]
}
