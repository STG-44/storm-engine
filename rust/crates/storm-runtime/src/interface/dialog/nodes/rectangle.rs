use super::{Setting, color, ini_color};
use anyhow::Result;
use storm_core::Message;
use storm_render::interface_text::{InterfaceDraw, RectangleDraw};

pub(super) struct Rectangle {
    colors: [[u8; 4]; 4],
    border: [u8; 4],
}

impl Rectangle {
    pub(super) fn load(setting: &Setting<'_>) -> Result<Self> {
        Ok(Self {
            colors: [
                ini_color(setting("leftColor"), [0; 4])?,
                ini_color(setting("topColor"), [0; 4])?,
                ini_color(setting("rightColor"), [0; 4])?,
                ini_color(setting("bottomColor"), [0; 4])?,
            ],
            border: ini_color(setting("borderColor"), [0; 4])?,
        })
    }

    pub(super) fn recolor(&mut self, message: &mut Message) -> Result<()> {
        let face = color(message.long()?);
        let border = color(message.long()?);
        self.colors = [face, [255; 4], face, [255; 4]];
        self.border = border;
        Ok(())
    }

    pub(super) fn draw(&self, rect: [i32; 4], canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        let [left, top, right, bottom] = self.colors;
        let product = |a: [u8; 4], b: [u8; 4]| {
            std::array::from_fn(|i| ((u16::from(a[i]) * u16::from(b[i])) >> 8) as u8)
        };
        out.push(InterfaceDraw::Rectangle(RectangleDraw {
            canvas,
            rect,
            corners: [
                product(left, top),
                product(left, bottom),
                product(right, top),
                product(right, bottom),
            ],
            border: self.border,
        }));
    }
}
