//! Retained RAIN streak generation and Realize scheduling.
use crate::{NativeHost, random::Random, weather::Weather};
use anyhow::{Context, Result, ensure};
use std::{
    cell::RefCell,
    f32::consts::{PI, TAU},
    rc::Rc,
    sync::Arc,
};
use storm_core::{AttributeId, AttributeStore, Entity, Message, Stage};
use storm_render::model::{RainBlock, RainDraw};

// The original heavy-storm presets request up to 50,000 drops.
const MAX_DROPS: u32 = 65_536;

#[derive(Default)]
struct Settings {
    count: u32,
    argb: u32,
    length: f32,
    height: f32,
    radius: f32,
    speed: f32,
    jitter: f32,
    wind_jitter: f32,
    blend_ms: u32,
    max_blend: u32,
}

struct Block {
    origin: [f32; 3],
    yaw: f32,
    wind_jitter: f32,
    age_ms: u32,
}

struct Streaks {
    starts: Arc<[[f32; 3]]>,
    length: f32,
    rgb: [u8; 3],
    blocks: [Block; 4],
    wind_angle: f32,
    wind_power: f32,
}

impl Streaks {
    fn new(
        settings: &Settings,
        (wind_angle, wind_speed): (f32, f32),
        random: &mut Random,
    ) -> Result<Self> {
        ensure!(
            settings.count <= MAX_DROPS,
            "RAIN drop count exceeds {MAX_DROPS}"
        );
        ensure!(
            settings.height > 0.0 && settings.radius >= 0.0 && settings.length >= 0.0,
            "invalid RAIN dimensions"
        );
        let wind_power = wind_speed / 18.0;
        let [x, z] = upwind(wind_angle, wind_power);
        let half = settings.height * 0.5;
        let blocks = [0.0, settings.height, half, settings.height + half].map(|y| Block {
            origin: [x, y, z],
            yaw: random.scaled(TAU),
            wind_jitter: random.scaled(settings.wind_jitter) - settings.wind_jitter * 0.5,
            age_ms: settings.blend_ms,
        });
        let starts = (0..settings.count)
            .map(|_| {
                let radius = 1.5 + random.scaled(settings.radius);
                let angle = random.scaled(TAU);
                let y = settings.height - random.scaled(settings.height * 2.0);
                [radius * angle.cos(), y, radius * angle.sin()]
            })
            .collect();
        let [_, r, g, b] = settings.argb.to_be_bytes();
        Ok(Self {
            starts,
            length: settings.length,
            rgb: [r, g, b],
            blocks,
            wind_angle,
            wind_power,
        })
    }

    fn draw(&self, settings: &Settings) -> RainDraw {
        let blocks = self.blocks.each_ref().map(|block| {
            let alpha = if settings.blend_ms == 0 {
                255
            } else {
                (255.0 * block.age_ms as f32 / settings.blend_ms as f32) as u32
            };
            RainBlock {
                origin: block.origin,
                yaw: block.yaw,
                tilt: self.wind_power + block.wind_jitter,
                wind_yaw: PI + self.wind_angle,
                alpha: alpha.min(settings.max_blend).min(255) as u8,
            }
        });
        RainDraw {
            starts: self.starts.clone(),
            length: self.length,
            rgb: self.rgb,
            blocks,
        }
    }

    fn advance(&mut self, settings: &Settings, delta: u32, random: &mut Random) {
        let wind_speed = self.wind_power * 0.2;
        let [x, z] = upwind(self.wind_angle, self.wind_power);
        for block in &mut self.blocks {
            block.origin[1] -= settings.speed * 0.2 * delta as f32 / 60.0;
            // The original updates the jitter accumulator but excludes it from drift.
            random.scaled(settings.jitter);
            let drift = settings.speed * wind_speed * delta as f32 / 60.0;
            block.origin[0] += drift * self.wind_angle.sin();
            block.origin[2] += drift * self.wind_angle.cos();
            block.age_ms = block.age_ms.wrapping_add(delta);
            if block.origin[1] < -settings.height {
                block.origin[0] = x;
                block.origin[2] = z;
                block.origin[1] += settings.height * 2.0 - 1.0;
                block.yaw = random.scaled(TAU);
                block.age_ms = 0;
            }
        }
    }
}

fn upwind(angle: f32, power: f32) -> [f32; 2] {
    let distance = 6.0 * 5.4 * power;
    [-distance * angle.sin(), -distance * angle.cos()]
}

#[derive(Clone)]
pub(crate) struct Rain(Rc<RefCell<Vec<RainDraw>>>);

impl Rain {
    pub(crate) fn register(host: &mut NativeHost) -> Result<()> {
        let rain = Self(Rc::default());
        let draws = rain.clone();
        let random = host.random.clone();
        let weather = host.weather.clone();
        if host.entities.register_class("rain", move || {
            Ok(Box::new(Actor {
                draws: draws.clone(),
                weather: weather.clone(),
                random: random.clone(),
                settings: Settings::default(),
                streaks: None,
                visible: true,
            }))
        })? {
            host.rain = Some(rain);
        }
        Ok(())
    }

    pub(crate) fn clear_draws(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn draws(&self) -> Vec<RainDraw> {
        self.0.borrow().clone()
    }
}

struct Actor {
    draws: Rain,
    weather: Option<Weather>,
    random: Rc<RefCell<Random>>,
    settings: Settings,
    streaks: Option<Streaks>,
    visible: bool,
}

impl Entity for Actor {
    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let name = store
            .name(changed)
            .context("missing RAIN attribute name")?
            .to_ascii_lowercase();
        let parent = store
            .get(changed)
            .and_then(|attr| attr.parent())
            .and_then(|id| store.name(id));
        if parent.is_some_and(|name| {
            name.eq_ignore_ascii_case("raindrops") || name.eq_ignore_ascii_case("rainbow")
        }) {
            return Ok(0);
        }
        let dword = || store.as_dword(changed, None, 0);
        let float = || -> Result<f32> {
            let value = store.as_float(changed, None, 0.0);
            ensure!(value.is_finite(), "non-finite RAIN setting {name}");
            Ok(value)
        };
        let settings = &mut self.settings;
        match name.as_str() {
            "clear" => {
                self.streaks = None;
                settings.count = 0;
            }
            "isdone" => {
                let wind = self
                    .weather
                    .as_ref()
                    .and_then(Weather::wind)
                    .context("RAIN requires a Weather entity")?;
                let next = if settings.count == 0 {
                    None
                } else {
                    Some(Streaks::new(settings, wind, &mut self.random.borrow_mut())?)
                };
                self.streaks = next;
            }
            "numdrops" => settings.count = dword(),
            "color" => settings.argb = dword(),
            "droplength" => settings.length = float()?,
            "height" => settings.height = float()?,
            "radius" => settings.radius = float()?,
            "speed" => settings.speed = float()?,
            "jitter" => settings.jitter = float()?,
            "windspeedjitter" => settings.wind_jitter = float()?,
            "timeblend" => settings.blend_ms = dword(),
            "maxblend" => settings.max_blend = dword(),
            _ => {}
        }
        Ok(0)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if message.long()? == 1 {
            self.visible = message.long()? != 0;
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage == Stage::Realize
            && self.visible
            && let Some(streaks) = &mut self.streaks
        {
            self.draws.0.borrow_mut().push(streaks.draw(&self.settings));
            streaks.advance(&self.settings, delta_ms, &mut self.random.borrow_mut());
        }
        Ok(())
    }
}
