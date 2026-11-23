//! Configured Vant, VantL and VantZ entity groups.
use crate::{
    GameDirectory, NativeHost,
    config::ini_field,
    model::{DrawPart, Models},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};
use storm_core::{Entity, EntityId, EntityInit, Message, Stage};
use storm_formats::texture::Texture;
use storm_render::{
    model::{ModelAsset, NativeRigFrame, RigMesh},
    texture_paths::TexturePaths,
};
use storm_sim::shroud::{Kind, Settings};

const INIT: i32 = 40300;
const DEL_GROUP: i32 = 40301;
const DEL_MAST: i32 = 40302;

fn load(
    game: &GameDirectory,
    paths: &TexturePaths,
    section: &str,
) -> Result<(Settings, Rc<Texture>)> {
    let bytes = game.read("RESOURCE/INI/rigging.ini", 1024 * 1024)?;
    let text = String::from_utf8_lossy(&bytes);
    let field = |key: &str| {
        ini_field(&text, section, key).map(|s| s.split(';').next().unwrap_or_default().trim())
    };
    let float = |key: &str, default| -> Result<f32> {
        field(key).map_or(Ok(default), |value| {
            value
                .parse()
                .with_context(|| format!("invalid [{section}] {key}"))
        })
    };
    let range = |keys: [&str; 2], values: [f32; 2]| -> Result<[f32; 2]> {
        Ok([float(keys[0], values[0])?, float(keys[1], values[1])?])
    };
    let defaults = Settings::default();
    let ropes = if let Some(value) = field("fRopeQuant") {
        value
            .parse::<i32>()
            .with_context(|| format!("invalid [{section}] fRopeQuant"))?
            .max(2) as usize
    } else {
        defaults.ropes
    };
    let settings = Settings {
        width: float("fWidth", defaults.width)?,
        ropes,
        horizontal_u: range(["fHRopeXbeg", "fHRopeXend"], defaults.horizontal_u)?,
        triangle_u: range(["fTreangXbeg", "fTreangXend"], defaults.triangle_u)?,
        triangle_v: range(["fTreangYbeg", "fTreangYend"], defaults.triangle_v)?,
        beam_v: range(["fBalkYbeg", "fBalkYend"], defaults.beam_v)?,
        vertical_u: range(["fVRopeXbeg", "fVRopeXend"], defaults.vertical_u)?,
        upper_width: float("fTreangWidth", defaults.upper_width)?,
        upper_height: float("fTreangHeight", defaults.upper_height)?,
        vertical_repeat: float("fVRopeHeight", defaults.vertical_repeat)?,
        horizontal_repeat: float("fHRopeHeight", defaults.horizontal_repeat)?,
        beam_height: float("fBalkHeight", defaults.beam_height)?,
        beam_width: float("fBalkWidth", defaults.beam_width)?,
        max_distance: float("fVantMaxDist", defaults.max_distance)?,
    };
    settings.validate()?;
    let name = field("TextureName").unwrap_or("vant.tga");
    let texture = game
        .load_texture(paths, name)?
        .with_context(|| format!("missing [{section}] texture {name}"))?;
    Ok((settings, Rc::new(texture)))
}

pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
    for (kind, name, section) in [
        (Kind::Vant, "Vant", "VANTS"),
        (Kind::VantL, "VantL", "VANTS_L"),
        (Kind::VantZ, "VantZ", "VANTS_Z"),
    ] {
        let models = host.models.clone();
        let paths = host.texture_paths.clone();
        let game = game.clone();
        host.entities.register_class(name, move || {
            let (settings, texture) = load(&game, &paths.borrow(), section)
                .with_context(|| format!("load {name} settings and material"))?;
            Ok(Box::new(Actor {
                kind,
                models: models.clone(),
                settings,
                texture,
                groups: BTreeMap::new(),
            }))
        })?;
    }
    Ok(())
}

struct Group {
    revision: u64,
    asset: Rc<ModelAsset>,
    removed: BTreeSet<usize>,
}

struct Actor {
    kind: Kind,
    models: Option<Models>,
    settings: Settings,
    texture: Rc<Texture>,
    groups: BTreeMap<u64, Group>,
}

impl Actor {
    fn attach(&mut self, model: EntityId) -> Result<()> {
        let models = self
            .models
            .as_ref()
            .context("Vant requires the MODELR service")?;
        let loaded = models
            .loaded(model)?
            .context("Vant INIT requires loaded ship geometry")?;
        if self
            .groups
            .get(&model.raw())
            .is_some_and(|group| group.revision == loaded.revision)
        {
            return Ok(());
        }
        let (revision, asset) =
            models.load_shrouds(model, self.kind, self.settings, self.texture.clone())?;
        if asset.meshes.is_empty() {
            self.groups.remove(&model.raw());
        } else {
            self.groups.insert(
                model.raw(),
                Group {
                    revision,
                    asset,
                    removed: BTreeSet::new(),
                },
            );
        }
        Ok(())
    }

    fn detach_mast(&mut self, model: EntityId, name: &str) {
        if let Some(group) = self.groups.get_mut(&model.raw()) {
            group
                .removed
                .extend(
                    group
                        .asset
                        .meshes
                        .iter()
                        .enumerate()
                        .filter_map(|(index, mesh)| match &mesh.rigging {
                            Some(RigMesh::NativeShroud(shroud))
                                if shroud.mast_node.eq_ignore_ascii_case(name) =>
                            {
                                Some(index)
                            }
                            _ => None,
                        }),
                );
            if group.removed.len() == group.asset.meshes.len() {
                self.groups.remove(&model.raw());
            }
        }
    }
}

impl Entity for Actor {
    fn init(&mut self, _: &mut EntityInit<'_>) -> Result<bool> {
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            INIT => {
                let _ship = message.entity()?;
                self.attach(message.entity()?)?;
            }
            DEL_GROUP => {
                let model = message.entity()?;
                self.groups.remove(&model.raw());
            }
            DEL_MAST => {
                let model = message.entity()?;
                if message.current_type() == Some('s') {
                    self.detach_mast(model, message.string()?);
                } else {
                    // Original callers pass a NODE pointer. Accept its null no-op
                    // and use a checked model-local node name for Rust callers.
                    ensure!(
                        message.pointer()? == 0,
                        "Vant mast removal requires a model node name"
                    );
                }
            }
            code => bail!("unported Vant message {code}"),
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, _: u32) -> Result<()> {
        let Some(models) = &self.models else {
            return Ok(());
        };
        let mut stale = Vec::new();
        for (&model, group) in &self.groups {
            if let Some(draw) = models
                .loaded(EntityId::from_raw(model))?
                .filter(|draw| draw.revision == group.revision)
            {
                if stage == Stage::Realize {
                    let rig = (!group.removed.is_empty()).then(|| {
                        Arc::new(NativeRigFrame {
                            removed_shrouds: group.removed.clone(),
                            ..Default::default()
                        })
                    });
                    models.queue_draw(crate::model::Draw {
                        asset: group.asset.clone(),
                        part: DrawPart::Shrouds(self.kind),
                        rig,
                        ..draw
                    })?;
                }
            } else {
                stale.push(model);
            }
        }
        for model in stale {
            self.groups.remove(&model);
        }
        Ok(())
    }
}
