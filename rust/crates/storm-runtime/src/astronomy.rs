//! Original Astronomy catalog, planetary sprites and weather-driven fades.
use crate::{GameDirectory, NativeHost, random::Random, weather::Weather};
use anyhow::{Context, Result, ensure};
use std::{
    cell::RefCell,
    f32::consts::{FRAC_PI_2, PI},
    rc::Rc,
};
use storm_core::{AttributeId, AttributeStore, Entity, EntityInit, Stage};
use storm_formats::{
    astronomy::{MAX_CATALOG_BYTES, Star, read_catalog},
    texture::Texture,
};
use storm_render::{
    model::{CelestialDraw, CelestialSprite},
    texture_paths::TexturePaths,
};

#[derive(Clone, Default)]
pub(crate) struct Astronomy(Rc<RefCell<Vec<CelestialDraw>>>);

impl Astronomy {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let astronomy = Self::default();
        let draws = astronomy.clone();
        let weather = host.weather.clone();
        let random = host.random.clone();
        let paths = host.texture_paths.clone();
        if host.entities.register_class("Astronomy", move || {
            Ok(Box::new(Actor {
                draws: draws.clone(),
                game: game.clone(),
                weather: weather.clone(),
                random: random.clone(),
                paths: paths.clone(),
                root: None,
                stars: None,
                planets: Planets::default(),
                twinkle_time: 0.0,
            }))
        })? {
            host.astronomy = Some(astronomy);
        }
        Ok(())
    }

    pub(crate) fn begin_frame(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn draws(&self) -> Vec<CelestialDraw> {
        self.0.borrow().clone()
    }
}

#[derive(Clone, Copy)]
struct Fade {
    value: f32,
    start: f32,
    duration: f32,
}

impl Fade {
    fn read(group: Settings<'_>, duration: f32) -> Result<Self> {
        Ok(Self {
            value: group.float("FadeValue", 1.0)?.clamp(0.0, 1.0),
            start: group.float("FadeStartTime", -1.0)?,
            duration: group.float("FadeTime", duration)?,
        })
    }

    fn advance(&mut self, hour: f32) {
        if self.start < 0.0 {
            return;
        }
        let elapsed = hour - self.start;
        if self.duration > 0.0 && self.value < 1.0 {
            self.value = (elapsed / self.duration).clamp(0.0, 1.0);
        } else if self.duration < 0.0 && self.value > 0.0 {
            self.value = (1.0 + elapsed / self.duration).clamp(0.0, 1.0);
        }
    }
}

#[derive(Clone, Copy)]
struct Settings<'a> {
    store: &'a AttributeStore,
    root: Option<AttributeId>,
}

impl<'a> Settings<'a> {
    fn float(self, name: &str, default: f32) -> Result<f32> {
        let value = self.root.map_or(default, |root| {
            self.store.as_float(root, Some(name), default)
        });
        ensure!(value.is_finite(), "non-finite Astronomy attribute {name}");
        Ok(value)
    }

    fn string(self, name: &str) -> Option<&'a str> {
        self.store
            .value(self.root?, Some(name))
            .filter(|value| !value.is_empty())
    }

    fn child(self, name: &str) -> Self {
        Self {
            store: self.store,
            root: self.root.and_then(|root| self.store.child(root, name)),
        }
    }

    fn children(self) -> impl Iterator<Item = AttributeId> {
        self.root
            .and_then(|root| self.store.get(root))
            .into_iter()
            .flat_map(|root| root.children().iter().copied())
    }
}

struct StarSettings {
    enabled: bool,
    radius: f32,
    size: f32,
    height_fade: f32,
    fade: Fade,
}

impl StarSettings {
    fn read(group: Settings<'_>) -> Result<Self> {
        let enabled = group
            .root
            .is_some_and(|root| group.store.as_dword(root, Some("Enable"), 0) != 0);
        Ok(Self {
            enabled,
            radius: group.float("Radius", 2000.0)?,
            size: group.float("Size", 20.0)?,
            height_fade: group.float("HeightFade", 100.0)?,
            fade: Fade::read(group, 0.2)?,
        })
    }
}

struct StarPoint {
    direction: [f32; 3],
    magnitude: f32,
    rgb: [u8; 3],
}

impl StarPoint {
    fn new(star: Star, spectrum: &[[u8; 3]; 256]) -> Self {
        let [ra, dec] = star.angles;
        Self {
            direction: [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()],
            magnitude: star.magnitude,
            rgb: spectrum[star.spectrum[0].to_ascii_uppercase() as usize],
        }
    }
}

struct Stars {
    settings: StarSettings,
    points: Vec<StarPoint>,
    texture: Rc<Texture>,
    magnitudes: [f32; 2],
    sprites: Option<Rc<[CelestialSprite]>>,
}

