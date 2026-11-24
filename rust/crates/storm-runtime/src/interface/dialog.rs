//! XINTERFACE dialog activation and original script-owned dynamic strings.
mod nodes;
use super::{InterfaceScreen, Shared, load_texture};
use crate::{
    ScriptEncoding,
    config::{ini_integer, ini_section},
};
use anyhow::{Context, Result, ensure};
use std::{collections::BTreeMap, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Message};
use storm_render::interface_text::{Alignment, BitmapFont, InterfaceDraw, TextDraw};

pub(super) const INIT: i32 = 45010;
pub(super) const RELEASE: i32 = 45011;
pub(super) const CREATE_STRING: i32 = 45012;
pub(super) const ENABLE_STRING: i32 = 45014;
pub(super) const DISABLE_STRING: i32 = 45015;
pub(super) const SET_NODE_USING: i32 = 45016;
pub(super) const MSG_TO_NODE: i32 = 45045;
pub(super) const DELETE_STRING: i32 = 45050;
pub(super) const CHANGE_COLOR: i32 = 45051;
const FONT_INI: &str = "RESOURCE/INI/fonts.ini";
const FILE_BUDGET: usize = 2 * 1024 * 1024;
const MAX_STRINGS: usize = 64;

#[derive(Default)]
pub(super) struct Dialog {
    pub(super) active: bool,
    pub(super) missing: bool,
    strings: Vec<Entry>,
    pub(super) nodes: nodes::Nodes,
}
struct Entry {
    name: String,
    font: Option<Rc<BitmapFont>>,
    position: [i32; 2],
    alignment: Alignment,
    scale: f32,
    rgba: [u8; 4],
    enabled: bool,
}

impl Dialog {
    pub(super) fn activate(
        &mut self,
        shared: &Shared,
        path: &str,
        screen: &InterfaceScreen,
        title: Option<&str>,
    ) -> Result<()> {
        let nodes = shared
            .game
            .read_optional(path, FILE_BUDGET)?
            .map(|bytes| nodes::Nodes::load(shared, &decode(&bytes)?, screen, title))
            .transpose()?;
        let missing = nodes.is_none();
        if missing {
            eprintln!("Interface INI file not found: {path}");
        }
        *self = Self {
            active: true,
            missing,
            nodes: nodes.unwrap_or_default(),
            ..Self::default()
        };
        Ok(())
    }

    pub(super) fn create_string(&mut self, shared: &Shared, message: &mut Message) -> Result<()> {
        let name = message.string()?.to_owned();
        let font = load_font(shared, message.string()?)?;
        let rgba = color(message.long()?);
        let position = [message.long()?, message.long()?];
        let alignment = match message.long()? {
            1 => Alignment::Right,
            2 => Alignment::Center,
            _ => Alignment::Left,
        };
        let scale = message.float()?;
        ensure!(
            name.len() <= 255 && (0.0..=16.0).contains(&scale),
            "invalid interface string name or scale"
        );
        let index = self.find(&name);
        let entry = Entry {
            name,
            font,
            position,
            alignment,
            scale,
            rgba,
            enabled: true,
        };
        if let Some(index) = index {
            self.strings[index] = entry;
        } else {
            ensure!(self.strings.len() < MAX_STRINGS, "interface string budget");
            self.strings.push(entry);
        }
        Ok(())
    }

