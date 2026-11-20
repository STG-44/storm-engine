//! The configured SEA entity shares the retained wave solver with the fleet.
use crate::{GameDirectory, NativeHost};
use anyhow::{Context, Result};
use std::{
    cell::{OnceCell, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};
use storm_core::{AttributeId, AttributeStore, Entity, EntityDestroy, EntityId, EntityInit, Stage};
use storm_sim::sea::{Frames, MAX_TGA_BYTES, Parameters, Sea};

pub use storm_sim::sea::Surface;

struct Shared {
    game: GameDirectory,
    frames: OnceCell<Arc<Frames>>,
    entities: RefCell<BTreeMap<u64, Sea>>,
    draw: RefCell<Option<Surface>>,
}

#[derive(Clone)]
pub(crate) struct Seas(Rc<Shared>);

impl Seas {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let seas = Self(Rc::new(Shared {
            game,
            frames: OnceCell::new(),
            entities: RefCell::default(),
            draw: RefCell::default(),
        }));
        let factory = seas.clone();
        if host.entities.register_class("sea", move || {
            Ok(Box::new(Actor {
                seas: factory.clone(),
                id: EntityId::INVALID,
                attributes: None,
            }))
        })? {
            host.seas = Some(seas);
        }
        Ok(())
    }

    fn frames(&self) -> Result<Arc<Frames>> {
        if let Some(frames) = self.0.frames.get() {
            return Ok(frames.clone());
        }
        let directory = self
            .0
            .game
            .directory("resource/sea")
            .context("open SEA wave resources")?;
        let frames = Arc::new(Frames::from_tga_series(|name| {
            directory.read(name, MAX_TGA_BYTES)
        })?);
        Ok(self.0.frames.get_or_init(|| frames).clone())
    }

    pub(crate) fn begin_frame(&self) {
        self.0.draw.replace(None);
    }

    pub(crate) fn draw(&self) -> Option<Surface> {
        self.0.draw.borrow().clone()
    }

    pub(crate) fn surface(&self, id: EntityId) -> Option<Surface> {
        self.0
            .entities
            .borrow()
            .get(&id.raw())
            .map(|sea| sea.surface().clone())
    }
}

struct Actor {
    seas: Seas,
    id: EntityId,
    attributes: Option<AttributeId>,
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        let mut sea = Sea::new(self.seas.frames()?, Parameters::default())?;
        sea.enable_weather();
        self.id = context.id;
        self.attributes = context.attributes;
        self.seas.0.entities.borrow_mut().insert(self.id.raw(), sea);
        Ok(true)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        if stage != Stage::Realize {
            return Ok(());
        }
        let mut entities = self.seas.0.entities.borrow_mut();
        let sea = entities
            .get_mut(&self.id.raw())
            .context("missing active SEA state")?;
        if !sea.weather().is_some_and(|weather| weather.stopped) {
            // The retained stage uses the preceding published surface during
            // Execute and publishes the new surface during Realize.
            sea.advance(delta_ms)?;
            self.seas.0.draw.replace(Some(sea.surface().clone()));
        }
        Ok(())
    }

    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        if let Some(root) = self.attributes.filter(|&root| root != changed) {
            let mut entities = self.seas.0.entities.borrow_mut();
            let sea = entities
                .get_mut(&self.id.raw())
                .context("missing active SEA state")?;
            let change = sea.attribute_changed(store, root, changed)?;
            if change.realize_zero_requested
                && !sea.weather().is_some_and(|weather| weather.stopped)
            {
                self.seas.0.draw.replace(Some(sea.surface().clone()));
            }
        }
        Ok(0)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.seas.0.entities.borrow_mut().remove(&self.id.raw());
        Ok(())
    }
}
