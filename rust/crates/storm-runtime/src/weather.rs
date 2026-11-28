//! Original WEATHER clock, attribute observer and celestial state.
use crate::NativeHost;
use anyhow::{Context, Result, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    f32::consts::{FRAC_PI_2, PI},
    rc::Rc,
};
use storm_core::{AttributeId, AttributeStore, Entity, EntityDestroy, EntityId, EntityInit, Stage};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct WeatherState {
    pub hour: f32,
    pub wind_angle: f32,
    pub wind_speed: f32,
    pub fog_enabled: bool,
    pub fog_start: f32,
    pub fog_density: f32,
    pub fog_color: u32,
    pub sun_color: u32,
    pub ambient: u32,
    /// The native WEATHER vector also used by the moon at night.
    pub light_position: [f32; 3],
    pub sun_present: bool,
    pub moon_present: bool,
}

struct Trajectory {
    height: f32,
    begin_angle: f32,
    end_angle: f32,
    begin_time: f32,
    end_time: f32,
}

impl Trajectory {
    fn phase(&self, hour: f32, wrap: bool) -> Option<f32> {
        let mut from_begin = hour - self.begin_time;
        let mut duration = self.end_time - self.begin_time;
        if wrap && self.begin_time >= self.end_time {
            if from_begin < 0.0 {
                from_begin += 24.0;
            }
            duration = 24.0 + self.end_time - self.begin_time;
        }
        let phase = from_begin / duration;
        (0.0..=1.0).contains(&phase).then_some(phase)
    }

    fn azimuth(&self, phase: f32) -> f32 {
        self.begin_angle + phase * (self.end_angle - self.begin_angle)
    }
}

struct State {
    values: WeatherState,
    speed: f32,
    frequency: f32,
    sun: Trajectory,
    moon: Trajectory,
    height: f32,
    azimuth: f32,
    time_event: Option<f32>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            values: WeatherState::default(),
            speed: 0.0,
            frequency: 0.0,
            sun: Trajectory {
                height: 3.0 / 4.0 * FRAC_PI_2,
                begin_angle: 0.0,
                end_angle: PI,
                begin_time: 5.0,
                end_time: 23.0,
            },
            moon: Trajectory {
                height: 5.0 / 4.0 * FRAC_PI_2,
                begin_angle: FRAC_PI_2,
                end_angle: -FRAC_PI_2,
                begin_time: 23.5,
                end_time: 4.5,
            },
            height: 0.0,
            azimuth: 0.0,
            time_event: None,
        }
    }
}

fn light_position(height: f32, azimuth: f32) -> [f32; 3] {
    let horizontal = 5000.0 * (-height).cos();
    [
        horizontal * azimuth.cos(),
        -5000.0 * (-height).sin(),
        -horizontal * azimuth.sin(),
    ]
}

impl State {
    fn update_celestial_positions(&mut self) {
        let sun = self.sun.phase(self.values.hour, false);
        self.values.sun_present = sun.is_some();
        self.values.light_position = if let Some(phase) = sun {
            self.height = (self.sun.height + 0.2) * (phase * PI).sin() - 0.2;
            self.azimuth = self.sun.azimuth(phase);
            light_position(self.height, self.azimuth)
        } else {
            self.height = -FRAC_PI_2;
            self.azimuth = 0.0;
            [0.0, -5000.0, 0.0]
        };
        let moon = self.moon.phase(self.values.hour, true);
        self.values.moon_present = moon.is_some();
        if let Some(phase) = moon {
            self.height = self.moon.height * phase.min(1.0 - phase) * 2.0;
            self.azimuth = self.moon.azimuth(phase);
            self.values.light_position = light_position(self.height, self.azimuth);
        }
    }

