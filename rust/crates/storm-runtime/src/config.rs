use crate::{Engine, GameDirectory, GameSource, NativeHost, ScriptEncoding};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

pub struct Startup {
    pub program_directory: String,
    pub run: String,
}
impl Startup {
    /// Root-section startup directives only. Original IFS keeps inline ';'
    /// characters, first matching duplicate keys and leading/trailing trimming.
    /// Full INIFILE services and application/window directives are not yet ported.
    pub fn parse(text: &str) -> Result<Self> {
        ensure!(
            text.len() <= 65536 && !text.contains('\0'),
            "engine.ini byte limit or NUL"
        );
        let mut root = true;
        let mut fields = BTreeMap::new();
        for line in text.split(['\r', '\n']) {
            let line = line.trim_matches([' ', '\t']);
            if line.starts_with('[') {
                root = false;
                continue;
            }
            if !root || line.is_empty() || line.starts_with(';') {
                continue;
            }
            if let Some((name, value)) = line.split_once('=') {
                fields
                    .entry(name.trim_end_matches([' ', '\t']).to_ascii_lowercase())
                    .or_insert_with(|| value.trim_matches([' ', '\t']).to_owned());
            }
        }
        let run = fields
            .remove("run")
            .context("engine.ini has no root 'run' script directive")?;
        ensure!(!run.is_empty(), "engine.ini has empty 'run' directive");
        Ok(Self {
            program_directory: fields.remove("program_directory").unwrap_or_default(),
            run,
        })
    }
    pub fn load(game: &GameDirectory) -> Result<Self> {
        let bytes = game.read("engine.ini", 65536)?;
        if let Some(text) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
            return Self::parse(std::str::from_utf8(text).context("UTF-8 engine.ini BOM")?);
        }
        let (text, errors) = encoding_rs::WINDOWS_1251.decode_without_bom_handling(&bytes);
        ensure!(!errors, "invalid engine.ini code page");
        Self::parse(&text)
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
