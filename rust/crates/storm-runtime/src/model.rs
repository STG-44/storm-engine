//! Static MODELR messages and shared runtime ownership. Animation, native
//! lighting/techniques, collision and complete NODER behavior remain pending.
use crate::{GameDirectory, NativeHost, Section, Service, ServiceFrame};
use anyhow::{Context, Result, bail, ensure};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, Message, Stage};
use storm_render::model::ModelAsset;
mod render;
pub use render::ModelScenes;

pub const LOAD_GEOMETRY: i32 = 20500;
pub const SET_POSITION: i32 = 20506;
pub const SET_TEXTURE_PATH: i32 = 20509;
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
const MAX_DRAWS: usize = 256;

#[derive(Clone)]
pub struct Draw {
    pub entity: EntityId,
    pub revision: u64,
    pub asset: Rc<ModelAsset>,
    /// Original CMatrix bytes; renderer coordinate conversion is separate.
    pub matrix: [f32; 16],
}
struct State {
    geometry: Option<Rc<ModelAsset>>,
    matrix: [f32; 16],
    revision: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            geometry: None,
            matrix: IDENTITY,
            revision: 0,
        }
    }
}
struct Shared {
    game: GameDirectory,
    texture_path: RefCell<String>,
    models: RefCell<BTreeMap<u64, Rc<RefCell<State>>>>,
    draws: RefCell<Vec<Draw>>,
}

/// Native factories share geometry texture-path state but retain independent
/// loaded assets and transforms. Realize records immutable per-frame draws.
#[derive(Clone)]
pub struct Models(Rc<Shared>);
impl Models {
    pub fn register(host: &mut NativeHost, game: GameDirectory) -> Result<Self> {
        let models = Self(Rc::new(Shared {
            game,
            texture_path: RefCell::default(),
            models: RefCell::default(),
            draws: RefCell::default(),
        }));
        let factory = models.clone();
        let added = host.entities.register_class("MODELR", move || {
            Ok(Box::new(Actor {
                shared: factory.clone(),
                id: EntityId::INVALID,
                state: Rc::new(RefCell::new(State::default())),
            }))
        })?;
        ensure!(added, "MODELR is already registered");
        Ok(models)
    }
    pub fn live_ids(&self) -> Result<Vec<EntityId>> {
        Ok(self
            .0
            .models
            .try_borrow()
            .context("reentrant model registry")?
            .keys()
            .copied()
            .map(EntityId::from_raw)
            .collect())
    }
    pub fn draws(&self) -> Result<Vec<Draw>> {
        Ok(self
            .0
            .draws
            .try_borrow()
            .context("reentrant model draw queue")?
            .clone())
    }
    /// Transfer this frame's asset ownership to the renderer, which may keep it
    /// through submission even after the entity's deferred destruction.
    pub fn take_draws(&self) -> Result<Vec<Draw>> {
        Ok(std::mem::take(
            &mut *self
                .0
                .draws
                .try_borrow_mut()
                .context("reentrant model draw queue")?,
        ))
    }
    pub fn loaded(&self, entity: EntityId) -> Result<Option<Draw>> {
        let models = self
            .0
            .models
            .try_borrow()
            .context("reentrant model registry")?;
        let Some(state) = models.get(&entity.raw()) else {
            return Ok(None);
        };
        let state = state.try_borrow().context("reentrant model state")?;
        Ok(state.geometry.as_ref().map(|asset| Draw {
            entity,
            revision: state.revision,
            asset: asset.clone(),
            matrix: state.matrix,
        }))
    }
}
impl Service for Models {
    fn section(&self) -> Section {
        Section::Realize
    }
    fn run_start(&mut self, _: &mut ServiceFrame) -> Result<()> {
        self.0
            .draws
            .try_borrow_mut()
            .context("reentrant model draw queue")?
            .clear();
        Ok(())
    }
    fn run_end(&mut self, _: &mut ServiceFrame) -> Result<()> {
        Ok(())
    }
}
struct Actor {
    shared: Models,
    id: EntityId,
    state: Rc<RefCell<State>>,
}
impl Entity for Actor {
    fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
        self.id = ctx.id;
        let mut models = self
            .shared
            .0
            .models
            .try_borrow_mut()
            .context("reentrant model Init")?;
        ensure!(
            !models.contains_key(&self.id.raw()),
            "duplicate model owner"
        );
        models.insert(self.id.raw(), self.state.clone());
        Ok(true)
    }
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            LOAD_GEOMETRY => {
                let name = message.string()?;
                // GEOMETRY::CreateGeometry always appends .gm.
                let path = self
                    .shared
                    .0
                    .game
                    .resolve(&format!("RESOURCE/MODELS/{name}.gm"))?;
                let prefix = self
                    .shared
                    .0
                    .texture_path
                    .try_borrow()
                    .context("reentrant texture path")?
                    .clone();
                let asset = ModelAsset::load_geometry(&path, self.shared.0.game.root(), &prefix)
                    .with_context(|| format!("load native MODELR geometry {name}"))?;
                let mut state = self
                    .state
                    .try_borrow_mut()
                    .context("reentrant model state")?;
                let revision = state
                    .revision
                    .checked_add(1)
                    .context("model revision overflow")?;
                state.geometry = Some(Rc::new(asset));
                state.revision = revision;
                Ok(1)
            }
            SET_POSITION => {
                let values = (0..12)
                    .map(|_| message.float())
                    .collect::<Result<Vec<_>>>()?;
                ensure!(
                    values.iter().all(|v| v.is_finite()),
                    "nonfinite native model transform"
                );
                let mut matrix = IDENTITY;
                matrix[12..15].copy_from_slice(&values[..3]);
                matrix[..3].copy_from_slice(&values[3..6]);
                matrix[4..7].copy_from_slice(&values[6..9]);
                matrix[8..11].copy_from_slice(&values[9..12]);
                self.state
                    .try_borrow_mut()
                    .context("reentrant model state")?
                    .matrix = matrix;
                Ok(0)
            }
            SET_TEXTURE_PATH => {
                let path = message.string()?;
                ensure!(
                    path.len() < 256 && !path.contains('\0'),
                    "native geometry texture path budget"
                );
                *self
                    .shared
                    .0
                    .texture_path
                    .try_borrow_mut()
                    .context("reentrant texture path")? = path.to_owned();
                Ok(0)
            }
            code => bail!("MODELR message {code} is not ported"),
        }
    }
    fn process_stage_checked(&mut self, stage: Stage, _: u32) -> Result<()> {
        if stage == Stage::Realize
            && let Some(draw) = self.shared.loaded(self.id)?
        {
            let mut draws = self
                .shared
                .0
                .draws
                .try_borrow_mut()
                .context("reentrant model draw queue")?;
            ensure!(draws.len() < MAX_DRAWS, "native model draw budget");
            draws.push(draw);
        }
        Ok(())
    }
    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared
            .0
            .models
            .try_borrow_mut()
            .context("reentrant model destroy")?
            .remove(&self.id.raw());
        self.state
            .try_borrow_mut()
            .context("reentrant model state")?
            .geometry = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests;