impl Stars {
    fn load(
        group: Settings<'_>,
        game: &GameDirectory,
        paths: &TexturePaths,
    ) -> Result<Option<Self>> {
        let (Some(catalog), Some(texture_name)) =
            (group.string("Catalog"), group.string("Texture"))
        else {
            return Ok(None);
        };
        let Some(bytes) = catalog_bytes(game, catalog)? else {
            return Ok(None);
        };
        let points =
            read_catalog(&bytes).with_context(|| format!("decode Astronomy catalog {catalog}"))?;
        let Some(texture) = game.load_texture(paths, texture_name)? else {
            return Ok(None);
        };
        let mut spectrum = [[0; 3]; 256];
        for id in group.child("Spectr").children() {
            if let Some(first) = group.store.name(id).and_then(|name| name.bytes().next()) {
                let [_, r, g, b] = group.store.as_dword(id, None, 0).to_be_bytes();
                spectrum[first.to_ascii_uppercase() as usize] = [r, g, b];
            }
        }
        Ok(Some(Self {
            settings: StarSettings::read(group)?,
            points: points
                .into_iter()
                .map(|point| StarPoint::new(point, &spectrum))
                .collect(),
            texture: Rc::new(texture),
            magnitudes: [
                group.float("TelescopeMagnitude", 13.0)?,
                group.float("VisualMagnitude", 8.5)?,
            ],
            sprites: None,
        }))
    }

    fn realize(
        &mut self,
        hour: Option<f32>,
        delta_ms: u32,
        twinkle_time: &mut f32,
        random: &mut Random,
    ) -> Option<CelestialDraw> {
        let settings = &mut self.settings;
        if let Some(hour) = hour.filter(|hour| *hour > settings.fade.start) {
            let old = (20.0 * settings.fade.value) as i32;
            let previous = settings.fade.value;
            settings.fade.advance(hour);
            if previous != settings.fade.value {
                settings.enabled = settings.fade.value > 0.0;
            }
            if old != (20.0 * settings.fade.value) as i32 {
                self.sprites = None;
            }
        }
        if !settings.enabled || settings.radius <= 0.0 || settings.size <= 0.0 {
            return None;
        }
        let sprites = self.sprites.get_or_insert_with(|| {
            *twinkle_time += delta_ms as f32 * 0.001 * (0.8 + random.scaled(0.2));
            const PERIOD: f32 = PI * 2.0 * 3.0 * 5.0 * 7.0;
            if *twinkle_time > PERIOD {
                *twinkle_time -= PERIOD;
            }
            let factors = [
                0.7 + 0.3 * (*twinkle_time * 0.5).sin(),
                0.75 + 0.25 * (*twinkle_time * 3.0).sin(),
                0.75 + 0.25 * (*twinkle_time * 2.0).sin(),
                0.8 + 0.2 * (*twinkle_time * 5.0).sin(),
                0.85 + 0.15 * (*twinkle_time * 7.0).sin(),
            ];
            let noise: [f32; 7] = std::array::from_fn(|_| 0.8 + random.scaled(0.2));
            self.points
                .iter()
                .enumerate()
                .filter_map(|(index, star)| {
                    let position = star.direction.map(|component| component * settings.radius);
                    let height = horizon_brightness(position[1], settings.height_fade);
                    let opacity =
                        settings.fade.value * factors[index % 5] * noise[index % 7] * height;
                    (opacity > 0.0).then(|| {
                        CelestialSprite::new(
                            position,
                            settings.size,
                            star.rgb,
                            opacity,
                            star.magnitude,
                        )
                    })
                })
                .collect()
        });
        (!sprites.is_empty()).then(|| CelestialDraw {
            texture: self.texture.clone(),
            sprites: sprites.clone(),
            magnitudes: Some(self.magnitudes),
        })
    }
}

