use crate::{Engine, GameDirectory, GameSource, NativeHost, ScriptEncoding};
use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;

const GAME_MAIN_INSTRUCTIONS: u64 = 1_000_000;
// The original game builds its complete roster and item tables in Main_Start
// after the logos, before control passes to ordinary frame callbacks.
pub(crate) const GAME_MAIN_START_INSTRUCTIONS: u64 = 32_000_000;

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

pub(crate) fn ini_integer(text: &str) -> Result<i32> {
    // IFS::GetInt uses atoll followed by an int32 cast.
    let text = text.trim_start_matches(|c: char| c.is_ascii_whitespace());
    let sign = usize::from(text.starts_with(['+', '-']));
    let end = sign
        + text.as_bytes()[sign..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
    if end == sign {
        return Ok(0);
    }
    Ok(text[..end]
        .parse::<i64>()
        .context("engine.ini integer overflow")? as i32)
}

pub(crate) fn ini_section(line: &str) -> Option<&str> {
    line.trim_ascii()
        .strip_prefix('[')?
        .split_once(']')
        .map(|(name, _)| name)
}

pub(crate) fn ini_field<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut selected = false;
    for line in text.lines().map(str::trim_ascii) {
        if let Some(name) = ini_section(line) {
            selected = name.eq_ignore_ascii_case(section);
        } else if selected
            && let Some((name, value)) = line.split_once('=')
            && name.trim_ascii().eq_ignore_ascii_case(key)
        {
            return Some(value.trim_ascii());
        }
    }
    None
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
        mut host: NativeHost,
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
        let runtime = storm_script::Runtime::compile_with_loader_and_limits(
            &self.run,
            &mut source,
            storm_script::CompileLimits::game(),
        )
        .with_context(|| format!("compile configured Storm program {}", self.run))?;
        for library in runtime.libraries() {
            host.register_game_library(&library.name, game)?;
        }
        crate::locator::register(&mut host, game.clone())?;
        crate::particles::register(&mut host, game.clone())?;
        crate::sea::Seas::register(&mut host, game.clone())?;
        crate::coast_foam::CoastFoams::register(&mut host, game.clone())?;
        crate::ship_lights::ShipLights::register(&mut host, game.clone())?;
        crate::weather::Weather::register(&mut host)?;
        let ropes = crate::rope::register(&mut host, game.clone())?;
        crate::sail::register(&mut host, game.clone(), ropes)?;
        crate::flag::Flags::register(&mut host, game.clone())?;
        crate::vant::register(&mut host, game.clone())?;
        crate::ship::Ships::register(&mut host)?;
        crate::rain::Rain::register(&mut host)?;
        crate::lightning::Lightning::register(&mut host, game.clone())?;
        crate::sun_glow::SunGlow::register(&mut host, game.clone())?;
        crate::astronomy::Astronomy::register(&mut host, game.clone())?;
        crate::sky::Skies::register(&mut host, game.clone())?;
        crate::teleport::register(&mut host, game.clone())?;
        storm_script::Host::world(&host).use_game_limits();
        let language = crate::script_libraries::interface::LanguageService::new(game.clone());
        crate::interface::Interfaces::register(&mut host, game.clone(), language.clone())?;
        language.register(&mut host)?;
        crate::back_scene::BackScenes::register(&mut host, game.clone(), language.clone())?;
        host.interface_language = Some(language);
        let sound = crate::sound::Sound::register(&mut host, game.clone())?;
        host.configure_script_source(source, game.clone());
        let mut engine =
            Engine::from_runtime(runtime, host).with_startup_instructions(GAME_MAIN_INSTRUCTIONS);
        if let Some(sound) = sound {
            engine.register_service(sound)?;
        }
        Ok(engine)
    }
}
