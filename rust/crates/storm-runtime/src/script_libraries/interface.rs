//! Configured language and common strings from the original interface service.
use crate::{GameDirectory, ScriptEncoding, host::NativeFunction};
use anyhow::{Result, bail, ensure};
use std::{collections::BTreeMap, rc::Rc};
use storm_script::Value;

pub(crate) const INTERFACE_LIBRARY: &str = "script_interface_functions";
const TEXTS: &str = "RESOURCE/INI/TEXTS";

#[derive(Default)]
struct Language {
    name: String,
    strings: BTreeMap<String, String>,
}

impl Language {
    fn load(game: &GameDirectory) -> Result<Self> {
        let Some(ini) = read_text(game, &format!("{TEXTS}/language.ini"))? else {
            return Ok(Self::default());
        };
        let name = ini_value(&ini, "COMMON", "defaultLanguage")
            .unwrap_or("English")
            .to_owned();
        if name.is_empty() {
            return Ok(Self::default());
        }
        let mut strings = BTreeMap::new();
        if let Some(directory) = ini_value(&ini, "DIRECTORY", &name)
            && let Some(common) = ini_value(&ini, "COMMON", "strings")
            && let Some(text) = read_text(game, &format!("{TEXTS}/{directory}/{common}"))?
        {
            for line in ini_lines(&text) {
                if line.starts_with('[') {
                    break;
                }
                let Some((key, record)) = assignment(line) else {
                    continue;
                };
                if !key.eq_ignore_ascii_case("string") {
                    continue;
                }
                let (name, value) = record.split_once(',').unwrap_or((record, ""));
                let name = name.trim_ascii();
                if !name.is_empty() {
                    let translated = value.split('"').nth(1).unwrap_or("");
                    strings
                        .entry(name.to_ascii_lowercase())
                        .or_insert_with(|| translated.to_owned());
                }
            }
        }
        Ok(Self { name, strings })
    }
}

fn read_text(game: &GameDirectory, path: &str) -> Result<Option<String>> {
    game.read_optional(path, 2 * 1024 * 1024)?
        .map(|bytes| {
            ScriptEncoding::Utf8
                .decode(&bytes)
                .or_else(|_| ScriptEncoding::Windows1251.decode(&bytes))
        })
        .transpose()
}

fn ini_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines()
        .map(str::trim_ascii)
        .filter(|line| !line.starts_with(';') && !line.is_empty())
}

fn assignment(line: &str) -> Option<(&str, &str)> {
    line.split_once('=')
        .map(|(key, value)| (key.trim_ascii(), value.trim_ascii()))
}

fn ini_value<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut selected = false;
    for line in ini_lines(text) {
        if let Some(header) = line.strip_prefix('[') {
            selected = header
                .split_once(']')
                .is_some_and(|(name, _)| name.eq_ignore_ascii_case(section));
        } else if selected
            && let Some((name, value)) = assignment(line)
            && name.eq_ignore_ascii_case(key)
        {
            return Some(value);
        }
    }
    None
}

pub(crate) fn functions(game: &GameDirectory) -> Result<[(&'static str, NativeFunction); 2]> {
    let language = Rc::new(Language::load(game)?);
    let current = language.clone();
    Ok([
        (
            "languagegetlanguage",
            NativeFunction::new(move |args| {
                ensure!(args.is_empty(), "LanguageGetLanguage expects no arguments");
                Ok(Value::String(current.name.clone()))
            }),
        ),
        (
            "xi_convertstring",
            NativeFunction::new(move |args| {
                let [key] = args else {
                    bail!("XI_ConvertString expects a string key");
                };
                let Value::String(key) = key.resolved()? else {
                    bail!("XI_ConvertString expects a string key");
                };
                Ok(Value::String(
                    language
                        .strings
                        .get(&key.to_ascii_lowercase())
                        .cloned()
                        .unwrap_or_default(),
                ))
            }),
        ),
    ])
}
