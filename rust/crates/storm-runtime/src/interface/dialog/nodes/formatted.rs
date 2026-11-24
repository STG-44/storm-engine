use super::{Setting, Shared, color, flag, ini_color, ini_integer, load_font, tuple};
use anyhow::{Result, bail, ensure};
use std::rc::Rc;
use storm_core::Message;
use storm_render::interface_text::{Alignment, BitmapFont, InterfaceDraw, TextDraw};

const MAX_TEXT_BYTES: usize = 64 * 1024;
const MAX_LINE_BYTES: usize = 511;
const MAX_LINES: usize = 2048;

pub(super) struct FormattedText {
    font: Option<Rc<BitmapFont>>,
    scale: f32,
    color: [u8; 4],
    alignment: Alignment,
    inset: i32,
    spacing: i32,
    wrap_off: bool,
    lines: Vec<Line>,
    groups: usize,
    first_line: usize,
    vertical_offset: i32,
}

struct Line {
    spans: Vec<Span>,
    color: [u8; 4],
    group: usize,
}

struct Span {
    text: String,
    color: Option<[u8; 4]>,
}

#[derive(Clone, Copy)]
struct Letter {
    symbol: char,
    color: Option<[u8; 4]>,
}

impl FormattedText {
    pub(super) fn load<'a>(
        shared: &Shared,
        setting: &Setting<'_>,
        rect: [i32; 4],
        strings: impl Iterator<Item = &'a str>,
    ) -> Result<Self> {
        let font = setting("font")
            .map(|name| load_font(shared, name))
            .transpose()?
            .flatten();
        let scale = setting("fontScale").map_or(Ok(1.0), str::parse::<f32>)?;
        ensure!(
            (0.0..=16.0).contains(&scale),
            "invalid formatted text scale"
        );
        let spacing = setting("lineSpace").map_or(Ok(30), ini_integer)?;
        ensure!(
            ini_color(setting("backColor"), [0; 4])?[3] == 0
                && [
                    "videoName",
                    "UpEnableTexture",
                    "UpDisableTexture",
                    "DownEnableTexture",
                    "DownDisableTexture",
                    "scrollerName",
                    "syncnodes"
                ]
                .into_iter()
                .all(|name| setting(name).is_none_or(str::is_empty)),
            "unported XINTERFACE formatted text decoration or linked scroller"
        );
        let alignment = match setting("alignment")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "center" => Alignment::Center,
            "right" => Alignment::Right,
            _ => Alignment::Left,
        };
        let mut text = Self {
            font,
            scale,
            color: ini_color(setting("color"), [128, 128, 128, 255])?,
            alignment,
            inset: setting("leftoffset").map_or(Ok(0), ini_integer)?,
            spacing: if spacing == 0 { 10 } else { spacing },
            wrap_off: flag(setting("wrapoff"), false),
            lines: Vec::new(),
            groups: 0,
            first_line: 0,
            vertical_offset: 0,
        };
        for value in strings {
            let value = match value.strip_prefix('#') {
                Some(literal) => literal.to_owned(),
                None => shared.language.common_string(value)?,
            };
            text.append(&value, rect)?;
        }
        if setting("valignment").and_then(|value| ini_integer(value).ok()) == Some(1) {
            text.center_vertically(rect);
        }
        Ok(text)
    }

    pub(super) fn replace(&mut self, input: &str, rect: [i32; 4]) -> Result<()> {
        let lines = self.format(input, rect, self.wrap_off, 0)?;
        self.groups = usize::from(!lines.is_empty());
        self.lines = lines;
        self.first_line = 0;
        self.vertical_offset = 0;
        Ok(())
    }

    fn append(&mut self, input: &str, rect: [i32; 4]) -> Result<usize> {
        let lines = self.format(input, rect, self.wrap_off, self.groups)?;
        let count = lines.len();
        ensure!(
            self.lines.len() + count <= MAX_LINES,
            "formatted text line budget"
        );
        self.groups += usize::from(count != 0);
        self.lines.extend(lines);
        self.vertical_offset = 0;
        Ok(count)
    }

    fn format(
        &self,
        input: &str,
        [left, _, right, _]: [i32; 4],
        wrap_off: bool,
        group: usize,
    ) -> Result<Vec<Line>> {
        ensure!(input.len() <= MAX_TEXT_BYTES, "formatted text byte budget");
        let letters = marked_letters(input, wrap_off);
        let width = right.saturating_sub(left).saturating_sub(self.inset);
        let advance = |letter: &Letter| {
            self.font
                .as_ref()
                .map_or(0.0, |font| font.glyph_advance(letter.symbol) * self.scale)
        };
        let mut lines = Vec::new();
        let mut push = |letters: &[Letter]| -> Result<()> {
            ensure!(lines.len() < MAX_LINES, "formatted text line budget");
            let mut spans: Vec<Span> = Vec::new();
            for &Letter { symbol, color } in letters {
                if let Some(span) = spans.last_mut().filter(|span| span.color == color) {
                    span.text.push(symbol);
                } else {
                    spans.push(Span {
                        text: symbol.to_string(),
                        color,
                    });
                }
            }
            lines.push(Line {
                spans,
                group,
                color: self.color,
            });
            Ok(())
        };
        let mut remaining = letters.as_slice();
        while !remaining.is_empty() {
            let paragraph_end = remaining
                .iter()
                .position(|letter| letter.symbol == '\n')
                .unwrap_or(remaining.len());
            let paragraph = &remaining[..paragraph_end];
            if wrap_off {
                let mut bytes = 0;
                let mut length = 0.0;
                let end = paragraph
                    .iter()
                    .take_while(|letter| {
                        bytes += letter.symbol.len_utf8();
                        length += advance(letter);
                        bytes <= MAX_LINE_BYTES && (length as i32) <= width
                    })
                    .count();
                push(&paragraph[..end])?;
                break;
            }
            let mut bytes = 0;
            let mut length = 0.0;
            let mut last_space = None;
            let mut fitted_space = None;
            let mut end = 0;
            for letter in paragraph {
                bytes += letter.symbol.len_utf8();
                if bytes > MAX_LINE_BYTES {
                    break;
                }
                if letter.symbol == ' ' {
                    last_space = Some(end);
                    if (length as i32) <= width {
                        fitted_space = Some(end)
                    }
                }
                length += advance(letter);
                end += 1;
            }
            if length as i32 > width {
                if let Some(space) = fitted_space {
                    end = space;
                } else if let Some(space) = last_space {
                    // Keep an overwide word intact, as the native word wrapper does.
                    end = paragraph[..space]
                        .iter()
                        .position(|letter| letter.symbol == ' ')
                        .unwrap_or(space);
                }
            }
            push(&paragraph[..end])?;
            let skip_break = usize::from(end == paragraph_end && paragraph_end < remaining.len());
            remaining = &remaining[end + skip_break..];
            while remaining
                .first()
                .is_some_and(|letter| letter.symbol <= ' ' && letter.symbol != '\n')
            {
                remaining = &remaining[1..];
            }
        }
        Ok(lines)
    }

    fn visible_count(&self, rect: [i32; 4]) -> usize {
        if self.spacing <= 0 {
            0
        } else {
            rect[3].saturating_sub(rect[1]).max(0) as usize / self.spacing as usize
        }
    }

    fn center_vertically(&mut self, rect: [i32; 4]) {
        if !self.lines.is_empty() && self.lines.len() < self.visible_count(rect) {
            let height = rect[3].saturating_sub(rect[1]);
            self.vertical_offset =
                height.saturating_sub(self.spacing.saturating_mul(self.lines.len() as i32)) / 2;
        }
    }

    pub(super) fn send(&mut self, code: i32, message: &mut Message, rect: [i32; 4]) -> Result<u64> {
        match code {
            0 => return Ok(self.append(&message.script_variable()?.string()?, rect)? as u64),
            2 => {
                self.first_line = ((self.lines.len() as f32 * message.float()?) as usize)
                    .min(self.lines.len().saturating_sub(1))
            }
            5 => self.center_vertically(rect),
            6 => {
                return Ok(u64::from(if message.long()? != 0 {
                    self.first_line != 0
                } else {
                    self.lines.len() > self.first_line.saturating_add(self.visible_count(rect))
                }));
            }
            8 => {
                let group = message.long()?;
                let rgba = color(message.long()?);
                let group = if group < 0 {
                    self.groups.saturating_sub(1)
                } else {
                    group as usize
                };
                for line in self.lines.iter_mut().filter(|line| line.group == group) {
                    line.color = rgba;
                }
            }
            9 => {
                return Ok(self
                    .format(&message.script_variable()?.string()?, rect, false, 0)?
                    .len() as u64);
            }
            12 => {
                let group = message.long()? as usize;
                let y = self
                    .lines
                    .iter()
                    .skip(self.first_line)
                    .take(self.visible_count(rect))
                    .position(|line| line.group == group)
                    .map_or(-1, |row| self.line_y(rect, row));
                return Ok(y as i64 as u64);
            }
            14 => match message.long()? {
                0 => self.alignment = Alignment::Left,
                1 => self.alignment = Alignment::Right,
                2 => self.alignment = Alignment::Center,
                _ => return Ok(u64::MAX),
            },
            _ => bail!("unported XINTERFACE formatted text message {code}"),
        }
        Ok(0)
    }

    pub(super) fn draw(&self, rect: [i32; 4], canvas: [i32; 2], out: &mut Vec<InterfaceDraw>) {
        let Some(font) = &self.font else { return };
        let [left, _, right, _] = rect;
        let anchor = match self.alignment {
            Alignment::Left => left.saturating_add(self.inset),
            Alignment::Right => right.saturating_sub(self.inset),
            Alignment::Center => self
                .inset
                .saturating_add((left.saturating_add(right).saturating_sub(self.inset)) / 2),
        };
        let scaled_width = |span: &Span| (font.text_width(&span.text) as f32 * self.scale) as i32;
        for (row, line) in self
            .lines
            .iter()
            .skip(self.first_line)
            .take(self.visible_count(rect))
            .enumerate()
        {
            let y = self.line_y(rect, row);
            let mut push = |span: &Span, x, alignment| {
                out.push(InterfaceDraw::Text(TextDraw {
                    font: font.clone(),
                    text: span.text.clone(),
                    canvas,
                    position: [x, y],
                    scale: self.scale,
                    alignment,
                    shadow: true,
                    rgba: span.color.unwrap_or(line.color),
                }))
            };
            match line.spans.as_slice() {
                [] => {}
                [single] => push(single, anchor, self.alignment),
                spans => {
                    let width = spans.iter().map(scaled_width).sum::<i32>();
                    let mut x = match self.alignment {
                        Alignment::Left => anchor,
                        Alignment::Right => anchor.saturating_sub(width),
                        Alignment::Center => anchor.saturating_sub(width / 2),
                    };
                    for span in spans {
                        push(span, x, Alignment::Left);
                        x = x.saturating_add(scaled_width(span));
                    }
                }
            }
        }
    }

    fn line_y(&self, rect: [i32; 4], row: usize) -> i32 {
        rect[1]
            .saturating_add(self.vertical_offset)
            .saturating_add((row as i32).saturating_mul(self.spacing))
    }
}

fn marked_letters(mut input: &str, wrap_off: bool) -> Vec<Letter> {
    let mut output = Vec::new();
    let mut color = None;
    while !input.is_empty() {
        if input
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("</color>"))
        {
            color = None;
            input = &input[8..];
            continue;
        }
        if input
            .get(..7)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("<color="))
            && let Some((value, rest)) = input[7..].split_once('>')
            && let Ok([a, r, g, b]) = tuple(value)
        {
            color = Some([r, g, b, a]);
            input = rest;
            continue;
        }
        let escaped_break = (!wrap_off)
            .then(|| {
                input
                    .strip_prefix("\\n")
                    .or_else(|| input.strip_prefix("\\N"))
            })
            .flatten();
        let symbol = if let Some(rest) = escaped_break.or_else(|| input.strip_prefix(['\r', '\n']))
        {
            input = rest.trim_start_matches(|value: char| value <= ' ');
            '\n'
        } else {
            let symbol = input.chars().next().expect("nonempty text");
            input = &input[symbol.len_utf8()..];
            symbol
        };
        output.push(Letter { symbol, color });
    }
    output
}