fn horizon_brightness(height: f32, fade_height: f32) -> f32 {
    if height >= fade_height {
        1.0
    } else if fade_height > 0.0 {
        (height / fade_height).clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn catalog_bytes(game: &GameDirectory, name: &str) -> Result<Option<Vec<u8>>> {
    let bytes = game.read_optional(name, MAX_CATALOG_BYTES)?;
    if bytes.is_some() || name.contains(['/', '\\']) {
        Ok(bytes)
    } else {
        game.read_optional(&format!("RESOURCE/{name}"), MAX_CATALOG_BYTES)
    }
}

#[derive(Default)]
struct Planets {
    list: Vec<Planet>,
    settings: Option<(f32, Fade)>,
}

struct Planet {
    position: [f32; 3],
    size: f32,
    texture: Option<Rc<Texture>>,
    cached: Option<(f32, u8, Rc<[CelestialSprite]>)>,
}

impl Planets {
    fn load(
        group: Settings<'_>,
        game: &GameDirectory,
        paths: &TexturePaths,
        random: &mut Random,
    ) -> Result<Self> {
        let mut initial = Vec::new();
        for root in group.child("Planet").children() {
            ensure!(initial.len() < 32, "Astronomy planet count exceeds budget");
            let name = group.store.name(root).context("unnamed Astronomy planet")?;
            ensure!(!name.contains(['/', '\\']), "invalid Astronomy planet name");
            let planet = Settings {
                store: group.store,
                root: Some(root),
            };
            let distance = planet.float("Distance", 0.0)?;
            let diameter = planet.float("Diameter", 0.0)?;
            let scale = planet.float("Scale", 1.0)?;
            let angle = FRAC_PI_2 + random.scaled(PI);
            let texture = game
                .load_texture(paths, &format!("weather/astronomy/planets/{name}.tga"))?
                .map(Rc::new);
            initial.push((distance, diameter, scale, angle, texture));
        }
        let furthest = initial
            .iter()
            .map(|planet| planet.0)
            .fold(1e-10_f32, f32::max);
        let (sin_pitch, cos_pitch) = (PI / 8.0).sin_cos();
        let list = initial
            .into_iter()
            .map(|(distance, diameter, scale, angle, texture)| {
                let sky_distance = 1200.0 + 500.0 * distance / furthest;
                let (sin, cos) = angle.sin_cos();
                let z = cos * sky_distance;
                let position = [sin * sky_distance, -z * sin_pitch, z * cos_pitch];
                let base_size = ((sky_distance * diameter) as f64
                    / ((distance as f64 - 1.0).abs() * 150_000_000.0))
                    as f32;
                Planet {
                    position,
                    size: base_size * scale,
                    texture,
                    cached: None,
                }
            })
            .collect();
        Ok(Self {
            list,
            settings: Some(Self::settings(group)?),
        })
    }

    fn settings(group: Settings<'_>) -> Result<(f32, Fade)> {
        Ok((group.float("Scale", 1.0)?, Fade::read(group, 0.1)?))
    }

    fn realize(&mut self, hour: Option<f32>, draws: &mut Vec<CelestialDraw>) {
        let Some((scale, fade)) = &mut self.settings else {
            return;
        };
        if let Some(hour) = hour {
            fade.advance(hour);
        }
        let opacity = (fade.value * 255.0) as u8;
        if opacity == 0 {
            return;
        }
        for planet in &mut self.list {
            let size = planet.size * *scale * 10.0;
            let Some(texture) = planet
                .texture
                .as_ref()
                .filter(|_| size.is_finite() && size > 0.0)
            else {
                continue;
            };
            let cached = planet.cached.get_or_insert_with(|| (0.0, 0, Rc::from([])));
            if (cached.0, cached.1) != (size, opacity) {
                let sprite = CelestialSprite::new(
                    planet.position,
                    size,
                    [255; 3],
                    opacity as f32 / 255.0,
                    0.0,
                );
                *cached = (size, opacity, Rc::from([sprite]));
            }
            draws.push(CelestialDraw {
                texture: texture.clone(),
                sprites: cached.2.clone(),
                magnitudes: None,
            });
        }
    }
}

struct Actor {
    draws: Astronomy,
    game: GameDirectory,
    paths: Rc<RefCell<TexturePaths>>,
    weather: Option<Weather>,
    random: Rc<RefCell<Random>>,
    root: Option<AttributeId>,
    stars: Option<Stars>,
    planets: Planets,
    twinkle_time: f32,
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.root = context.attributes;
        Ok(true)
    }

    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let root = Settings {
            store,
            root: self.root,
        };
        let name = store
            .name(changed)
            .context("missing Astronomy attribute name")?;
        if name.eq_ignore_ascii_case("isDone") {
            let paths = self.paths.borrow();
            let stars = Stars::load(root.child("Stars"), &self.game, &paths)?;
            let planets = Planets::load(
                root.child("Planets"),
                &self.game,
                &paths,
                &mut self.random.borrow_mut(),
            )?;
            self.stars = stars;
            self.planets = planets;
        } else if name.eq_ignore_ascii_case("TimeUpdate") {
            if let Some(stars) = &mut self.stars {
                stars.settings = StarSettings::read(root.child("Stars"))?;
                stars.sprites = None;
            }
            self.planets.settings = Some(Planets::settings(root.child("Planets"))?);
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage == Stage::Realize {
            let hour = self
                .weather
                .as_ref()
                .and_then(Weather::first_state)
                .map(|weather| weather.hour);
            let mut draws = self.draws.0.borrow_mut();
            if let Some(stars) = &mut self.stars
                && let Some(draw) = stars.realize(
                    hour,
                    delta_ms,
                    &mut self.twinkle_time,
                    &mut self.random.borrow_mut(),
                )
            {
                draws.push(draw);
            }
            self.planets.realize(hour, &mut draws);
        }
        Ok(())
    }
}
