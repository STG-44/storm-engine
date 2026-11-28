//! Script-owned SUNGLOW settings, texture lifecycle and Realize snapshots.
use crate::{GameDirectory, NativeHost, weather::Weather};
use anyhow::{Context, Result, ensure};
use std::{cell::RefCell, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, EntityInit, Stage};
use storm_formats::texture::Texture;
use storm_render::{
    model::{Flare, FlareDraw, OverflowDraw, SunDraw, SunSprite},
    texture_paths::TexturePaths,
};

#[derive(Default)]
struct TextureSlot {
    name: String,
    candidates: Vec<String>,
    texture: Option<Rc<Texture>>,
}

impl TextureSlot {
    fn refresh(&mut self, game: &GameDirectory, paths: &TexturePaths) -> Result<()> {
        if self.name.is_empty() {
            self.candidates.clear();
            self.texture = None;
            return Ok(());
        }
        let candidates: Vec<_> = paths
            .candidates(&self.name)?
            .into_iter()
            .map(|c| c.file)
            .collect();
        if self.candidates != candidates || self.texture.is_none() {
            self.texture = game.load_texture(paths, &self.name)?.map(Rc::new);
            self.candidates = candidates;
        }
        Ok(())
    }
}

#[derive(Default)]
struct Glow {
    enabled: bool,
    distance: f32,
    sun_size: f32,
    moon_size: f32,
    halo_size: f32,
    color: [u8; 3],
    decay: f32,
    sun: TextureSlot,
    moon: TextureSlot,
    halo: TextureSlot,
}

#[derive(Default)]
struct Overflow {
    enabled: bool,
    size: f32,
    start: f32,
    color: [u8; 3],
    image: TextureSlot,
}

struct Flares {
    distance: f32,
    scale: f32,
    grid: [u32; 2],
    entries: Vec<Flare>,
    atlas: TextureSlot,
}

impl Default for Flares {
    fn default() -> Self {
        Self {
            distance: 0.0,
            scale: 1.0,
            grid: [1; 2],
            entries: Vec::new(),
            atlas: TextureSlot::default(),
        }
    }
}

#[derive(Clone, Default)]
pub(crate) struct SunGlow(Rc<RefCell<Vec<SunDraw>>>);

impl SunGlow {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let sun = Self::default();
        let draws = sun.clone();
        let weather = host.weather.clone();
        let paths = host.texture_paths.clone();
        if host.entities.register_class("SunGlow", move || {
            Ok(Box::new(Actor {
                draws: draws.clone(),
                weather: weather.clone(),
                game: game.clone(),
                paths: paths.clone(),
                moon: false,
                glow: Glow::default(),
                overflow: Overflow::default(),
                flares: Flares::default(),
                visible: false,
                alpha: [0.0; 3],
            }))
        })? {
            host.sun_glow = Some(sun);
        }
        Ok(())
    }

    pub(crate) fn clear_draws(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn draws(&self) -> Vec<SunDraw> {
        self.0.borrow().clone()
    }
}

struct Actor {
    draws: SunGlow,
    weather: Option<Weather>,
    game: GameDirectory,
    paths: Rc<RefCell<TexturePaths>>,
    moon: bool,
    glow: Glow,
    overflow: Overflow,
    flares: Flares,
    visible: bool,
    alpha: [f32; 3],
}

impl Actor {
    fn refresh(&mut self) -> Result<()> {
        let paths = self.paths.borrow();
        for slot in [
            &mut self.glow.sun,
            &mut self.glow.moon,
            &mut self.glow.halo,
            &mut self.overflow.image,
            &mut self.flares.atlas,
        ] {
            slot.refresh(&self.game, &paths)?;
        }
        Ok(())
    }

    fn draw(&self, direction: [f32; 3], delta_ms: u32) -> SunDraw {
        let glow = &self.glow;
        let (body, size) = if self.moon {
            (&glow.moon, glow.moon_size)
        } else {
            (&glow.sun, glow.sun_size)
        };
        let disc = body
            .texture
            .as_ref()
            .filter(|_| glow.enabled)
            .map(|texture| SunSprite {
                texture: texture.clone(),
                size,
                color: glow.color,
            });
        let halo = glow
            .halo
            .texture
            .as_ref()
            .filter(|_| glow.enabled && !self.moon)
            .map(|texture| SunSprite {
                texture: texture.clone(),
                size: glow.halo_size,
                color: glow.color,
            });
        let overflow = &self.overflow;
        let overflow = overflow
            .image
            .texture
            .as_ref()
            .filter(|_| overflow.enabled)
            .map(|texture| OverflowDraw {
                sprite: SunSprite {
                    texture: texture.clone(),
                    size: overflow.size,
                    color: overflow.color,
                },
                start: overflow.start,
                alpha: self.alpha[2],
            });
        let flares = &self.flares;
        let flares = flares
            .atlas
            .texture
            .as_ref()
            .filter(|_| delta_ms != 0 && !flares.entries.is_empty())
            .map(|atlas| FlareDraw {
                atlas: atlas.clone(),
                entries: flares.entries.clone(),
                distance: flares.distance,
                scale: flares.scale,
                grid: flares.grid,
                alpha: self.alpha[0] * self.alpha[1],
            });
        SunDraw {
            direction,
            distance: glow.distance,
            disc,
            halo,
            halo_alpha: self.alpha[0],
            overflow,
            flares,
        }
    }
}

