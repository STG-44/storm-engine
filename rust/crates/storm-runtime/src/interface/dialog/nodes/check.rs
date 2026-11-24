use super::{Atlas, Nodes, Setting, Shared, flag, ini_color, inside, load_font, tuple};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_core::{AttributeId, AttributeStore, Message, Value};
use storm_formats::texture::Texture;
use storm_render::interface_text::{
    Alignment, BitmapFont, InterfaceDraw, PictureColor, PictureDraw, TextDraw,
};

const INI_SECTIONS: usize = 100;
const MAX_CHOICES: usize = 256;
const MAX_LABEL_BYTES: usize = 4096;

#[derive(Clone, Copy)]
enum State {
    Normal,
    Selected,
    Disabled,
}

impl State {
    fn of(selected: bool, disabled: bool) -> Self {
        if disabled {
            Self::Disabled
        } else if selected {
            Self::Selected
        } else {
            Self::Normal
        }
    }
}

struct Choice {
    label: String,
    lines: Vec<(String, i32)>,
    location: Option<[i32; 2]>,
    selected: bool,
    disabled: bool,
    sprite: State,
    tint: [u8; 4],
    dirty: bool,
}

pub(super) struct Change(usize, bool);

impl Change {
    pub(super) fn message(self, name: &str) -> Result<Message> {
        Message::new(
            "sll",
            vec![
                Value::String(name.to_owned()),
                Value::Long((self.0 + 1) as i32),
                Value::Long(i32::from(self.1)),
            ],
        )
    }
}

pub(super) struct CheckButtons {
    choices: Vec<Choice>,
    texture: Option<Rc<Texture>>,
    uv: [[f32; 4]; 3],
    icon_tints: [[u8; 4]; 3],
    icon_size: [f32; 2],
    icon_offset: [f32; 2],
    font: Option<Rc<BitmapFont>>,
    font_tints: [[u8; 4]; 3],
    scale: f32,
    alignment: Alignment,
    text_offset: [f32; 4],
    line_height: f32,
    section_interval: f32,
    exclusive: bool,
    text_clickable: bool,
    individual: bool,
}

