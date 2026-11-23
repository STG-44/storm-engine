//! Static MODELR messages and shared runtime ownership.
use crate::{GameDirectory, NativeHost, Section, Service, ServiceFrame};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, Message, Stage};
use storm_render::{
    model::{ModelAsset, NativeRigFrame, SailDamageSettings},
    texture_paths::TexturePaths,
};
mod render;
pub use render::{AtmosphereDraws, ModelScenes};

pub const LOAD_GEOMETRY: i32 = 20500;
pub const SET_POSITION: i32 = 20506;
pub const SET_TEXTURE_PATH: i32 = 20509;
const IDENTITY: [f32; 16] = [
    1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
];
const MAX_DRAWS: usize = 256;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DrawPart {
    Body,
    Sails,
    Ropes,
    Flags,
}

#[derive(Clone)]
pub struct Draw {
    pub entity: EntityId,
    pub revision: u64,
    pub asset: Rc<ModelAsset>,
    /// Original CMatrix bytes; renderer coordinate conversion is separate.
    pub matrix: [f32; 16],
    pub ambient_light: Option<[u8; 3]>,
    pub rig: Option<Arc<NativeRigFrame>>,
    pub part: DrawPart,
}
struct State {
    geometry: Option<Rc<ModelAsset>>,
    source: Option<PathBuf>,
    matrix: [f32; 16],
    revision: u64,
}
impl Default for State {
    fn default() -> Self {
        Self {
            geometry: None,
            source: None,
            matrix: IDENTITY,
            revision: 0,
        }
    }
}
struct Shared {
    game: GameDirectory,
    texture_path: RefCell<String>,
    texture_paths: Rc<RefCell<TexturePaths>>,
    ambient_light: Rc<Cell<Option<[u8; 3]>>>,
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
            texture_paths: host.texture_paths.clone(),
            ambient_light: host.ambient_light.clone(),
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
        host.models = Some(models.clone());
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
            ambient_light: self.0.ambient_light.get(),
            rig: None,
            part: DrawPart::Body,
        }))
    }

    pub(crate) fn load_sails(
        &self,
        entity: EntityId,
        damage: SailDamageSettings,
    ) -> Result<(u64, Rc<ModelAsset>)> {
        let (source, revision) = self.overlay_source(entity)?;
        let asset = ModelAsset::load_sails(
            &source,
            self.0.game.root(),
            &self.0.texture_paths.borrow(),
            damage,
        )
        .with_context(|| format!("load cloth from {}", source.display()))?;
        Ok((revision, Rc::new(asset)))
    }

    pub(crate) fn load_ropes(
        &self,
        entity: EntityId,
        settings: storm_sim::rope::Settings,
        texture: Rc<storm_formats::texture::Texture>,
    ) -> Result<(u64, Rc<ModelAsset>)> {
        let (source, revision) = self.overlay_source(entity)?;
        let asset = ModelAsset::load_ropes(&source, settings, texture)
            .with_context(|| format!("load ropes from {}", source.display()))?;
        Ok((revision, Rc::new(asset)))
    }

    pub(crate) fn load_flags(
        &self,
        entity: EntityId,
        grid: [u32; 2],
        texture: Rc<storm_formats::texture::Texture>,
    ) -> Result<(u64, Rc<ModelAsset>)> {
        let (source, revision) = self.overlay_source(entity)?;
        let asset = ModelAsset::load_flags(&source, grid, texture)
            .with_context(|| format!("load flags from {}", source.display()))?;
        Ok((revision, Rc::new(asset)))
    }
    fn overlay_source(&self, entity: EntityId) -> Result<(PathBuf, u64)> {
        let models = self
            .0
            .models
            .try_borrow()
            .context("reentrant model registry")?;
        let state = models
            .get(&entity.raw())
            .context("rigging attachment requires a MODELR entity")?
            .try_borrow()
            .context("reentrant model state")?;
        Ok((
            state
                .source
                .clone()
                .context("rigging attachment has no loaded geometry")?,
            state.revision,
        ))
    }
    pub(crate) fn queue_draw(&self, draw: Draw) -> Result<()> {
        let mut draws = self
            .0
            .draws
            .try_borrow_mut()
            .context("reentrant model draw queue")?;
        ensure!(draws.len() < MAX_DRAWS, "native model draw budget");
        draws.push(draw);
        Ok(())
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
                let paths = self
                    .shared
                    .0
                    .texture_paths
                    .try_borrow()
                    .context("reentrant renderer texture paths")?;
                let asset = ModelAsset::load_geometry_with_paths(
                    &path,
                    self.shared.0.game.root(),
                    &prefix,
                    &paths,
                )
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
                state.source = Some(path);
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
            self.shared.queue_draw(draw)?;
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