impl Entity for Actor {
    fn init(&mut self, _: &mut EntityInit<'_>) -> Result<bool> {
        ensure!(
            self.weather
                .as_ref()
                .and_then(Weather::first_state)
                .is_some(),
            "SUNGLOW requires a Weather entity"
        );
        Ok(true)
    }

    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let name = store
            .name(changed)
            .context("missing SUNGLOW attribute")?
            .to_ascii_lowercase();
        let parent = store
            .get(changed)
            .and_then(|attr| attr.parent())
            .and_then(|id| store.name(id))
            .map(str::to_ascii_lowercase);
        let value = store.value(changed, None).unwrap_or_default();
        let float = || -> Result<f32> {
            let value = store.as_float(changed, None, 0.0);
            ensure!(value.is_finite(), "non-finite SUNGLOW setting {name}");
            Ok(value)
        };
        let dword = || store.as_dword(changed, None, 0);
        let color = || {
            let [_, r, g, b] = dword().to_be_bytes();
            [r, g, b]
        };
        match name.as_str() {
            "isdone" => self.refresh()?,
            "clear" => {
                self.glow.enabled = false;
                self.overflow.enabled = false;
                self.flares.entries.clear();
            }
            "moon" => self.moon = dword() != 0,
            _ => match parent.as_deref() {
                Some("glow") => {
                    let glow = &mut self.glow;
                    glow.enabled = true;
                    match name.as_str() {
                        "dist" => glow.distance = float()?,
                        "sunsize" => glow.sun_size = float()?,
                        "moonsize" => glow.moon_size = float()?,
                        "size" => glow.halo_size = float()?,
                        "decaytime" => glow.decay = float()?,
                        "color" => glow.color = color(),
                        "suntexture" => glow.sun.name = value.into(),
                        "moontexture" => glow.moon.name = value.into(),
                        "glowtexture" => glow.halo.name = value.into(),
                        _ => {}
                    }
                }
                Some("overflow") => {
                    let overflow = &mut self.overflow;
                    overflow.enabled = true;
                    match name.as_str() {
                        "size" => overflow.size = float()?,
                        "start" => overflow.start = float()?,
                        "color" => overflow.color = color(),
                        "texture" => overflow.image.name = value.into(),
                        _ => {}
                    }
                }
                Some("flare") => {
                    let flares = &mut self.flares;
                    match name.as_str() {
                        "dist" => flares.distance = float()?,
                        "scale" => flares.scale = float()?,
                        "texsizex" => flares.grid[0] = dword().max(1),
                        "texsizey" => flares.grid[1] = dword().max(1),
                        "texture" => flares.atlas.name = value.into(),
                        _ => {}
                    }
                }
                Some("flares") if !value.is_empty() => {
                    ensure!(self.flares.entries.len() < 128, "SUNGLOW flare count limit");
                    self.flares.entries.push(parse_flare(value)?);
                }
                _ => {}
            },
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage == Stage::Execute {
            let delta = if self.visible {
                delta_ms as f32
            } else {
                -(delta_ms as f32)
            };
            let enabled = [
                self.glow.enabled,
                !self.flares.entries.is_empty(),
                self.overflow.enabled,
            ];
            let rates = [self.glow.decay * 0.001, 0.008, 0.008];
            for ((alpha, enabled), rate) in self.alpha.iter_mut().zip(enabled).zip(rates) {
                if enabled {
                    *alpha = (*alpha + delta * rate).clamp(0.0, 1.0);
                }
            }
        } else if stage == Stage::Realize {
            // Open water visibility. World collision and sky opacity will
            // supply occlusion when their original layers are ported.
            let weather = self.weather.as_ref().and_then(Weather::first_state);
            self.visible = weather.is_some();
            if let Some(weather) = weather {
                let draw = self.draw(weather.light_position, delta_ms);
                if draw.disc.is_some()
                    || draw.halo.is_some()
                    || draw.overflow.is_some()
                    || draw.flares.is_some()
                {
                    self.draws.0.borrow_mut().push(draw);
                }
            }
        }
        Ok(())
    }
}

fn parse_flare(value: &str) -> Result<Flare> {
    let fields: Vec<_> = value.split(',').map(str::trim).collect();
    let [distance, size, tile, rgb] = fields.as_slice() else {
        anyhow::bail!("invalid SUNGLOW flare record");
    };
    let (distance, size): (f32, f32) = (distance.parse()?, size.parse()?);
    ensure!(
        distance.is_finite() && size.is_finite(),
        "non-finite SUNGLOW flare"
    );
    let rgb = rgb
        .strip_prefix("0x")
        .or_else(|| rgb.strip_prefix("0X"))
        .unwrap_or(rgb);
    let [_, r, g, b] = u32::from_str_radix(rgb, 16)?.to_be_bytes();
    Ok(Flare {
        distance,
        size,
        tile: tile.parse()?,
        color: [r, g, b],
    })
}