impl CheckButtons {
    pub(super) fn load(
        nodes: &mut Nodes,
        shared: &Shared,
        setting: &Setting<'_>,
        rect: [i32; 4],
    ) -> Result<Self> {
        let atlas = match setting("icongroup") {
            Some(group) => nodes.atlas(shared, group)?,
            None => Atlas::default(),
        };
        let font = setting("font")
            .map(|name| load_font(shared, name))
            .transpose()?
            .flatten();
        let font_height = font
            .as_ref()
            .map_or(0.0, |f| (f.height as f32 * f.pc_scale).trunc());
        let scale = setting("fontScale").map_or(Ok(1.0), str::parse::<f32>)?;
        let normal_font = ini_color(setting("normalFontColor"), [255; 4])?;
        let normal_icon = ini_color(setting("normalpiccolor"), [255; 4])?;
        let normal_picture = setting("normalpicture");
        let icon_size: [f32; 2] = setting("iconsize").map_or(Ok([0.0; 2]), tuple)?;
        let icon_offset = setting("iconoffset").map_or(Ok([0.0; 2]), tuple)?;
        let text_offset = setting("rect_textoffset").map_or(Ok([0.0; 4]), tuple)?;
        let line_height = setting("lineheight").map_or(Ok(font_height), str::parse::<f32>)?;
        let section_interval =
            setting("sectioninterval").map_or(Ok(font_height), str::parse::<f32>)?;
        ensure!(
            scale >= 0.0
                && [scale, line_height, section_interval]
                    .iter()
                    .chain(&icon_size)
                    .chain(&icon_offset)
                    .chain(&text_offset)
                    .all(|value| value.is_finite()),
            "invalid check button geometry"
        );
        let uv = [
            normal_picture,
            setting("selectpicture").or(normal_picture),
            setting("disablepicture")
                .filter(|name| !name.is_empty())
                .or(normal_picture),
        ]
        .map(|name| {
            atlas.named_part(name).map_or_else(
                || {
                    let [w, h] = atlas.size.map(|n| n as f32);
                    [
                        0.0,
                        0.0,
                        (icon_size[0].trunc() / w).min(1.0),
                        (icon_size[1].trunc() / h).min(1.0),
                    ]
                },
                |part| part.0,
            )
        });
        let mut checks = Self {
            choices: Vec::new(),
            texture: atlas.texture,
            uv,
            icon_tints: [
                normal_icon,
                ini_color(setting("selectpiccolor"), normal_icon)?,
                ini_color(setting("disablepiccolor"), normal_icon)?,
            ],
            icon_size,
            icon_offset,
            font,
            font_tints: [
                normal_font,
                ini_color(setting("selectFontColor"), normal_font)?,
                ini_color(setting("disableFontColor"), normal_font)?,
            ],
            scale,
            alignment: match setting("alignment")
                .unwrap_or_default()
                .to_ascii_lowercase()
                .as_str()
            {
                "center" => Alignment::Center,
                "right" => Alignment::Right,
                _ => Alignment::Left,
            },
            text_offset,
            line_height,
            section_interval,
            exclusive: flag(setting("exclusiveChoose"), true),
            text_clickable: flag(setting("textClickable"), false),
            individual: flag(setting("individualpos"), false),
        };
        for id in 1..=INI_SECTIONS {
            let Some(raw) = setting(&format!("section{id}")) else {
                break;
            };
            let mut parts = raw.splitn(3, ',');
            let selected = flag(parts.next().map(str::trim_ascii), false);
            let disabled = flag(parts.next().map(str::trim_ascii), false);
            if let Some(choice) = checks.add(
                shared,
                parts.next().unwrap_or_default(),
                selected,
                disabled,
                rect,
            )? {
                choice.dirty = true;
            }
        }
        if checks.individual {
            for (index, choice) in checks.choices.iter_mut().enumerate() {
                if let Some(location) = setting(&format!("pos{}", index + 1)) {
                    choice.location = Some(tuple(location)?);
                }
            }
        }
        if checks.exclusive
            && !checks.choices.iter().any(|choice| choice.selected)
            && let Some(first) = checks.choices.first_mut()
        {
            set_state(first, true);
        }
        Ok(checks)
    }

    fn add(
        &mut self,
        shared: &Shared,
        text: &str,
        selected: bool,
        disabled: bool,
        rect: [i32; 4],
    ) -> Result<Option<&mut Choice>> {
        let Some(label) = (match text.strip_prefix('#') {
            Some(text) => Some(text.to_owned()),
            None => shared.language.lookup_common_string(text)?,
        }) else {
            return Ok(None);
        };
        ensure!(
            self.choices.len() < MAX_CHOICES,
            "check button choice budget"
        );
        let lines = self.layout(&label, rect)?;
        let sprite = State::of(selected, disabled);
        self.choices.push(Choice {
            label,
            lines,
            location: None,
            selected,
            disabled,
            sprite,
            tint: self.icon_tints[sprite as usize],
            dirty: false,
        });
        Ok(self.choices.last_mut())
    }

