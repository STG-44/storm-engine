//! Configured ROPE entity and its checked shared endpoints.
use crate::{
    GameDirectory, NativeHost,
    config::ini_field,
    model::{DrawPart, Models},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{RefCell, RefMut},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, Message, Stage};
use storm_formats::texture::Texture;
use storm_render::{
    model::{ModelAsset, NativeRigFrame, SailPatch},
    texture_paths::TexturePaths,
};
use storm_sim::{
    rope::{Phase, Pose, Settings},
    sail_rope::End,
};

const INIT: i32 = 40100;
const DELETE: i32 = 40101;
const DEL_GROUP: i32 = 40102;

fn load(game: &GameDirectory, paths: &TexturePaths) -> Result<(Settings, Rc<Texture>)> {
    let bytes = game.read("RESOURCE/INI/rigging.ini", 1024 * 1024)?;
    let text = String::from_utf8_lossy(&bytes);
    let field = |key| {
        ini_field(&text, "ROPES", key)
            .map(|value| value.split(';').next().unwrap_or_default().trim())
    };
    let number = |key, default| -> Result<f32> {
        field(key).map_or(Ok(default), |value| {
            value
                .parse()
                .with_context(|| format!("invalid [ROPES] {key}"))
        })
    };
    let defaults = Settings::default();
    let settings = Settings {
        segment_length: number("fSEG_LENGTH", defaults.segment_length)?,
        width: number("fWIDTH", defaults.width)?,
        stay_width: number("fSTAY_WIDTH", defaults.stay_width)?,
        end_length: number("fEND_LENGTH", defaults.end_length)?,
        texture_length: number("fTexHeight", defaults.texture_length)?,
        wave: number("fWAVE", defaults.wave)?,
        depth_speed: number("fDeepChangeSpeed", defaults.depth_speed)?,
        rotation_speed: number("fRotateSpeed", defaults.rotation_speed)?,
        minimum_depth: number("fMinDeepMul", defaults.minimum_depth)?,
        depth_variation: number("fVarDeepMul", defaults.depth_variation)?,
        rotation_angle: number("fRotateAng", defaults.rotation_angle)?,
        max_distance: number("fMaxDist", defaults.max_distance)?,
    };
    settings.validate()?;
    let name = field("TextureName").unwrap_or("sail_rope.tga");
    let texture = game
        .load_texture(paths, name)?
        .with_context(|| format!("missing Rope texture {name}"))?;
    Ok((settings, Rc::new(texture)))
}

#[derive(Clone, Default)]
pub(crate) struct Groups(Rc<RefCell<BTreeMap<u64, Group>>>);

impl Groups {
    pub(crate) fn for_model(&self, model: EntityId, revision: u64) -> Option<RefMut<'_, Group>> {
        RefMut::filter_map(self.0.borrow_mut(), |groups| {
            groups
                .get_mut(&model.raw())
                .filter(|group| group.revision == revision)
        })
        .ok()
    }
}

pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<Groups> {
    let groups = Groups::default();
    let active = groups.clone();
    let models = host.models.clone();
    let paths = host.texture_paths.clone();
    host.entities.register_class("Rope", move || {
        let (settings, texture) =
            load(&game, &paths.borrow()).context("load Rope settings and material")?;
        Ok(Box::new(Actor {
            id: EntityId::INVALID,
            groups: active.clone(),
            models: models.clone(),
            settings,
            texture,
        }))
    })?;
    Ok(groups)
}

struct State {
    local: [[f32; 3]; 2],
    phase: Phase,
    next_phase: Phase,
}

pub(crate) struct Group {
    owner: EntityId,
    model: EntityId,
    revision: u64,
    asset: Rc<ModelAsset>,
    ropes: Vec<Option<State>>,
}

#[derive(Clone, Copy)]
pub(crate) struct Tie {
    rope: usize,
    side: usize,
    pub(crate) point: [f32; 3],
}

impl Group {
    pub(crate) fn ties(&self, patch: &SailPatch) -> [Option<Tie>; 4] {
        let mut ties = [None; 4];
        if let Some(origin) = &patch.origin {
            for (rope, (data, state)) in self
                .asset
                .native_ropes()
                .iter()
                .zip(&self.ropes)
                .enumerate()
            {
                let Some(state) = state else { continue };
                for (side, end) in data.locator.ends.iter().enumerate() {
                    if end.node == origin.node && end.group == origin.group {
                        ties[patch.rope_slot(end.point)] = Some(Tie {
                            rope,
                            side,
                            point: state.local[side],
                        });
                    }
                }
            }
        }
        ties
    }

