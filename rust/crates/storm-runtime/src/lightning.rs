//! Script-owned lightning textures and the retained strike flicker and fade.
use crate::{GameDirectory, NativeHost};
use anyhow::{Context, Result, ensure};
use std::{cell::RefCell, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, Message, Stage};
use storm_formats::texture::Texture;
use storm_render::{model::LightningDraw, texture_paths::TexturePaths};

const ADD: i32 = 54000;
const MAX_STRIKES: usize = 128;

#[derive(Clone, Default)]
pub(crate) struct Lightning(Rc<RefCell<Vec<LightningDraw>>>);

impl Lightning {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let lightning = Self::default();
        let draws = lightning.clone();
        let paths = host.texture_paths.clone();
        if host.entities.register_class("Lightning", move || {
            Ok(Box::new(Actor {
                game: game.clone(),
                paths: paths.clone(),
                draws: draws.clone(),
                bolt: None,
                flash: None,
                tiles: [1; 2],
                strikes: Vec::new(),
            }))
        })? {
            host.lightning = Some(lightning);
        }
        Ok(())
    }

    pub(crate) fn begin_frame(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn draws(&self) -> Vec<LightningDraw> {
        self.0.borrow().clone()
    }
}

struct Actor {
    game: GameDirectory,
    paths: Rc<RefCell<TexturePaths>>,
    draws: Lightning,
    bolt: Option<Rc<Texture>>,
    flash: Option<Rc<Texture>>,
    tiles: [u32; 2],
    strikes: Vec<Strike>,
}

struct Strike {
    tile: u32,
    time: f32,
    flicker: u32,
    position: [f32; 3],
    size: [f32; 2],
    flash_size: f32,
    alpha: f32,
}

impl Strike {
    fn read(message: &mut Message) -> Result<Self> {
        let tile = message.long()? as u32;
        ensure!(
            message.string()?.eq_ignore_ascii_case("lightning"),
            "unsupported Lightning bolt technique"
        );
        let time = message.float()?;
        let flicker = message.long()? as u32;
        let size = message.float()?;
        let scale = [message.float()?, message.float()?];
        let position = [message.float()?, message.float()?, message.float()?];
        ensure!(
            message.string()?.eq_ignore_ascii_case("flash_lightning"),
            "unsupported Lightning flash technique"
        );
        let flash_size = message.float()?;
        let flash_time = message.float()?;
        let size = scale.map(|scale| scale * size);
        ensure!(
            [time, flash_size, flash_time]
                .into_iter()
                .chain(size)
                .chain(position)
                .all(f32::is_finite),
            "invalid Lightning dimensions or timing"
        );
        // The native message carries a separate flash time, but both passes
        // follow the bolt's lifetime and intensity.
        Ok(Self {
            tile,
            time,
            flicker,
            position,
            size,
            flash_size,
            alpha: 0.0,
        })
    }

    fn advance(&mut self, delta: u32) -> bool {
        if self.time > 0.0 {
            let visible = (self.time * 1000.0) as u32 & self.flicker != 0;
            self.alpha = f32::from(u8::from(visible));
            self.time -= delta as f32 * 0.001;
            if self.time <= 0.0 {
                self.alpha = 1.0;
            }
            if self.time == 0.0 {
                return true;
            }
        }
        if self.time <= 0.0 {
            self.alpha -= delta as f32 * 0.01;
        }
        self.alpha >= 0.0
    }
}

impl Entity for Actor {
    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let name = store.name(changed).context("missing Lightning attribute")?;
        let parent = store
            .get(changed)
            .and_then(|attribute| attribute.parent())
            .and_then(|parent| store.name(parent));
        let is_flash = parent.is_some_and(|name| name.eq_ignore_ascii_case("Flash"));
        if name.eq_ignore_ascii_case("Clear") {
            self.strikes.clear();
        } else if name.eq_ignore_ascii_case("Texture") {
            let value = store.value(changed, None).unwrap_or_default();
            let texture = if value.is_empty() {
                None
            } else {
                self.game
                    .load_texture(&self.paths.borrow(), value)?
                    .map(Rc::new)
            };
            if is_flash {
                self.flash = texture;
            } else {
                self.bolt = texture;
            }
        } else if !is_flash && name.eq_ignore_ascii_case("SubTexX") {
            self.tiles[0] = store.as_dword(changed, None, 1);
        } else if !is_flash && name.eq_ignore_ascii_case("SubTexY") {
            self.tiles[1] = store.as_dword(changed, None, 1);
        }
        Ok(0)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if message.long()? == ADD {
            let strike = Strike::read(message)?;
            ensure!(self.strikes.len() < MAX_STRIKES, "Lightning strike limit");
            self.strikes.push(strike);
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta: u32) -> Result<()> {
        match stage {
            Stage::Execute => {
                let mut index = 0;
                while index < self.strikes.len() {
                    if self.strikes[index].advance(delta) {
                        index += 1;
                    } else {
                        self.strikes.swap_remove(index);
                    }
                }
            }
            Stage::Realize if self.bolt.is_some() || self.flash.is_some() => {
                self.draws
                    .0
                    .borrow_mut()
                    .extend(self.strikes.iter().filter_map(|strike| {
                        let intensity = (255.0 * strike.alpha) as u8;
                        (intensity != 0).then(|| LightningDraw {
                            bolt: self.bolt.clone(),
                            flash: self.flash.clone(),
                            position: strike.position,
                            bolt_size: strike.size,
                            flash_size: strike.flash_size,
                            tile: strike.tile,
                            tiles: self.tiles,
                            intensity,
                        })
                    }));
            }
            _ => {}
        }
        Ok(())
    }
}