    fn apply_attributes(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<bool> {
        let attr = store.get(changed).context("missing Weather attribute")?;
        let parent = attr
            .parent()
            .and_then(|id| store.name(id))
            .unwrap_or_default()
            .to_ascii_lowercase();
        let name = store.name(changed).unwrap_or_default().to_ascii_lowercase();
        let float = || -> Result<f32> {
            let value = store.as_float(changed, None, 0.0);
            ensure!(value.is_finite(), "non-finite Weather setting");
            Ok(value)
        };
        let dword = || store.as_dword(changed, None, 0);
        match (parent.as_str(), name.as_str()) {
            ("fog", "enable") => self.values.fog_enabled = dword() != 0,
            ("fog", "start") => self.values.fog_start = float()?,
            ("fog", "density") => self.values.fog_density = float()?,
            ("fog", "color") => self.values.fog_color = dword(),
            ("wind", "angle") => self.values.wind_angle = float()?,
            ("wind", "speed") => self.values.wind_speed = float()?,
            ("sun", "color") => self.values.sun_color = dword(),
            ("sun", "ambient") => self.values.ambient = dword(),
            ("sun", "heightangle") if self.speed == 0.0 => self.height = float()?,
            ("sun", "azimuthangle") if self.speed == 0.0 => self.azimuth = float()?,
            (
                body @ ("sun" | "moon"),
                key @ ("height" | "begangle" | "endangle" | "begtime" | "endtime"),
            ) => {
                let trajectory = if body == "sun" {
                    &mut self.sun
                } else {
                    &mut self.moon
                };
                *match key {
                    "height" => &mut trajectory.height,
                    "begangle" => &mut trajectory.begin_angle,
                    "endangle" => &mut trajectory.end_angle,
                    "begtime" => &mut trajectory.begin_time,
                    "endtime" => &mut trajectory.end_time,
                    _ => unreachable!("Weather trajectory field"),
                } = float()?;
            }
            ("time", "time") => self.values.hour = float()?,
            ("time", "speed") => {
                let speed = float()?;
                let rate = if speed == 0.0 { 0.0 } else { 0.001 / speed };
                ensure!(rate.is_finite(), "Weather clock speed overflow");
                self.speed = rate;
            }
            ("time", "updatefrequence") => self.frequency = float()?,
            (_, "isdone") => {
                if self.speed == 0.0 {
                    self.values.light_position = light_position(self.height, self.azimuth);
                } else {
                    self.update_celestial_positions();
                }
                return Ok(true);
            }
            _ => {}
        }
        Ok(false)
    }

    fn execute(&mut self, delta_ms: u32) -> Result<()> {
        if self.speed == 0.0 {
            return Ok(());
        }
        let old_hour = self.values.hour;
        let mut next = old_hour + delta_ms as f32 * self.speed;
        ensure!(next.is_finite(), "Weather clock overflow");
        if next > 24.0 {
            next -= 24.0;
        }
        self.values.hour = next;
        self.update_celestial_positions();
        if (old_hour * self.frequency) as i32 != (next * self.frequency) as i32 {
            self.time_event = Some(next);
        }
        Ok(())
    }
}

struct Shared {
    states: RefCell<BTreeMap<u64, State>>,
    ambient: Rc<Cell<Option<[u8; 3]>>>,
}

#[derive(Clone)]
pub(crate) struct Weather(Rc<Shared>);

impl Weather {
    pub(crate) fn wind(&self) -> Option<(f32, f32)> {
        let weather = self.first_state()?;
        Some((weather.wind_angle, weather.wind_speed))
    }

    pub(crate) fn first_state(&self) -> Option<WeatherState> {
        self.0
            .states
            .borrow()
            .values()
            .next()
            .map(|state| state.values)
    }

    pub(crate) fn register(host: &mut NativeHost) -> Result<()> {
        let weather = Self(Rc::new(Shared {
            states: RefCell::default(),
            ambient: host.ambient_light.clone(),
        }));
        let factory = weather.clone();
        if host.entities.register_class("Weather", move || {
            Ok(Box::new(Actor {
                weather: factory.clone(),
                id: EntityId::INVALID,
            }))
        })? {
            host.weather = Some(weather);
        }
        Ok(())
    }

    pub(crate) fn take_time_update(&self, id: EntityId) -> Option<f32> {
        self.0
            .states
            .borrow_mut()
            .get_mut(&id.raw())?
            .time_event
            .take()
    }
}

struct Actor {
    weather: Weather,
    id: EntityId,
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        self.weather
            .0
            .states
            .borrow_mut()
            .insert(self.id.raw(), State::default());
        Ok(true)
    }

    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let mut states = self.weather.0.states.borrow_mut();
        let state = states
            .get_mut(&self.id.raw())
            .context("missing Weather state")?;
        if state.apply_attributes(store, changed)? {
            let [_, r, g, b] = state.values.ambient.to_be_bytes();
            self.weather.0.ambient.set(Some([r, g, b]));
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage == Stage::Execute {
            self.weather
                .0
                .states
                .borrow_mut()
                .get_mut(&self.id.raw())
                .context("missing Weather state")?
                .execute(delta_ms)?;
        }
        Ok(())
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.weather.0.states.borrow_mut().remove(&self.id.raw());
        Ok(())
    }
}

impl NativeHost {
    pub fn weather_state(&self) -> Option<WeatherState> {
        let id = self.entities.first_id("weather")?;
        self.weather
            .as_ref()?
            .0
            .states
            .borrow()
            .get(&id.raw())
            .map(|state| state.values)
    }
}