    fn layout(&self, label: &str, rect: [i32; 4]) -> Result<Vec<(String, i32)>> {
        ensure!(
            label.len() <= MAX_LABEL_BYTES,
            "check button label byte budget"
        );
        let width = rect[2]
            .saturating_sub(rect[0])
            .saturating_sub((self.text_offset[0] + self.text_offset[2]) as i32)
            .max(10);
        let measure = |text: &str| {
            self.font
                .as_ref()
                .map_or(0, |font| (font.text_width(text) as f32 * self.scale) as i32)
        };
        let mut lines = Vec::new();
        let mut rest = label;
        loop {
            let mut line = String::new();
            let mut previous_space = None;
            while let Some(ch) = rest.chars().next() {
                if ch == ' ' {
                    if measure(&line) >= width {
                        break;
                    }
                    previous_space = Some((rest, line.len()));
                    rest = rest.trim_start_matches(' ');
                    line.push(' ');
                    continue;
                }
                if rest.starts_with("\\n") || rest.starts_with("\\N") {
                    break;
                }
                line.push(ch);
                rest = &rest[ch.len_utf8()..];
            }
            if measure(&line) > width
                && let Some((remainder, length)) = previous_space
            {
                rest = remainder;
                line.truncate(length);
                line.push(' ');
            }
            rest = rest.trim_start_matches(' ');
            if let Some(after) = rest
                .strip_prefix("\\n")
                .or_else(|| rest.strip_prefix("\\N"))
            {
                rest = after;
                line.truncate(line.trim_end_matches(' ').len());
            }
            let unused = width.saturating_sub(measure(&line));
            let offset = match self.alignment {
                Alignment::Left => 0,
                Alignment::Center => unused / 2,
                Alignment::Right => unused,
            };
            lines.push((line, offset));
            if rest.is_empty() {
                return Ok(lines);
            }
        }
    }

    pub(super) fn move_to(&mut self, rect: [i32; 4]) -> Result<()> {
        let lines = self
            .choices
            .iter()
            .map(|choice| self.layout(&choice.label, rect))
            .collect::<Result<Vec<_>>>()?;
        for (choice, lines) in self.choices.iter_mut().zip(lines) {
            choice.lines = lines;
        }
        Ok(())
    }

    pub(super) fn publish(
        &mut self,
        store: &mut AttributeStore,
        root: AttributeId,
        name: &str,
    ) -> Result<()> {
        if !self.choices.iter().any(|choice| choice.dirty) {
            return Ok(());
        }
        let node = match store.child(root, name) {
            Some(node) => node,
            None => store.create_attribute(root, name, None)?,
        };
        for (index, choice) in self.choices.iter_mut().enumerate() {
            if choice.dirty {
                store.set_dword(
                    node,
                    Some(&format!("state{}", index + 1)),
                    u32::from(choice.selected),
                )?;
                choice.dirty = false;
            }
        }
        Ok(())
    }

    pub(super) fn send(
        &mut self,
        shared: &Shared,
        rect: [i32; 4],
        code: i32,
        message: &mut Message,
    ) -> Result<(u64, Option<Change>)> {
        let mut result = 0;
        let mut change = None;
        match code {
            0 => {
                let text = message.string()?.to_owned();
                let checked = message.long()? != 0;
                let disabled = message.long()? != 0;
                self.add(shared, &text, checked, disabled, rect)?;
            }
            1 => {
                let index = read_index(message)?;
                let label = message.string()?;
                if let Some(index) = index.filter(|&index| index < self.choices.len()) {
                    let lines = self.layout(label, rect)?;
                    self.choices[index].label = label.to_owned();
                    self.choices[index].lines = lines;
                }
            }
            2 => {
                let index = read_index(message)?;
                let checked = message.long()? != 0;
                change = index.and_then(|index| self.set_checked(index, checked));
            }
            3 => {
                let index = read_index(message)?;
                result = u64::from(
                    index
                        .and_then(|index| self.choices.get(index))
                        .is_some_and(|choice| choice.selected),
                );
            }
            4 => {
                result = self
                    .choices
                    .iter()
                    .position(|choice| choice.selected)
                    .unwrap_or(0) as u64
            }
            5 => {
                let index = read_index(message)?;
                let disabled = message.long()? != 0;
                if let Some(choice) = index.and_then(|index| self.choices.get_mut(index)) {
                    choice.disabled = disabled;
                    choice.sprite = State::of(choice.selected, disabled);
                }
            }
            _ => {}
        }
        Ok((result, change))
    }

    fn set_checked(&mut self, index: usize, checked: bool) -> Option<Change> {
        if (self.exclusive && !checked) || self.choices.get(index)?.selected == checked {
            return None;
        }
        if self.exclusive {
            for choice in &mut self.choices {
                if choice.selected && !choice.disabled {
                    set_state(choice, false);
                }
            }
        }
        set_state(&mut self.choices[index], checked);
        Some(Change(index, checked))
    }

