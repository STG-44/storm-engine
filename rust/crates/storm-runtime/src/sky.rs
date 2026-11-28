//! Native SKY weather schedule and its Realize requests.
use crate::{GameDirectory, NativeHost, weather::Weather};
use anyhow::{Context, Result, ensure};
use std::{cell::RefCell, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, EntityId, EntityInit, Stage};
use storm_render::{
    model::{SkyDraw, SkyFaces},
    texture_paths::TexturePaths,
};

const FACE_NAMES: [&str; 5] = [
    "sky_fr.tga",
    "sky_lf.tga",
    "sky_bk.tga",
    "sky_rt.tga",
    "sky_up.tga",
];

#[derive(Clone, Default)]
pub(crate) struct Skies(Rc<RefCell<Vec<Staged>>>);

pub(crate) struct Staged {
    pub id: EntityId,
    draw: SkyDraw,
    pub fog: Rc<RefCell<Fog>>,
}

#[derive(Default)]
pub(crate) struct Fog {
    pub dirty: bool,
    pub colors: Option<Rc<[[u8; 4]]>>,
}

impl Skies {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let skies = Self::default();
        let draws = skies.clone();
        let paths = host.texture_paths.clone();
        let weather = host.weather.clone();
        if host.entities.register_class("Sky", move || {
            Ok(Box::new(Actor {
                draws: draws.clone(),
                game: game.clone(),
                paths: paths.clone(),
                weather: weather.clone(),
                id: EntityId::INVALID,
                schedule: Schedule::default(),
                ready: false,
                loaded: [None, None],
                size: 2048.0,
                angle: 0.0,
                speed: 0.0,
                rotation: 0.0,
                fog: Rc::default(),
            }))
        })? {
            host.skies = Some(skies);
        }
        Ok(())
    }

    pub(crate) fn begin_frame(&self) {
        self.0.borrow_mut().clear();
    }

    pub(crate) fn stage_request(&self, id: EntityId) -> Option<(Rc<RefCell<Fog>>, bool)> {
        let draws = self.0.borrow();
        let draw = draws.last().filter(|draw| draw.id == id)?;
        Some((draw.fog.clone(), draw.draw.celestial))
    }

    pub(crate) fn draw(&self) -> Option<SkyDraw> {
        let draws = self.0.borrow();
        let latest = draws.last()?;
        let mut draw = latest.draw.clone();
        draw.fog = latest.fog.borrow().colors.clone();
        Some(draw)
    }
}

#[derive(Default)]
struct Schedule {
    day: Vec<String>,
    hour: f32,
}

impl Schedule {
    fn read(store: &AttributeStore, dir: Option<AttributeId>) -> Result<Self> {
        let Some(dir) = dir else {
            return Ok(Self::default());
        };
        let children = store.get(dir).context("missing Sky.Dir")?.children();
        ensure!(children.len() <= 24, "Sky accepts up to 24 weather slots");
        let mut day = vec![String::new(); children.len()];
        if let [only] = children {
            day[0] = store.value(*only, None).unwrap_or_default().into();
        } else {
            for &entry in children {
                let Some(index) = store
                    .name(entry)
                    .and_then(|key| key.strip_prefix('d'))
                    .and_then(|index| index.parse::<usize>().ok())
                    .filter(|&index| index < day.len())
                else {
                    continue;
                };
                day[index] = store.value(entry, None).unwrap_or_default().into();
            }
        }
        let hour = store.as_float(dir, None, 0.0);
        Ok(Self { day, hour })
    }

    fn pick(&self, hour: f32) -> Option<([usize; 2], f32)> {
        let count = self.day.len();
        if count == 0 {
            return None;
        }
        let hour = if (0.0..=24.0).contains(&hour) {
            hour
        } else {
            0.0
        };
        let phase = hour * (1.0 / 24.0) * count as f32;
        let current = phase as usize % count;
        let next = (current + 1) % count;
        let blend = if count == 1 { 0.0 } else { phase.fract() };
        Some(([current, next], blend))
    }
}

struct Loaded {
    directory: String,
    faces: Rc<SkyFaces>,
}

