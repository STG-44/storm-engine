use super::{InterfaceScreen, Setting, Shared, color, ini_color, ini_integer, load_font, tuple};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_core::Message;
use storm_render::interface_text::{Alignment, BitmapFont, InterfaceDraw, TextDraw};

const MAX_STRINGS: usize = 256;

pub(super) struct Strings {
    entries: Vec<Entry>,
    origin: [i32; 2],
}

struct Entry {
    id: Option<String>,
    font: Option<Rc<BitmapFont>>,
    text: String,
    rgba: [u8; 4],
    position: [i32; 2],
    scale: f32,
    alignment: Alignment,
    shadow: bool,
}

impl Strings {
    pub(super) fn load<'a>(
        shared: &Shared,
        screen: &InterfaceScreen,
        setting: &Setting<'_>,
        lines: impl Iterator<Item = &'a str>,
    ) -> Result<Self> {
        let origin = [screen.rect[0], screen.rect[1]];
        let mut collection = Self {
            entries: Vec::new(),
            origin,
        };
        let relative = setting("bAbsoluteRectangle").map_or(Ok(0), ini_integer)? == 0;
        for line in lines {
            ensure!(
                collection.entries.len() < MAX_STRINGS,
                "string collection entry budget"
            );
            let (name, fields) = line.split_once(',').unwrap_or((line, ""));
            let fields = parameters(fields);
            let setting = |key| {
                fields
                    .iter()
                    .find_map(|(name, value)| name.eq_ignore_ascii_case(key).then_some(*value))
            };
            let state = setting("state").unwrap_or_default();
            let position = setting("pos").map_or(Ok([0; 2]), tuple)?;
            let scale: f32 = setting("scale").map_or(Ok(0.0), str::parse)?;
            ensure!(
                scale.is_finite() && scale >= 0.0,
                "invalid string collection scale"
            );
            let alignment = state
                .bytes()
                .find_map(|c| match c.to_ascii_lowercase() {
                    b'c' => Some(Alignment::Center),
                    b'r' => Some(Alignment::Right),
                    _ => None,
                })
                .unwrap_or(Alignment::Left);
            collection.entries.push(Entry {
                id: None,
                font: setting("font")
                    .map(|font| load_font(shared, font))
                    .transpose()?
                    .flatten(),
                text: caption(shared, name.trim_ascii())?,
                rgba: ini_color(setting("fc"), [0; 4])?,
                position: if relative {
                    offset(position, origin)
                } else {
                    position
                },
                scale,
                alignment,
                shadow: state.bytes().any(|c| c.eq_ignore_ascii_case(&b's')),
            });
        }
        Ok(collection)
    }

    pub(super) fn send(&mut self, shared: &Shared, code: i32, message: &mut Message) -> Result<()> {
        match code {
            0 => self.dynamic(shared, message)?,
            1 => {
                let index = index(message)?;
                let key = message.string()?;
                if let Some(entry) = self.entries.get_mut(index) {
                    entry.text = caption(shared, key)?;
                }
            }
            2 => {
                let target = index(message)?;
                let source = index(message)?;
                if target != source
                    && target < self.entries.len()
                    && let Some(source) = self.entries.get(source)
                {
                    self.entries[target].text = source.text.clone();
                }
            }
            3 => {
                let index = index(message)?;
                let rgba = color(message.long()?);
                if let Some(entry) = self.entries.get_mut(index) {
                    entry.rgba = rgba;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn dynamic(&mut self, shared: &Shared, message: &mut Message) -> Result<()> {
        let id = message.string()?.to_owned();
        let text = message.string()?.to_owned();
        if id.is_empty() {
            return Ok(());
        }
        let existing = self.entries.iter().position(|entry| {
            entry
                .id
                .as_ref()
                .is_some_and(|key| key.eq_ignore_ascii_case(&id))
        });
        if text.is_empty() {
            if let Some(index) = existing {
                self.entries.remove(index);
            }
            return Ok(());
        }
        ensure!(
            existing.is_some() || self.entries.len() < MAX_STRINGS,
            "string collection entry budget"
        );
        let font = load_font(shared, message.string()?)?;
        let position = offset([message.long()?, message.long()?], self.origin);
        let rgba = color(message.long()?);
        // The native ExtPrint accepts a background color but only draws the glyphs.
        let _background = message.long()?;
        let alignment = match message.long()? {
            1 => Alignment::Right,
            2 => Alignment::Center,
            _ => Alignment::Left,
        };
        let shadow = message.long()? != 0;
        let mut scale = message.float()?;
        ensure!(
            scale.is_finite() && scale >= 0.0,
            "invalid string collection scale"
        );
        let width = message.long()?;
        if width > 0
            && let Some(font) = &font
        {
            let measured = (font.text_width(&text) as f32 * scale) as i32;
            if measured > width {
                scale *= (width - 1) as f32 / measured as f32;
            }
        }
        let entry = Entry {
            id: Some(id),
            font,
            text,
            rgba,
            position,
            scale,
            alignment,
            shadow,
        };
        if let Some(index) = existing {
            self.entries[index] = entry;
        } else {
            self.entries.push(entry);
        }
        Ok(())
    }

    pub(super) fn move_by(&mut self, delta: [i32; 2]) {
        for entry in &mut self.entries {
            entry.position = offset(entry.position, delta);
        }
    }

    pub(super) fn draw(&self, canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        for entry in &self.entries {
            if let Some(font) = &entry.font
                && !entry.text.is_empty()
            {
                out.push(InterfaceDraw::Text(TextDraw {
                    font: font.clone(),
                    text: entry.text.clone(),
                    canvas,
                    position: entry.position,
                    vertical_anchor: None,
                    scale: entry.scale,
                    alignment: entry.alignment,
                    shadow: entry.shadow,
                    rgba: entry.rgba,
                }));
            }
        }
    }
}

fn caption(shared: &Shared, name: &str) -> Result<String> {
    match name.strip_prefix('#') {
        Some(text) => Ok(text.to_owned()),
        None => shared.language.common_string(name),
    }
}

fn index(message: &mut Message) -> Result<usize> {
    Ok(message
        .long()?
        .checked_sub(1)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(usize::MAX))
}

fn offset([x, y]: [i32; 2], [dx, dy]: [i32; 2]) -> [i32; 2] {
    [x.saturating_add(dx), y.saturating_add(dy)]
}

fn parameters(fields: &str) -> Vec<(&str, &str)> {
    let mut depth = 0i32;
    fields
        .split(|character| match character {
            '{' => {
                depth += 1;
                false
            }
            '}' => {
                depth -= 1;
                false
            }
            ',' => depth == 0,
            _ => false,
        })
        .filter_map(|field| {
            let (key, value) = field.split_once(':')?;
            Some((
                key.trim_ascii(),
                value.trim_ascii().trim_matches(['{', '}']),
            ))
        })
        .collect()
}
