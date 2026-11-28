use super::{
    Nodes, Setting, Shared, flag, image::InterfaceImage, ini_color, ini_integer, load_font, tuple,
};
use crate::controls::EditKey;
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_core::{AttributeId, AttributeStore};
use storm_render::interface_text::{Alignment, BitmapFont, InterfaceDraw, TextDraw};

const TEXT_LIMIT: usize = 512;

pub(super) struct EditBox {
    images: [Option<InterfaceImage>; 3],
    font: Option<Rc<BitmapFont>>,
    scale: f32,
    color: [u8; 4],
    offset: [i32; 2],
    alignment: Alignment,
    length: usize,
    width: Option<i32>,
    excluded: String,
    disguised: bool,
    caret: Option<usize>,
    waiting_for_idle: bool,
}

impl EditBox {
    pub(super) fn load(nodes: &mut Nodes, shared: &Shared, setting: &Setting<'_>) -> Result<Self> {
        let font = setting("strFont")
            .map(|name| load_font(shared, name))
            .transpose()?
            .flatten();
        let scale: f32 = setting("fontScale").map_or(Ok(1.0), str::parse)?;
        ensure!(
            scale.is_finite() && scale >= 0.0,
            "invalid edit box font scale"
        );
        let size = setting("stringLength").map_or(Ok(-1), ini_integer)?;
        let width = setting("stringWidth").map_or(Ok(-1), ini_integer)?;
        let mut image = |key| -> Result<_> {
            let Some(value) = setting(key) else {
                return Ok(None);
            };
            Ok(InterfaceImage::load(nodes, shared, value)?
                .filter(|image| image.texture_width().is_some()))
        };
        Ok(Self {
            images: [
                image("leftImage")?,
                image("rightImage")?,
                image("middleImage")?,
            ],
            font,
            scale,
            color: ini_color(setting("argbFontColor"), [0; 4])?,
            offset: setting("stringoffset").map_or(Ok([0; 2]), tuple)?,
            alignment: match setting("stringalign")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "center" => Alignment::Center,
                "right" => Alignment::Right,
                _ => Alignment::Left,
            },
            length: usize::try_from(size).unwrap_or(TEXT_LIMIT).min(TEXT_LIMIT),
            width: (width >= 0).then_some(width),
            excluded: setting("excludechars").unwrap_or_default().to_owned(),
            disguised: flag(setting("disguisestring"), false),
            caret: None,
            waiting_for_idle: true,
        })
    }

    pub(super) fn initialize(
        store: &mut AttributeStore,
        root: AttributeId,
        name: &str,
    ) -> Result<()> {
        let field = match store.child(root, name) {
            Some(field) => field,
            None => store.create_attribute(root, name, None)?,
        };
        if store.value(field, Some("str")).is_none() {
            store.set_attribute(field, "str", Some(""))?;
        }
        Ok(())
    }

    pub(super) fn lose_focus(&mut self) {
        self.waiting_for_idle = true;
    }

    pub(super) fn apply(
        &mut self,
        store: &mut AttributeStore,
        root: AttributeId,
        name: &str,
        keys: &[EditKey],
    ) -> Result<()> {
        if keys.is_empty() {
            self.waiting_for_idle = false;
            return Ok(());
        }
        if self.waiting_for_idle {
            return Ok(());
        }
        Self::initialize(store, root, name)?;
        let Some(field) = store.child(root, name) else {
            return Ok(());
        };
        let previous = store.value(field, Some("str")).unwrap_or_default();
        let mut text: Vec<char> = previous.chars().collect();
        let mut caret = self.caret.unwrap_or(text.len()).min(text.len());
        for &key in keys {
            match key {
                EditKey::Home => caret = 0,
                EditKey::End => caret = text.len(),
                EditKey::Left => caret = caret.saturating_sub(1),
                EditKey::Right => caret = caret.saturating_add(1).min(text.len()),
                EditKey::Backspace if caret > 0 => {
                    caret -= 1;
                    text.remove(caret);
                }
                EditKey::Delete if caret < text.len() => {
                    text.remove(caret);
                }
                EditKey::Character(c) if text.len() < self.length && !self.excluded.contains(c) => {
                    text.insert(caret, c);
                    if self.width.is_some_and(|width| self.measure(&text) > width) {
                        text.remove(caret);
                    } else {
                        caret += 1;
                    }
                }
                _ => {}
            }
        }
        let text: String = text.into_iter().collect();
        if text != previous {
            store.set_attribute(field, "str", Some(&text))?;
        }
        self.caret = Some(caret);
        Ok(())
    }

    fn measure(&self, text: &[char]) -> i32 {
        self.font.as_ref().map_or(0, |font| {
            (text
                .iter()
                .take(TEXT_LIMIT)
                .map(|&c| font.glyph_advance(c))
                .sum::<f32>()
                .trunc()
                * self.scale) as i32
        })
    }

    pub(super) fn draw(
        &self,
        rect: [i32; 4],
        canvas: [i32; 2],
        value: &str,
        focused: bool,
        out: &mut Vec<InterfaceDraw>,
    ) {
        let [left, top, right, bottom] = rect;
        let [left_image, right_image, middle] = &self.images;
        let mut middle_left = left;
        let mut middle_right = right;
        if let Some(image) = left_image {
            middle_left = left.saturating_add(image.texture_width().unwrap_or_default());
            image.draw([left, top, middle_left, bottom], canvas, out);
        }
        if let Some(image) = right_image {
            middle_right = right.saturating_sub(image.texture_width().unwrap_or_default());
            image.draw([middle_right, top, right, bottom], canvas, out);
        }
        if let Some(image) = middle
            && middle_left < middle_right
        {
            image.draw([middle_left, top, middle_right, bottom], canvas, out);
        }
        let Some(font) = &self.font else {
            return;
        };
        let mut letters: Vec<char> = value.chars().take(TEXT_LIMIT).collect();
        if self.disguised {
            letters.fill('*');
        }
        let caret = self.caret.unwrap_or(letters.len()).min(letters.len());
        let inner_width = right
            .saturating_sub(left)
            .saturating_sub(self.offset[0].saturating_mul(2));
        let mut first = 0;
        if focused && !self.waiting_for_idle {
            while first < caret && self.measure(&letters[first..]) > inner_width {
                first += 1;
            }
        }
        let line = &letters[first..];
        let line_width = self.measure(line);
        let x = match self.alignment {
            Alignment::Left => left.saturating_add(self.offset[0]),
            Alignment::Center => ((i64::from(left) + i64::from(right)) / 2) as i32,
            Alignment::Right => right.saturating_sub(self.offset[0]),
        };
        let y = top.saturating_add(self.offset[1]);
        let mut push = |text: String, x, alignment| {
            if !text.is_empty() {
                out.push(InterfaceDraw::Text(TextDraw {
                    font: font.clone(),
                    text,
                    canvas,
                    position: [x, y],
                    vertical_anchor: None,
                    scale: self.scale,
                    alignment,
                    shadow: true,
                    rgba: self.color,
                }));
            }
        };
        let aligned_left = x.saturating_sub(match self.alignment {
            Alignment::Left => 0,
            Alignment::Center => line_width / 2,
            Alignment::Right => line_width,
        });
        let aligned_right = aligned_left.saturating_add(line_width);
        if aligned_left >= left && aligned_right <= right {
            push(line.iter().collect(), x, self.alignment);
        } else {
            let mut from = 0;
            let mut to = line.len();
            let mut start = aligned_left;
            while start < left && from < to {
                from += 1;
                start = aligned_right.saturating_sub(self.measure(&line[from..]));
            }
            while start.saturating_add(self.measure(&line[from..to])) > right && from < to {
                to -= 1;
            }
            push(line[from..to].iter().collect(), start, Alignment::Left);
        }
        if focused {
            let cursor_x = aligned_left.saturating_add(self.measure(&letters[first..caret]));
            push("_".to_owned(), cursor_x, Alignment::Left);
        }
    }
}
