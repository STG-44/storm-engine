//! Native SOUND messages and desktop stereo playback.
mod catalog;
#[cfg(test)]
mod tests;

use crate::{GameDirectory, NativeHost, Section, Service, ServiceFrame, random::Random};
use anyhow::{Context, Result, ensure};
use rodio::{Decoder, DeviceSinkBuilder, MixerDeviceSink, Player, mixer::Mixer};
use std::{cell::RefCell, fs::File, io::BufReader, rc::Rc};
use storm_core::{Entity, EntityInit, Message, ScriptScalar};

const REALIZE: u8 = 1;
const PLAY: i32 = 77003;
const STOP: i32 = 77004;
const RELEASE: i32 = 77005;
const SET_VOLUME: i32 = 77008;
const IS_PLAYING: i32 = 77009;
const GET_POSITION: i32 = 77010;
const RESTART: i32 = 77011;
const RESUME: i32 = 77012;
const SCHEME_RESET: i32 = 77013;
const SCHEME_SET: i32 = 77014;
const SCHEME_ADD: i32 = 77015;
const ALIAS_ADD: i32 = 77017;
const SET_MASTER_VOLUME: i32 = 77021;
const GET_MASTER_VOLUME: i32 = 77022;
const STREAMED_STEREO: i32 = 2;
const PCM_STEREO: i32 = 4;
const MAX_VOICES: usize = 150;

#[derive(Clone)]
pub(crate) struct Sound(Rc<RefCell<State>>);

impl Sound {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<Option<Self>> {
        let sound = Self(Rc::new(RefCell::new(State::new(game, host.random.clone()))));
        let actor = sound.clone();
        let registered = host
            .entities
            .register_class("Sound", move || Ok(Box::new(actor.clone())))?;
        Ok(registered.then_some(sound))
    }
}

impl Entity for Sound {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        context
            .manager
            .add_to_layer(REALIZE, context.id, u32::MAX)?;
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let mut state = self.0.borrow_mut();
        match message.long()? {
            PLAY => {
                let name = message.string()?.to_owned();
                let kind = message.long()?;
                let category = optional_long(message)?.unwrap_or(1);
                let paused = optional_long(message)?.unwrap_or(0) != 0;
                let looped = optional_long(message)?.unwrap_or(0) != 0;
                let _cached = optional_long(message)?;
                let fade_ms = optional_long(message)?.unwrap_or(0).max(0) as u32;
                let mut volume = 1.0;
                if kind == STREAMED_STEREO && looped {
                    let _loop_pause = optional_long(message)?;
                    if message.param_valid() {
                        volume = message.float()?;
                    }
                }
                Ok(u64::from(state.play(
                    &name,
                    Playback {
                        kind,
                        category,
                        paused,
                        looped,
                        fade_ms,
                        volume,
                    },
                )))
            }
            STOP => {
                let id = message.long()? as u32;
                let fade_ms = message.long()?.max(0) as u32;
                state.stop(id, fade_ms);
                Ok(0)
            }
            RESUME => {
                let id = message.long()?;
                let _fade = message.long()?;
                for voice in state.voices.iter().flatten() {
                    if matches!(id, -1..=1) || voice.id == id as u32 {
                        voice.player.play();
                    }
                }
                Ok(0)
            }
            IS_PLAYING => {
                let id = message.long()? as u32;
                Ok(u64::from(
                    state
                        .voices
                        .iter()
                        .flatten()
                        .any(|voice| voice.id == id && !voice.player.empty()),
                ))
            }
            SET_VOLUME => {
                let id = message.long()? as u32;
                let volume = message.float()?;
                let master = state.master;
                for voice in state.voices.iter_mut().flatten() {
                    if id == 0 || voice.id == id {
                        voice.volume = volume;
                        voice.update_volume(master);
                    }
                }
                Ok(0)
            }
            SET_MASTER_VOLUME => {
                let values = [message.float()?, message.float()?, message.float()?]
                    .map(|value| value.clamp(0.0, 1.0));
                state.master = values;
                for voice in state.voices.iter().flatten() {
                    voice.update_volume(values);
                }
                Ok(0)
            }
            GET_MASTER_VOLUME => {
                for value in state.master {
                    message.script_variable()?.set(ScriptScalar::Float(value))?;
                }
                Ok(0)
            }
            RELEASE | RESTART | GET_POSITION => {
                // Retained C++ methods consume the ID and leave the sound alone.
                message.long()?;
                Ok(0)
            }
            SCHEME_RESET => {
                state.scheme.clear();
                Ok(0)
            }
            code @ (SCHEME_SET | SCHEME_ADD) => {
                let name = message.string()?;
                state.load_scheme(name, code == SCHEME_SET)?;
                Ok(0)
            }
            ALIAS_ADD => {
                let name = message.string()?;
                state.load_aliases(name)?;
                Ok(0)
            }
            _ => Ok(0),
        }
    }
}

