use crate::{
    GameDirectory, ScriptEncoding,
    config::{ini_field as field, ini_section as heading},
};
use anyhow::{Context, Result, ensure};
use storm_formats::texture::Texture;

const CANVAS: [i32; 2] = [800, 600];
const FILE: &str = "RESOURCE/INI/INTERFACES/interfaces.ini";

pub struct InterfaceScreen {
    pub section: String,
    pub size: [i32; 2],
    /// left, top, right, bottom in native interface coordinates.
    pub rect: [i32; 4],
    pub scale: f32,
    pub repeat_delay: i32,
    pub double_click_interval: i32,
}

pub struct InterfaceCursor {
    pub name: String,
    pub sensitivity: i32,
    pub size: [i32; 2],
    pub offset: [i32; 2],
    pub texture: Option<Texture>,
}

pub(super) fn load(
    game: &GameDirectory,
    window: [u32; 2],
) -> Result<(InterfaceScreen, Option<InterfaceCursor>)> {
    let bytes = game.read(FILE, 1024 * 1024)?;
    let text = ScriptEncoding::Utf8
        .decode(&bytes)
        .or_else(|_| ScriptEncoding::Windows1251.decode(&bytes))?;
    let ratio = f64::from(window[0] as f32 / window[1] as f32);
    let section = text
        .lines()
        .filter_map(heading)
        .find(|name| {
            let Some(size) = name.strip_prefix("PC_SCREEN_") else {
                return false;
            };
            let Some((width, height)) = size.split_once(['_', ':']) else {
                return false;
            };
            let candidate = integer(width) as f32 / integer(height) as f32;
            let candidate = f64::from(candidate);
            (candidate * 0.97..=candidate * 1.03).contains(&ratio)
        })
        .unwrap_or("PC_SCREEN");
    let mut scale = 1.0;
    let mut size = [
        (i64::from(window[0]) * i64::from(CANVAS[1]) / i64::from(window[1]))
            .max(i64::from(CANVAS[0]))
            .try_into()
            .context("interface canvas width overflow")?,
        CANVAS[1],
    ];
    let left = (size[0] - CANVAS[0]) / 2;
    let mut rect = [left, 0, left + CANVAS[0], CANVAS[1]];
    let get = |key, default| field(&text, section, key).map(integer).unwrap_or(default);
    if get("bDynamicScaling", 0) == 0 {
        scale = field(&text, section, "fScale")
            .and_then(|value| value.parse().ok())
            .unwrap_or(1.0);
        if !(0.1..=3.0).contains(&scale) {
            scale = 1.0;
        }
        size = [
            get("wScreenWidth", CANVAS[0]),
            get("wScreenHeight", CANVAS[1]),
        ];
        rect = [
            get("wScreenLeft", 0),
            get("wScreenTop", CANVAS[1]),
            get("wScreenRight", CANVAS[0]),
            get("wScreenDown", 0),
        ];
    }
    ensure!(
        size.iter().all(|&dimension| dimension > 0),
        "invalid interface canvas size"
    );
    let common = |key, default| field(&text, "COMMON", key).map(integer).unwrap_or(default);
    let screen = InterfaceScreen {
        section: section.to_owned(),
        size,
        rect,
        scale,
        repeat_delay: common("RepeatDelay", 500),
        double_click_interval: common("mouseDblClickInterval", 300),
    };
    let cursor = field(&text, "COMMON", "MousePointer")
        .filter(|value| !value.is_empty())
        .map(parse_cursor)
        .transpose()?;
    Ok((screen, cursor))
}

fn integer(text: &str) -> i32 {
    let text = text.trim_start();
    let sign = usize::from(text.starts_with(['+', '-']));
    let digits = text.as_bytes()[sign..]
        .iter()
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    text[..sign + digits].parse::<i64>().unwrap_or(0) as i32
}

fn parse_cursor(value: &str) -> Result<InterfaceCursor> {
    let (name, remaining) = value.split_once(',').context("cursor texture name")?;
    let (sensitivity, remaining) = remaining.split_once(',').context("cursor sensitivity")?;
    let (size, offset) = remaining
        .split_once("),")
        .context("cursor size and offset")?;
    let size = size
        .trim_ascii()
        .strip_prefix("size:(")
        .context("cursor size")?;
    let offset = offset
        .trim_ascii()
        .strip_prefix("pos:(")
        .and_then(|s| s.strip_suffix(')'))
        .context("cursor offset")?;
    let pair = |s: &str| -> Result<[i32; 2]> {
        let (x, y) = s.split_once(',').context("cursor coordinate pair")?;
        Ok([integer(x), integer(y)])
    };
    Ok(InterfaceCursor {
        name: name.to_owned(),
        sensitivity: integer(sensitivity),
        size: pair(size)?,
        offset: pair(offset)?,
        texture: None,
    })
}