    pub(super) fn update_string(&mut self, code: i32, message: &mut Message) -> Result<()> {
        let Some(index) = self.find(message.string()?) else {
            return Ok(());
        };
        match code {
            ENABLE_STRING | DISABLE_STRING => self.strings[index].enabled = code == ENABLE_STRING,
            CHANGE_COLOR => self.strings[index].rgba = color(message.long()?),
            DELETE_STRING => {
                self.strings.swap_remove(index);
            }
            _ => unreachable!("validated interface string message"),
        }
        Ok(())
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.strings
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    pub(super) fn draws(
        &self,
        store: &AttributeStore,
        root: Option<AttributeId>,
        screen: &InterfaceScreen,
    ) -> Vec<InterfaceDraw> {
        let mut draws = Vec::new();
        self.nodes.draws(screen, false, &mut draws);
        draws.extend(
            self.string_draws(store, root, screen)
                .into_iter()
                .map(InterfaceDraw::Text),
        );
        self.nodes.draws(screen, true, &mut draws);
        draws
    }

    fn string_draws(
        &self,
        store: &AttributeStore,
        root: Option<AttributeId>,
        screen: &InterfaceScreen,
    ) -> Vec<TextDraw> {
        let Some(strings) = root.and_then(|id| store.child(id, "strings")) else {
            return Vec::new();
        };
        self.strings
            .iter()
            .filter_map(|entry| {
                if !entry.enabled {
                    return None;
                }
                let font = entry.font.as_ref()?;
                let text = store
                    .value(strings, Some(&entry.name))
                    .filter(|text| !text.is_empty())?;
                let [x, y] = entry.position;
                Some(TextDraw {
                    font: font.clone(),
                    text: text.chars().take(512).collect(),
                    canvas: screen.size,
                    position: [
                        x.saturating_add(screen.rect[0]),
                        y.saturating_add(screen.rect[1]),
                    ],
                    scale: entry.scale,
                    alignment: entry.alignment,
                    rgba: entry.rgba,
                })
            })
            .collect()
    }
}

fn color(native: i32) -> [u8; 4] {
    let [a, r, g, b] = native.to_be_bytes();
    [r, g, b, a]
}

fn decode(bytes: &[u8]) -> Result<String> {
    ScriptEncoding::Utf8
        .decode(bytes)
        .or_else(|_| ScriptEncoding::Windows1251.decode(bytes))
}

fn records<'a>(text: &'a str, section: &str) -> impl Iterator<Item = (&'a str, &'a str)> {
    let mut active = false;
    text.trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim_ascii)
        .filter_map(move |line| {
            if let Some(name) = ini_section(line) {
                active = name.eq_ignore_ascii_case(section);
                return None;
            }
            if !active || line.starts_with(';') || line.is_empty() {
                return None;
            }
            let (key, value) = line.split_once('=').unwrap_or((line, ""));
            Some((key.trim_ascii(), value.trim_ascii()))
        })
}

fn load_font(shared: &Shared, name: &str) -> Result<Option<Rc<BitmapFont>>> {
    let key = name.to_ascii_lowercase();
    if let Some(font) = shared
        .fonts
        .borrow()
        .get(&key)
        .and_then(std::rc::Weak::upgrade)
    {
        return Ok(Some(font));
    }
    let Some(bytes) = shared.game.read_optional(FONT_INI, FILE_BUDGET)? else {
        return Ok(None);
    };
    let text = decode(&bytes)?;
    let mut properties = BTreeMap::new();
    let mut glyphs = BTreeMap::new();
    for (name, value) in records(&text, name) {
        if let Some(glyph) = name.strip_prefix("char_") {
            let Some(glyph) = glyph_char(glyph) else {
                continue;
            };
            let rect = value
                .split(',')
                .map(|number| number.trim_ascii().parse::<u32>())
                .collect::<Result<Vec<_>, _>>()
                .with_context(|| format!("invalid font glyph {name}"))?;
            let rect: [u32; 4] = rect
                .try_into()
                .map_err(|_| anyhow::anyhow!("invalid font glyph {name}"))?;
            glyphs.entry(glyph).or_insert(rect);
        } else {
            properties.entry(name.to_ascii_lowercase()).or_insert(value);
        }
    }
    let Some(texture) = properties.get("texture") else {
        return Ok(None);
    };
    let Some(texture) = load_texture(shared, texture)? else {
        return Ok(None);
    };
    let metric = |key, default| {
        properties
            .get(key)
            .map_or(Ok(default), |value| ini_integer(value))
    };
    let pc_scale = properties.get("pcscale").map_or(Ok(1.0), |value| {
        value.parse::<f32>().context("invalid font pcscale")
    })?;
    let [width, height, cell_height] = [
        metric("texture_xsize", 1)?,
        metric("texture_ysize", 1)?,
        metric("height", 0)?,
    ];
    ensure!(
        (0.01..=16.0).contains(&pc_scale)
            && (1..=8192).contains(&width)
            && (1..=8192).contains(&height)
            && (0..=8192).contains(&cell_height)
            && glyphs
                .values()
                .all(|&[x, y, w, h]| x.saturating_add(w) <= width as u32
                    && y.saturating_add(h) <= height as u32),
        "invalid bitmap font dimensions"
    );
    let font = Rc::new(BitmapFont {
        texture: Rc::new(texture),
        sheet_size: [width as u32, height as u32],
        glyphs,
        height: cell_height as u32,
        pc_scale,
        interval: metric("symbol_interval", 0)?,
        space: metric("spacebar", 8)?,
        shadow: [metric("shadow_offsetx", 2)?, metric("shadow_offsety", 2)?],
    });
    let mut fonts = shared.fonts.borrow_mut();
    fonts.retain(|_, font| font.strong_count() != 0);
    fonts.insert(key, Rc::downgrade(&font));
    Ok(Some(font))
}

fn glyph_char(name: &str) -> Option<char> {
    if name == "equ" {
        return Some('=');
    }
    let mut chars = name.chars();
    let c = chars.next()?;
    if c.is_ascii_lowercase() {
        return (chars.as_str() == "_").then_some(c);
    }
    (chars.as_str().is_empty()).then_some(c)
}