impl Service for Sound {
    fn section(&self) -> Section {
        Section::Execute
    }

    fn run_start(&mut self, frame: &mut ServiceFrame) -> Result<()> {
        self.0.borrow_mut().advance(frame.delta_ms);
        Ok(())
    }

    fn run_end(&mut self, _: &mut ServiceFrame) -> Result<()> {
        Ok(()) // The desktop mixer is advanced by the device callback.
    }
}

enum Output {
    Unopened,
    Device(MixerDeviceSink),
    Unavailable,
    #[cfg(test)]
    Test(Mixer),
}

impl Output {
    fn mixer(&mut self) -> Option<&Mixer> {
        if matches!(self, Self::Unopened) {
            *self = match DeviceSinkBuilder::open_default_sink() {
                Ok(mut sink) => {
                    sink.log_on_drop(false);
                    Self::Device(sink)
                }
                Err(error) => {
                    eprintln!("Sound output unavailable: {error}");
                    Self::Unavailable
                }
            };
        }
        match self {
            Self::Device(sink) => Some(sink.mixer()),
            #[cfg(test)]
            Self::Test(mixer) => Some(mixer),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
struct Fade {
    start: f32,
    end: f32,
    elapsed_ms: u32,
    duration_ms: u32,
}

struct Voice {
    id: u32,
    player: Player,
    volume: f32,
    category: i32,
    gain: f32,
    fade: Option<Fade>,
}

impl Voice {
    fn update_volume(&self, master: [f32; 3]) {
        let base = match self.category {
            1..=3 => master[(self.category - 1) as usize],
            _ => 1.0,
        };
        self.player
            .set_volume((self.volume * self.gain * base).max(0.0));
    }

    fn start_fade(&mut self, target: f32, duration_ms: u32) {
        self.fade = Some(Fade {
            start: self.gain,
            end: target,
            elapsed_ms: 0,
            duration_ms,
        });
    }

    fn advance(&mut self, delta_ms: u32, master: [f32; 3]) -> bool {
        if self.player.empty() {
            return false;
        }
        if let Some(fade) = &mut self.fade {
            fade.elapsed_ms = fade
                .elapsed_ms
                .saturating_add(delta_ms)
                .min(fade.duration_ms);
            let t = fade.elapsed_ms as f32 / fade.duration_ms as f32;
            self.gain = fade.start + t * (fade.end - fade.start);
            let stop = t == 1.0 && fade.end == 0.0;
            if t == 1.0 {
                self.fade = None;
            }
            self.update_volume(master);
            return !stop;
        }
        true
    }
}

struct Playback {
    kind: i32,
    category: i32,
    paused: bool,
    looped: bool,
    fade_ms: u32,
    volume: f32,
}

struct State {
    game: GameDirectory,
    rng: Rc<RefCell<Random>>,
    output: Output,
    voices: Vec<Option<Voice>>,
    master: [f32; 3],
    next_music: usize,
    stamp: u16,
    aliases: Option<catalog::Aliases>,
    scheme: Vec<catalog::Channel>,
}

impl State {
    fn new(game: GameDirectory, rng: Rc<RefCell<Random>>) -> Self {
        Self {
            game,
            rng,
            output: Output::Unopened,
            voices: (0..MAX_VOICES).map(|_| None).collect(),
            master: [0.5; 3],
            next_music: 0,
            stamp: 0,
            aliases: None,
            scheme: Vec::new(),
        }
    }

    fn load_aliases(&mut self, name: &str) -> Result<()> {
        let aliases = match &mut self.aliases {
            Some(aliases) => aliases,
            None => self.aliases.insert(catalog::default_aliases(&self.game)?),
        };
        catalog::load_aliases(&self.game, aliases, name)
    }

    fn load_scheme(&mut self, name: &str, replace: bool) -> Result<()> {
        let channels = catalog::scheme(&self.game, name, &mut self.rng.borrow_mut())?;
        let existing = if replace { 0 } else { self.scheme.len() };
        ensure!(
            existing + channels.len() <= catalog::MAX_SCHEME_CHANNELS,
            "sound scheme channel limit"
        );
        if replace {
            self.scheme.clear();
        }
        self.scheme.extend(channels);
        Ok(())
    }

    fn play(&mut self, name: &str, args: Playback) -> u32 {
        match self.try_play(name, args) {
            Ok(id) => id,
            Err(error) => {
                eprintln!("Sound {name:?}: {error:#}");
                0
            }
        }
    }

    fn try_play(&mut self, name: &str, args: Playback) -> Result<u32> {
        let slot = match args.kind {
            STREAMED_STEREO => self.next_music,
            PCM_STEREO => {
                let Some(slot) = (2..self.voices.len()).find(|&i| self.voices[i].is_none()) else {
                    return Ok(0);
                };
                slot
            }
            _ => return Ok(0),
        };
        if name.is_empty() || matches!(self.output, Output::Unavailable) {
            return Ok(0);
        }
        let (name, volume) = if name.contains(['/', '\\']) {
            (name, args.volume)
        } else {
            let aliases = match &self.aliases {
                Some(aliases) => aliases,
                None => self.aliases.insert(catalog::default_aliases(&self.game)?),
            };
            aliases
                .get(name)
                .and_then(|alias| alias.choose(args.volume, &mut self.rng.borrow_mut()))
                .unwrap_or((name, args.volume))
        };
        let Some(path) = self
            .game
            .resolve_optional(&format!("RESOURCE/Sounds/{name}"))?
        else {
            return Ok(0);
        };
        ensure!(path.is_file(), "sound resource is not a regular file");
        let file = File::open(path).context("open sound resource")?;
        let Some(mixer) = self.output.mixer() else {
            return Ok(0);
        };
        let player = Player::connect_new(mixer);
        player.pause();
        if args.looped {
            player
                .append(Decoder::new_looped(BufReader::new(file)).context("decode looped sound")?);
        } else {
            player.append(Decoder::try_from(file).context("decode sound")?);
        }
        let mut id = slot as u32 + 1;
        if slot < 2 {
            self.next_music ^= 1;
            self.stop(self.next_music as u32 + 1, args.fade_ms);
        } else {
            self.stamp = self.stamp.wrapping_add(1);
            id |= u32::from(self.stamp) << 16;
        }
        let mut voice = Voice {
            id,
            player,
            volume,
            category: args.category,
            gain: 1.0,
            fade: None,
        };
        if args.fade_ms > 0 {
            voice.gain = 0.0;
            voice.start_fade(1.0, args.fade_ms);
        }
        voice.update_volume(self.master);
        if !args.paused {
            voice.player.play();
        }
        self.voices[slot] = Some(voice);
        Ok(id)
    }

    fn stop(&mut self, id: u32, fade_ms: u32) {
        for (slot, channel) in self.voices.iter_mut().enumerate() {
            if let Some(voice) = channel
                && (id == 0 || voice.id == id)
            {
                if fade_ms == 0 || (id == 0 && slot >= 2) {
                    *channel = None;
                } else {
                    voice.start_fade(0.0, fade_ms);
                }
            }
        }
    }

    fn advance(&mut self, delta_ms: u32) {
        for voice in &mut self.voices {
            if let Some(channel) = voice
                && !channel.advance(delta_ms, self.master)
            {
                *voice = None;
            }
        }
        for index in 0..self.scheme.len() {
            let channel = &mut self.scheme[index];
            if !channel.advance(delta_ms, &mut self.rng.borrow_mut()) {
                continue;
            }
            let name = channel.name.clone();
            let args = Playback {
                kind: PCM_STEREO,
                category: 1,
                paused: false,
                looped: channel.looped,
                fade_ms: 0,
                volume: channel.volume,
            };
            self.play(&name, args);
        }
    }
}

fn optional_long(message: &mut Message) -> Result<Option<i32>> {
    message.param_valid().then(|| message.long()).transpose()
}
