//! Configured language and common strings from the original interface service.
use crate::{GameDirectory, ScriptEncoding, host::NativeFunction};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use storm_core::{Entity, EntityInit, Message, ScriptScalar};
use storm_render::color::ColorCorrection;
use storm_script::Value;

pub(crate) const INTERFACE_LIBRARY: &str = "script_interface_functions";
const TEXTS: &str = "RESOURCE/INI/TEXTS";
const LANGUAGE_INI: &str = "RESOURCE/INI/TEXTS/language.ini";
const MAX_USER_CATALOGS: i32 = 128;
const TEXT_WHITESPACE: [char; 4] = [' ', '\t', '\r', '\n'];

#[derive(Default)]
struct Language {
    name: String,
    directory: Option<String>,
    strings: BTreeMap<String, String>,
    loaded_common: bool,
    catalogs: RefCell<BTreeMap<i32, Catalog>>,
}

struct Catalog {
    file: String,
    references: u32,
    strings: BTreeMap<String, String>,
}

#[derive(Clone)]
pub(crate) struct LanguageService(Rc<LanguageResources>);

struct LanguageResources {
    game: GameDirectory,
    language: RefCell<Option<Rc<Language>>>,
}

impl LanguageService {
    pub fn new(game: GameDirectory) -> Self {
        Self(Rc::new(LanguageResources {
            game,
            language: RefCell::default(),
        }))
    }

    fn get(&self) -> Result<Rc<Language>> {
        if let Some(language) = self.0.language.borrow().as_ref() {
            return Ok(language.clone());
        }
        let game = &self.0.game;
        let language = match read_text(game, LANGUAGE_INI)? {
            Some(ini) => Language::load(game, &ini, None)?,
            None => Language::default(),
        };
        let language = Rc::new(language);
        self.0.language.replace(Some(language.clone()));
        Ok(language)
    }

    pub(crate) fn name(&self) -> Result<String> {
        Ok(self.get()?.name.clone())
    }

    pub(crate) fn common_string(&self, key: &str) -> Result<String> {
        Ok(self.lookup_common_string(key)?.unwrap_or_default())
    }

    pub(crate) fn lookup_common_string(&self, key: &str) -> Result<Option<String>> {
        Ok(self.get()?.strings.get(&key.to_ascii_lowercase()).cloned())
    }

    fn set(&self, name: &str) -> Result<()> {
        let previous = self.get()?;
        if previous.name.eq_ignore_ascii_case(name) {
            return Ok(());
        }
        let game = &self.0.game;
        let Some(ini) = read_text(game, LANGUAGE_INI)? else {
            return Ok(());
        };
        let mut language = Language::load(game, &ini, Some(name))?;
        for (&id, catalog) in previous.catalogs.borrow().iter() {
            let strings = if language.loaded_common {
                let Some(strings) = language.read_catalog(game, &catalog.file)? else {
                    continue;
                };
                strings
            } else {
                catalog.strings.clone()
            };
            language.catalogs.get_mut().insert(
                id,
                Catalog {
                    file: catalog.file.clone(),
                    references: catalog.references,
                    strings,
                },
            );
        }
        self.0.language.replace(Some(Rc::new(language)));
        Ok(())
    }

    pub fn register(&self, host: &mut crate::NativeHost) -> Result<()> {
        let service = self.clone();
        host.entities.register_class("obj_strservice", move || {
            Ok(Box::new(LanguageActor {
                service: service.clone(),
            }))
        })?;
        Ok(())
    }
}

struct LanguageActor {
    service: LanguageService,
}

impl Entity for LanguageActor {
    fn init(&mut self, _: &mut EntityInit<'_>) -> Result<bool> {
        self.service.get()?;
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let language = self.service.get()?;
        match message.long()? {
            45100 => return Ok(language.open(&self.service.0.game, message.string()?)? as u64),
            45101 => language.close(message.long()?),
            45102 => {
                let id = message.long()?;
                let input = message.script_variable()?;
                let output = message.script_variable()?;
                let translated = match input.read() {
                    Ok(ScriptScalar::String(name)) => language.translation(id, &name),
                    _ => String::new(),
                };
                output.set(ScriptScalar::String(translated))?;
            }
            45103 => message
                .script_variable()?
                .set(ScriptScalar::String(language.name.clone()))?,
            _ => {}
        }
        Ok(0)
    }
}

