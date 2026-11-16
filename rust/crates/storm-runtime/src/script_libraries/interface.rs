//! Configured language and common strings from the original interface service.
use crate::{GameDirectory, ScriptEncoding, host::NativeFunction};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{OnceCell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use storm_core::{Entity, EntityInit, Message, ScriptScalar};
use storm_script::Value;

pub(crate) const INTERFACE_LIBRARY: &str = "script_interface_functions";
const TEXTS: &str = "RESOURCE/INI/TEXTS";
const MAX_USER_CATALOGS: i32 = 128;
const TEXT_WHITESPACE: [char; 4] = [' ', '\t', '\r', '\n'];

#[derive(Default)]
struct Language {
    name: String,
    directory: Option<String>,
    strings: BTreeMap<String, String>,
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
    language: OnceCell<Rc<Language>>,
}

impl LanguageService {
    pub fn new(game: GameDirectory) -> Self {
        Self(Rc::new(LanguageResources {
            game,
            language: OnceCell::new(),
        }))
    }

    fn get(&self) -> Result<Rc<Language>> {
        if let Some(language) = self.0.language.get() {
            return Ok(language.clone());
        }
        let language = Rc::new(Language::load(&self.0.game)?);
        Ok(self.0.language.get_or_init(|| language).clone())
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
        let directory = ini_value(&ini, "DIRECTORY", &name).map(str::to_owned);
        let mut strings = BTreeMap::new();
        if let Some(directory) = &directory
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
        Ok(Self {
            name,
            directory,
            strings,
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
        let Some(directory) = &self.directory else {
            return Ok(-1);
        };
        if file.is_empty() {
            return Ok(-1);
        }
        let Some(text) = read_text(game, &format!("{TEXTS}/{directory}/{file}"))? else {
            return Ok(-1);
        };
        if text.is_empty() {
            return Ok(-1);
        }
        let id = (0..MAX_USER_CATALOGS)
            .find(|id| !catalogs.contains_key(id))
            .context("open language file limit")?;
        catalogs.insert(
            id,
            Catalog {
                file: file.to_owned(),
                references: 1,
                strings: catalog_strings(&text),
            },
        );
        Ok(id)
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

pub(crate) fn functions(service: &LanguageService) -> Result<[(&'static str, NativeFunction); 5]> {
    let language = service.get()?;
    let current = language.clone();
    let common = language.clone();
    let opener = language.clone();
    let closer = language.clone();
    let game = service.0.game.clone();
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
                    common
                        .strings
                        .get(&key.to_ascii_lowercase())
                        .cloned()
                        .unwrap_or_default(),
                ))
            }),
        ),
        (
            "languageopenfile",
            NativeFunction::new(move |args| {
                let [file] = args else {
                    bail!("LanguageOpenFile expects one argument")
                };
                Ok(Value::Int(match file.resolved()? {
                    Value::String(file) => opener.open(&game, &file)?,
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
                    closer.close(id);
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
                    (Value::Int(id), Value::String(name)) => language.translation(id, &name),
                    _ => String::new(),
                };
                Ok(Value::String(text))
            }),
        ),
    ])
}
