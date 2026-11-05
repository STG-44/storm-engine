use crate::{Engine, GameDirectory, GameSource, NativeHost, ScriptEncoding};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

pub struct Startup {
    pub program_directory: String,
    pub run: String,
}
pub(crate) fn root_fields(text: &str) -> Result<BTreeMap<String, Option<String>>> {
    ensure!(
        text.len() <= 65536 && !text.contains('\0'),
        "engine.ini byte limit or NUL"
    );
    let mut root = true;
    let mut fields = BTreeMap::new();
    for line in text.split(['\r', '\n']) {
        let line = line.trim_matches([' ', '\t']);
        if line.starts_with('[') {
            ensure!(line.contains(']'), "unterminated engine.ini section");
            root = false;
            continue;
        }
        if !root || line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (name, value) = if let Some((name, value)) = line.split_once('=') {
            let value = value.trim_matches([' ', '\t']);
            (
                name.trim_end_matches([' ', '\t']),
                if value.is_empty() {
                    None
                } else {
                    Some(value.to_owned())
                },
            )
        } else {
            (line, None)
        };
        fields.entry(name.to_ascii_lowercase()).or_insert(value);
    }
    Ok(fields)
}
pub(crate) fn configuration_text(game: &GameDirectory) -> Result<String> {
    let bytes = game.read("engine.ini", 65536)?;
    if let Some(text) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return Ok(std::str::from_utf8(text)
            .context("UTF-8 engine.ini BOM")?
            .to_owned());
    }
    let (text, errors) = encoding_rs::WINDOWS_1251.decode_without_bom_handling(&bytes);
    ensure!(!errors, "invalid engine.ini code page");
    Ok(text.into_owned())
}
impl Startup {
    /// Root-section startup directives only. Original IFS keeps inline ';'
    /// characters, first matching duplicate keys and leading/trailing trimming.
    /// Full INIFILE services and application/window directives are not yet ported.
    pub fn parse(text: &str) -> Result<Self> {
        let mut fields = root_fields(text)?;
        let run = fields
            .remove("run")
            .flatten()
            .context("engine.ini has no root 'run' script directive")?;
        ensure!(!run.is_empty(), "engine.ini has empty 'run' directive");
        Ok(Self {
            program_directory: fields
                .remove("program_directory")
                .flatten()
                .unwrap_or_default(),
            run,
        })
    }
    pub fn load(game: &GameDirectory) -> Result<Self> {
        Self::parse(&configuration_text(game)?)
    }
    pub fn engine(
        &self,
        game: &GameDirectory,
        encoding: ScriptEncoding,
        host: NativeHost,
    ) -> Result<Engine> {
        let directory = if self.program_directory.is_empty() {
            game.clone()
        } else {
            game.directory(&self.program_directory)
                .context("open configured program_directory; supply the installed game's original PROGRAM folder")?
        };
        let mut source = GameSource {
            directory,
            encoding,
        };
        let runtime = storm_script::Runtime::compile_with_loader(&self.run, &mut source)
            .with_context(|| format!("compile configured Storm program {}", self.run))?;
        Ok(Engine::from_runtime(runtime, host))
    }
}