    pub(crate) fn set_ties(&mut self, ties: [Option<Tie>; 4], points: [[f32; 3]; 4]) {
        for (tie, point) in ties.into_iter().zip(points) {
            if let Some(tie) = tie
                && let Some(state) = &mut self.ropes[tie.rope]
            {
                state.local[tie.side] = point;
            }
        }
    }

    fn frame(&self) -> Result<NativeRigFrame> {
        let mut frame = NativeRigFrame::default();
        for (data, state) in self.asset.native_ropes().iter().zip(&self.ropes) {
            let Some(state) = state else { continue };
            let world = |side: usize| -> Result<[f32; 3]> {
                let [x, y, z] = End {
                    point: state.local[side],
                    matrix: data.locator.ends[side].frame,
                }
                .world()?;
                Ok([x, y, -z])
            };
            frame.ropes.insert(
                data.locator.number,
                Pose {
                    ends: [world(0)?, world(1)?],
                    phase: state.phase,
                },
            );
        }
        Ok(frame)
    }
}

struct Actor {
    id: EntityId,
    groups: Groups,
    models: Option<Models>,
    settings: Settings,
    texture: Rc<Texture>,
}

impl Actor {
    fn attach(&self, model: EntityId) -> Result<()> {
        let models = self
            .models
            .as_ref()
            .context("Rope requires the MODELR service")?;
        let Some(draw) = models.loaded(model)? else {
            return Ok(());
        };
        if let Some(group) = self.groups.for_model(model, draw.revision) {
            ensure!(group.owner == self.id, "Rope model already has an owner");
            return Ok(());
        }
        let (revision, asset) = models.load_ropes(model, self.settings, self.texture.clone())?;
        let ropes = asset
            .native_ropes()
            .iter()
            .map(|rope| {
                Some(State {
                    local: rope.locator.ends.each_ref().map(|end| end.point),
                    phase: Phase::default(),
                    next_phase: Phase::default(),
                })
            })
            .collect::<Vec<_>>();
        if !ropes.is_empty() {
            self.groups.0.borrow_mut().insert(
                model.raw(),
                Group {
                    owner: self.id,
                    model,
                    revision,
                    asset,
                    ropes,
                },
            );
        }
        Ok(())
    }
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            INIT => {
                let _ship = message.entity()?;
                self.attach(message.entity()?)?;
            }
            DELETE => {
                let model = message.entity()?;
                let number = message.long()?;
                if let Some(group) = self
                    .groups
                    .0
                    .borrow_mut()
                    .get_mut(&model.raw())
                    .filter(|group| group.owner == self.id)
                    && let Some(index) = group
                        .asset
                        .native_ropes()
                        .iter()
                        .position(|rope| rope.locator.number == number)
                {
                    group.ropes[index] = None;
                }
            }
            DEL_GROUP => {
                let model = message.entity()?;
                let mut groups = self.groups.0.borrow_mut();
                if groups
                    .get(&model.raw())
                    .is_some_and(|group| group.owner == self.id)
                {
                    groups.remove(&model.raw());
                }
            }
            code => bail!("unported Rope message {code}"),
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta_ms: u32) -> Result<()> {
        let Some(models) = &self.models else {
            return Ok(());
        };
        let mut groups = self.groups.0.borrow_mut();
        let mut retired = Vec::new();
        for group in groups.values_mut().filter(|group| group.owner == self.id) {
            let draw = models
                .loaded(group.model)?
                .filter(|draw| draw.revision == group.revision);
            if let Some(draw) = draw {
                match stage {
                    Stage::Execute => {
                        for state in group.ropes.iter_mut().flatten() {
                            state.phase = state.next_phase;
                            state.next_phase.advance(self.settings, delta_ms);
                        }
                    }
                    Stage::Realize if delta_ms > 0 && group.ropes.iter().any(Option::is_some) => {
                        models.queue_draw(crate::model::Draw {
                            asset: group.asset.clone(),
                            rig: Some(Arc::new(group.frame()?)),
                            part: DrawPart::Ropes,
                            ..draw
                        })?;
                    }
                    Stage::Realize => {}
                }
            } else {
                retired.push(group.model.raw());
            }
        }
        for id in retired {
            groups.remove(&id);
        }
        Ok(())
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.groups
            .0
            .borrow_mut()
            .retain(|_, group| group.owner != self.id);
        Ok(())
    }
}
