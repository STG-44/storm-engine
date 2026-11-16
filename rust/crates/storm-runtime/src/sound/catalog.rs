use crate::{GameDirectory, random::Random};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;

const ALIAS_DIRECTORY: &str = "RESOURCE/INI/aliases";
const MAX_INI_BYTES: usize = 1024 * 1024;
pub(super) const MAX_SCHEME_CHANNELS: usize = 100;

pub(super) type Aliases = BTreeMap<String, Alias>;

#[derive(Default)]
pub(super) struct Alias {
    files: Vec<(String, f32)>,
    volume: Option<f32>,
}

impl Alias {
    pub(super) fn choose(&self, volume: f32, rng: &mut Random) -> Option<(&str, f32)> {
        let mut selected = self.files.last()?;
        let total = self
            .files
            .iter()
            .map(|(_, weight)| f64::from(*weight))
            .sum::<f64>();
        let mut roll = f64::from(rng.unit()) * total;
        for file in &self.files {
            roll -= f64::from(file.1);
            if roll < 0.0 {
                selected = file;
                break;
            }
        }
        Some((&selected.0, self.volume.unwrap_or(volume)))
    }
}

pub(super) fn default_aliases(game: &GameDirectory) -> Result<Aliases> {
    let mut aliases = Aliases::new();
    let Some(path) = game.resolve_optional(ALIAS_DIRECTORY)? else {
        return Ok(aliases);
    };
    let mut names = Vec::new();
    for (index, entry) in std::fs::read_dir(path)?.enumerate() {
        ensure!(index < 4096, "sound alias directory limit");
        let entry = entry?;
        if entry.file_type()?.is_file()
            && let Ok(name) = entry.file_name().into_string()
            && name.to_ascii_lowercase().ends_with(".ini")
        {
            names.push(name);
        }
    }
    names.sort_unstable();
    for name in names {
        load_aliases(game, &mut aliases, &name)?;
    }
    Ok(aliases)
}

pub(super) fn load_aliases(game: &GameDirectory, aliases: &mut Aliases, name: &str) -> Result<()> {
    let Some(bytes) = game.read_optional(&format!("{ALIAS_DIRECTORY}/{name}"), MAX_INI_BYTES)?
    else {
        return Ok(());
    };
    let text = decode_ini(&bytes);
    for (section, fields) in sections(&text) {
        ensure!(
            aliases.len() < 8192 || aliases.contains_key(section),
            "sound alias limit"
        );
        aliases.entry(section.to_owned()).or_insert_with(|| {
            let mut alias = Alias::default();
            for (key, value) in fields.iter().copied() {
                if key.eq_ignore_ascii_case("volume") {
                    alias.volume = positive(value);
                } else if key.eq_ignore_ascii_case("name") && alias.files.len() < 50 {
                    let (file, weight) = value.split_once(',').unwrap_or((value, "1"));
                    if !file.trim().is_empty()
                        && let Some(weight) = positive(weight)
                    {
                        alias.files.push((file.trim().to_owned(), weight));
                    }
                }
            }
            alias
        });
    }
    Ok(())
}

pub(super) struct Channel {
    pub(super) name: String,
    pub(super) volume: f32,
    pub(super) looped: bool,
    delay_ms: [f32; 2],
    next_ms: Option<u32>,
}

impl Channel {
    pub(super) fn advance(&mut self, delta_ms: u32, rng: &mut Random) -> bool {
        match self.next_ms {
            None => false,
            Some(0) => {
                let [min, max] = self.delay_ms;
                self.next_ms = (!self.looped).then(|| (min + (max - min) * rng.unit()) as u32);
                true
            }
            Some(remaining) => {
                self.next_ms = Some(remaining.saturating_sub(delta_ms));
                false
            }
        }
    }
}

pub(super) fn scheme(game: &GameDirectory, name: &str, rng: &mut Random) -> Result<Vec<Channel>> {
    let Some(bytes) = game.read_optional("RESOURCE/INI/sound_scheme.ini", MAX_INI_BYTES)? else {
        return Ok(Vec::new());
    };
    let text = decode_ini(&bytes);
    let Some((_, fields)) = sections(&text)
        .into_iter()
        .find(|(section, _)| section.eq_ignore_ascii_case(name))
    else {
        return Ok(Vec::new());
    };
    let mut result = Vec::new();
    for (key, looped) in [("ch", false), ("ch_loop", true)] {
        for (_, value) in fields
            .iter()
            .filter(|(field, _)| field.eq_ignore_ascii_case(key))
        {
            ensure!(
                result.len() < MAX_SCHEME_CHANNELS,
                "sound scheme channel limit"
            );
            let mut fields = value.split(',').map(str::trim);
            let name = fields.next().filter(|name| !name.is_empty());
            let Some(name) = name else { continue };
            let first = fields.next().and_then(|n| n.parse::<i32>().ok());
            let second = fields.next().and_then(|n| n.parse::<i32>().ok());
            let [min, max] = match (first, second) {
                (Some(min), Some(max)) => [min.max(0) as f32 * 1000.0, max.max(0) as f32 * 1000.0],
                (Some(max), None) => [0.0, max.max(0) as f32 * 1000.0],
                _ => [0.0, i32::MAX as f32],
            };
            let volume = fields
                .next()
                .and_then(|v| v.parse::<f32>().ok())
                .filter(|v| v.is_finite())
                .unwrap_or(1.0);
            result.push(Channel {
                name: name.to_owned(),
                volume,
                looped,
                delay_ms: [min, max],
                next_ms: Some(if looped {
                    0
                } else {
                    ((max - min) * rng.unit()) as u32
                }),
            });
        }
    }
    Ok(result)
}

fn decode_ini(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.trim_start_matches('\u{feff}').into();
    }
    encoding_rs::WINDOWS_1251
        .decode_without_bom_handling(bytes)
        .0
}

fn sections(text: &str) -> Vec<(&str, Vec<(&str, &str)>)> {
    let mut sections = Vec::<(&str, Vec<(&str, &str)>)>::new();
    for line in text.lines().map(str::trim) {
        if let Some(section) = line.strip_prefix('[').and_then(|line| line.split_once(']')) {
            sections.push((section.0.trim(), Vec::new()));
        } else if !line.starts_with(';')
            && let Some((key, value)) = line.split_once('=')
            && let Some((_, fields)) = sections.last_mut()
        {
            fields.push((key.trim(), value.trim()));
        }
    }
    sections
}

fn positive(value: &str) -> Option<f32> {
    value
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|n| n.is_finite() && *n > 0.0)
}