    pub(super) fn can_click(&self, rect: [i32; 4], point: [f64; 2]) -> bool {
        (self.text_clickable && inside(rect, point))
            || self
                .origins(rect)
                .any(|(_, origin)| inside(self.icon_rect(origin), point))
    }

    pub(super) fn click(&mut self, rect: [i32; 4], point: [f64; 2]) -> Option<Change> {
        let [x, y] = point;
        let mut index = None;
        if self.text_clickable && !self.individual {
            if !(f64::from(rect[0])..=f64::from(rect[2])).contains(&x) {
                return None;
            }
            let mut top = rect[1];
            for (candidate, choice) in self.choices.iter().enumerate() {
                let bottom =
                    top.saturating_add((choice.lines.len() as f32 * self.line_height) as i32);
                if !choice.disabled && (f64::from(top)..=f64::from(bottom)).contains(&y) {
                    index = Some(candidate);
                    break;
                }
                top = bottom.saturating_add(self.section_interval as i32);
            }
        }
        let selected = index.or_else(|| {
            self.origins(rect)
                .enumerate()
                .find_map(|(index, (choice, origin))| {
                    (!choice.disabled && inside(self.icon_rect(origin), point)).then_some(index)
                })
        })?;
        self.set_checked(selected, self.exclusive || !self.choices[selected].selected)
    }

    fn origins(&self, rect: [i32; 4]) -> impl Iterator<Item = (&Choice, [f32; 2])> {
        let start = [rect[0] as f32, rect[1] as f32];
        self.choices.iter().scan(start, move |pos, choice| {
            if let Some([x, y]) = choice.location {
                *pos = [start[0] + x as f32, start[1] + y as f32];
            }
            let origin = *pos;
            pos[1] += self.line_height * choice.lines.len() as f32 + self.section_interval;
            Some((choice, origin))
        })
    }

    fn icon_rect(&self, [x, y]: [f32; 2]) -> [i32; 4] {
        let left = (x + self.icon_offset[0]) as i32;
        let top = (y + self.icon_offset[1]) as i32;
        [
            left,
            top,
            left.saturating_add(self.icon_size[0] as i32),
            top.saturating_add(self.icon_size[1] as i32),
        ]
    }

    pub(super) fn draw(
        &self,
        rect: [i32; 4],
        canvas: [i32; 2],
        selectable: bool,
        out: &mut Vec<InterfaceDraw>,
    ) {
        for (choice, origin) in self.origins(rect) {
            if let Some(texture) = &self.texture {
                out.push(InterfaceDraw::Picture(PictureDraw {
                    texture: texture.clone(),
                    canvas,
                    rect: self.icon_rect(origin).map(|n| n as f32),
                    uv: self.uv[choice.sprite as usize],
                    rgba: choice.tint,
                    color: PictureColor::Modulate,
                }));
            }
            if let Some(font) = &self.font {
                let color = self.font_tints
                    [State::of(choice.selected, choice.disabled || !selectable) as usize];
                for (index, (text, offset)) in choice.lines.iter().enumerate() {
                    out.push(InterfaceDraw::Text(TextDraw {
                        font: font.clone(),
                        text: text.clone(),
                        canvas,
                        position: [
                            (origin[0] + self.text_offset[0] + *offset as f32) as i32,
                            (origin[1] + self.text_offset[1] + self.line_height * index as f32)
                                as i32,
                        ],
                        scale: self.scale,
                        alignment: Alignment::Left,
                        shadow: true,
                        rgba: color,
                    }));
                }
            }
        }
    }
}

fn read_index(message: &mut Message) -> Result<Option<usize>> {
    let index = message.long()?;
    Ok(index
        .checked_sub(1)
        .and_then(|index| usize::try_from(index).ok()))
}

fn set_state(choice: &mut Choice, checked: bool) {
    choice.selected = checked;
    // The engine replaces the icon here, preserving the creation tint.
    choice.sprite = State::of(checked, false);
    choice.dirty = true;
}