impl Language {
    fn load(game: &GameDirectory, ini: &str, requested: Option<&str>) -> Result<Self> {
        let default = ini_value(ini, "COMMON", "defaultLanguage");
        let common = ini_value(ini, "COMMON", "strings");
        let mut name = requested.unwrap_or(default.unwrap_or("English"));
        if requested.is_none() && name.is_empty() {
            return Ok(Self::default());
        }
        if let (Some(_), Some(default)) = (requested, default)
            && (common.is_none() || ini_value(ini, "DIRECTORY", name).is_none())
        {
            name = default;
        }
        let directory = ini_value(ini, "DIRECTORY", name).map(str::to_owned);
        let mut strings = BTreeMap::new();
        let mut loaded_common = false;
        if let Some(directory) = &directory
            && let Some(common) = common
            && let Some(text) = read_text(game, &format!("{TEXTS}/{directory}/{common}"))?
        {
            loaded_common = true;
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
        Ok(Self {
            name: name.to_owned(),
            directory,
            strings,
            loaded_common,
            catalogs: RefCell::default(),
        })
    }

    fn open(&self, game: &GameDirectory, file: &str) -> Result<i32> {
        let mut catalogs = self.catalogs.borrow_mut();
        if let Some((&id, catalog)) = catalogs
            .iter_mut()
            .find(|(_, catalog)| catalog.file.eq_ignore_ascii_case(file))
        {
            catalog.references = catalog
                .references
                .checked_add(1)
                .context("language file reference limit")?;
            return Ok(id);
        }
        let Some(strings) = self.read_catalog(game, file)? else {
            return Ok(-1);
        };
        let id = (0..MAX_USER_CATALOGS)
            .find(|id| !catalogs.contains_key(id))
            .context("open language file limit")?;
        catalogs.insert(
            id,
            Catalog {
                file: file.to_owned(),
                references: 1,
                strings,
            },
        );
        Ok(id)
    }

    fn read_catalog(
        &self,
        game: &GameDirectory,
        file: &str,
    ) -> Result<Option<BTreeMap<String, String>>> {
        let Some(directory) = self.directory.as_ref().filter(|_| !file.is_empty()) else {
            return Ok(None);
        };
        Ok(read_text(game, &format!("{TEXTS}/{directory}/{file}"))?
            .filter(|text| !text.is_empty())
            .map(|text| catalog_strings(&text)))
    }

    fn close(&self, id: i32) {
        let mut catalogs = self.catalogs.borrow_mut();
        if let Some(catalog) = catalogs.get_mut(&id) {
            catalog.references -= 1;
            if catalog.references == 0 {
                catalogs.remove(&id);
            }
        }
    }

    fn translation(&self, id: i32, name: &str) -> String {
        self.catalogs
            .borrow()
            .get(&id)
            .and_then(|catalog| catalog.strings.get(&name.to_ascii_lowercase()))
            .cloned()
            .unwrap_or_default()
    }
}

fn catalog_strings(mut text: &str) -> BTreeMap<String, String> {
    let mut strings = BTreeMap::new();
    while let Some((header, body)) = text.split_once('{') {
        let Some((body, next)) = body.split_once('}') else {
            break;
        };
        text = next;
        let name = catalog_name(header);
        if !name.is_empty() {
            let value = body.trim_matches(TEXT_WHITESPACE);
            // Native trimming stops with one byte remaining in a whitespace-only value.
            let value = if value.is_empty() && !body.is_empty() {
                &body[..1]
            } else {
                value
            };
            strings
                .entry(name.to_ascii_lowercase())
                .or_insert_with(|| value.to_owned());
        }
    }
    strings
}

fn catalog_name(header: &str) -> &str {
    let bytes = header.as_bytes();
    if bytes.is_empty() {
        return "";
    }
    let (mut begin, mut end, mut search) = (0, 0, 0);
    while let Some(offset) = bytes
        .get(search..)
        .and_then(|remaining| remaining.iter().position(|&byte| byte == b'\n'))
    {
        begin = end;
        end = search + offset;
        // GetNextUsersString resumes two bytes after the previous LF.
        search = end + 2;
    }
    while begin <= end && TEXT_WHITESPACE.contains(&char::from(bytes[begin])) {
        begin += 1;
    }
    // A short or absent complete line uses the text through the opening brace.
    if end.saturating_sub(begin) <= 1 {
        end = bytes.len() - 1;
    }
    header
        .get(begin..=end)
        .unwrap_or_default()
        .trim_matches(TEXT_WHITESPACE)
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

pub(crate) fn functions(
    service: &LanguageService,
    correction: Rc<Cell<Option<ColorCorrection>>>,
    interfaces: crate::interface::Interfaces,
) -> Result<[(&'static str, NativeFunction); 10]> {
    service.get()?;
    let current = service.clone();
    let setter = service.clone();
    let common = service.clone();
    let opener = service.clone();
    let closer = service.clone();
    let translation = service.clone();
    let game = service.0.game.clone();
    let toggle = |visibility: bool| {
        let interfaces = interfaces.clone();
        NativeFunction::with_world(move |args, _, entities| {
            let [name, value] = args else {
                bail!("window update expects a name and a flag")
            };
            let Value::Int(value) = value.resolved()? else {
                bail!("window flag must be an integer")
            };
            let mut value = value != 0;
            if !visibility {
                value = !value;
            }
            if let Some(id) = entities.first_id("XINTERFACE") {
                interfaces.window_state(id, name.resolved()?.text()?, value, visibility);
            }
            Ok(Value::Void)
        })
    };
    let show = toggle(true);
    let disable = toggle(false);
    Ok([
        ("xi_windowshow", show),
        ("xi_windowdisable", disable),
        (
            "xi_iswindowenable",
            NativeFunction::with_world(move |args, _, entities| {
                let [name] = args else {
                    bail!("XI_IsWindowEnable expects a name")
                };
                let name = name.resolved()?;
                let name = name.text()?;
                let active = entities
                    .first_id("XINTERFACE")
                    .is_none_or(|id| interfaces.window_active(id, name));
                Ok(Value::Int(i32::from(active)))
            }),
        ),
        (
            "xi_setcolorcorrection",
            NativeFunction::new(move |args| {
                let [contrast, gamma, brightness] = args else {
                    bail!("XI_SetColorCorrection expects three arguments");
                };
                let channel = |value: &Value| -> Result<f32> {
                    let number = super::float_or_one(value)?;
                    ensure!(
                        number.is_finite(),
                        "color correction requires finite settings"
                    );
                    Ok(number)
                };
                correction.set(Some(ColorCorrection {
                    contrast: channel(contrast)?,
                    gamma: channel(gamma)?,
                    brightness: channel(brightness)?,
                }));
                Ok(Value::Void)
            }),
        ),
        (
            "languagegetlanguage",
            NativeFunction::new(move |args| {
                ensure!(args.is_empty(), "LanguageGetLanguage expects no arguments");
                Ok(Value::String(current.name()?))
            }),
        ),
        (
            "languagesetlanguage",
            NativeFunction::new(move |args| {
                let [name] = args else {
                    bail!("LanguageSetLanguage expects one argument");
                };
                if let Value::String(name) = name.resolved()? {
                    setter.set(&name)?;
                }
                Ok(Value::Void)
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
                Ok(Value::String(common.common_string(&key)?))
            }),
        ),
        (
            "languageopenfile",
            NativeFunction::new(move |args| {
                let [file] = args else {
                    bail!("LanguageOpenFile expects one argument")
                };
                Ok(Value::Int(match file.resolved()? {
                    Value::String(file) => opener.get()?.open(&game, &file)?,
                    _ => -1,
                }))
            }),
        ),
        (
            "languageclosefile",
            NativeFunction::new(move |args| {
                let [id] = args else {
                    bail!("LanguageCloseFile expects one argument")
                };
                // The original DATA::Get leaves the ID at -1 for a non-int argument.
                if let Value::Int(id) = id.resolved()? {
                    closer.get()?.close(id);
                }
                Ok(Value::Void)
            }),
        ),
        (
            "languageconvertstring",
            NativeFunction::new(move |args| {
                let [id, name] = args else {
                    bail!("LanguageConvertString expects two arguments")
                };
                let text = match (id.resolved()?, name.resolved()?) {
                    (Value::Int(id), Value::String(name)) => {
                        translation.get()?.translation(id, &name)
                    }
                    _ => String::new(),
                };
                Ok(Value::String(text))
            }),
        ),
    ])
}