struct Actor {
    draws: Skies,
    game: GameDirectory,
    paths: Rc<RefCell<TexturePaths>>,
    weather: Option<Weather>,
    id: EntityId,
    schedule: Schedule,
    ready: bool,
    loaded: [Option<Loaded>; 2],
    size: f32,
    angle: f32,
    speed: f32,
    rotation: f32,
    fog: Rc<RefCell<Fog>>,
}

impl Actor {
    fn faces(&self, directory: &str) -> Result<Loaded> {
        if let Some(previous) = self
            .loaded
            .iter()
            .flatten()
            .find(|old| old.directory.eq_ignore_ascii_case(directory))
        {
            return Ok(Loaded {
                directory: directory.into(),
                faces: previous.faces.clone(),
            });
        }
        let mut faces = SkyFaces::default();
        if !directory.is_empty() {
            let directory = directory.trim_end_matches(['/', '\\']);
            let paths = self.paths.borrow();
            for (name, target) in FACE_NAMES.into_iter().zip(faces.0.iter_mut()) {
                *target = self
                    .game
                    .load_texture(&paths, &format!("{directory}/{name}"))?
                    .map(Rc::new);
            }
        }
        Ok(Loaded {
            directory: directory.into(),
            faces: Rc::new(faces),
        })
    }

    fn update_faces(&mut self, slots: [usize; 2]) -> Result<()> {
        let [current, next] = slots.map(|slot| self.schedule.day[slot].as_str());
        if self
            .loaded
            .iter()
            .zip([current, next])
            .all(|(loaded, dir)| {
                loaded
                    .as_ref()
                    .is_some_and(|old| old.directory.eq_ignore_ascii_case(dir))
            })
        {
            return Ok(());
        }
        let first = self.faces(current)?;
        let second = if current.eq_ignore_ascii_case(next) {
            Loaded {
                directory: next.into(),
                faces: first.faces.clone(),
            }
        } else {
            self.faces(next)?
        };
        self.loaded = [Some(first), Some(second)];
        Ok(())
    }
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        Ok(true)
    }

    fn attribute_changed(&mut self, store: &AttributeStore, id: AttributeId) -> Result<u32> {
        let attr = store.get(id).context("missing Sky attribute")?;
        let key = store
            .name(id)
            .context("unnamed Sky attribute")?
            .to_ascii_lowercase();
        let number = || -> Result<f32> {
            let value = store.as_float(id, None, 0.0);
            ensure!(value.is_finite(), "non-finite Sky attribute {key}");
            Ok(value)
        };
        match key.as_str() {
            "dir" => self.schedule = Schedule::read(store, Some(id))?,
            "size" => self.size = number()?,
            "angle" => self.angle = number()?,
            "rotatespeed" => self.speed = number()?,
            "isdone" => {
                let dir = attr.parent().and_then(|root| store.child(root, "Dir"));
                self.schedule = Schedule::read(store, dir)?;
                self.loaded = [None, None];
                if let Some((slots, _)) = self.schedule.pick(self.schedule.hour) {
                    self.update_faces(slots)?;
                }
                self.rotation = 0.0;
                self.ready = true;
                self.fog.borrow_mut().dirty = true;
            }
            "timeupdate" => self.fog.borrow_mut().dirty |= self.ready,
            _ => {}
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage != Stage::Realize || !self.ready {
            return Ok(());
        }
        let hour = self
            .weather
            .as_ref()
            .and_then(Weather::first_state)
            .map_or(self.schedule.hour, |weather| weather.hour);
        let Some((slots, blend)) = self.schedule.pick(hour) else {
            return Ok(());
        };
        self.update_faces(slots)?;
        self.rotation += delta_ms as f32 * 0.001 * self.speed;
        let [Some(current), Some(next)] = &self.loaded else {
            unreachable!("prepared sky textures")
        };
        self.draws.0.borrow_mut().push(Staged {
            id: self.id,
            draw: SkyDraw {
                current: current.faces.clone(),
                next: next.faces.clone(),
                size: self.size,
                angle: self.angle + self.rotation,
                blend,
                celestial: self.schedule.day.len() > 1 && delta_ms != 0,
                fog: None,
            },
            fog: self.fog.clone(),
        });
        Ok(())
    }
}
