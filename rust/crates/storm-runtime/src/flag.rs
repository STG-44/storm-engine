//! Configured FLAG entity, atlas lookup and script-owned flag selection.
use crate::{
    GameDirectory, NativeHost,
    config::ini_field,
    model::{DrawPart, Models},
    weather::Weather,
};
use anyhow::{Context, Result, bail, ensure};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc, sync::Arc};
use storm_core::{
    Entity, EntityDestroy, EntityId, EntityInit, EntityManager, Message, Stage, Value as Field,
};
use storm_formats::texture::Texture;
use storm_render::{
    model::{ModelAsset, NativeRigFrame, RigMesh, SailWind},
    texture_paths::TexturePaths,
};
use storm_script::Value;
use storm_sim::FlagTexture;

const INIT_SHIP: i32 = 40200;
const DEL_GROUP: i32 = 40201;
const INIT_TOWN: i32 = 40202;

struct Atlas {
    grid: [u32; 2],
    texture: Rc<Texture>,
}

impl Atlas {
    fn load(game: &GameDirectory, texture_paths: &TexturePaths) -> Result<Self> {
        let bytes = game.read("RESOURCE/INI/rigging.ini", 1024 * 1024)?;
        let text = String::from_utf8_lossy(&bytes);
        let field = |key| {
            ini_field(&text, "FLAGS", key).map(|s| s.split(';').next().unwrap_or_default().trim())
        };
        let dimension = |key, default| -> Result<u32> {
            field(key).map_or(Ok(default), |value| {
                value
                    .parse()
                    .with_context(|| format!("invalid [FLAGS] {key}"))
            })
        };
        let grid = [
            dimension("TextureCountColumn", 4)?,
            dimension("TextureCountRow", 8)?,
        ];
        ensure!(
            grid.into_iter().all(|size| (1..=64).contains(&size)),
            "invalid Flag atlas grid"
        );
        let name = field("TextureName").unwrap_or("flagall.tga");
        let texture = game
            .load_texture(texture_paths, name)?
            .with_context(|| format!("missing Flag texture {name}"))?;
        ensure!(
            texture.header.width >= grid[0] && texture.header.height >= grid[1],
            "Flag atlas is smaller than its grid"
        );
        Ok(Self {
            grid,
            texture: Rc::new(texture),
        })
    }
}

#[derive(Clone, Copy)]
enum Host {
    Ship(EntityId),
    Town,
}

struct Group {
    revision: u64,
    asset: Rc<ModelAsset>,
    location: Host,
    nation: i32,
    pending: bool,
    cells: BTreeMap<usize, FlagTexture>,
}

#[derive(Clone, Default)]
pub(crate) struct Flags(Rc<RefCell<BTreeMap<u64, BTreeMap<u64, Group>>>>);

impl Flags {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let flags = Self::default();
        let shared = flags.clone();
        let models = host.models.clone();
        let weather = host.weather.clone();
        let paths = host.texture_paths.clone();
        host.entities.register_class("Flag", move || {
            Ok(Box::new(Actor {
                id: EntityId::INVALID,
                flags: shared.clone(),
                models: models.clone(),
                weather: weather.clone(),
                atlas: Atlas::load(&game, &paths.borrow())
                    .context("load Flag settings and material")?,
            }))
        })?;
        host.flags = Some(flags);
        Ok(())
    }

    pub(crate) fn take_requests(&self, owner: EntityId) -> Vec<Request> {
        let mut states = self.0.borrow_mut();
        let mut requests = Vec::new();
        if let Some(groups) = states.get_mut(&owner.raw()) {
            for (&model, group) in groups.iter_mut().filter(|(_, group)| group.pending) {
                group.pending = false;
                for (mesh, data) in group.asset.meshes.iter().enumerate() {
                    if let Some(RigMesh::Flag(flag)) = &data.rigging {
                        requests.push(Request {
                            owner: owner.raw(),
                            model,
                            asset: group.asset.clone(),
                            location: group.location,
                            nation: group.nation,
                            mesh,
                            triangle: flag.is_triangle(),
                            special: flag.origin.as_ref().is_some_and(|origin| origin.special),
                        });
                    }
                }
            }
        }
        requests
    }

    pub(crate) fn apply_reply(&self, request: Request, reply: Option<Value>) -> Result<()> {
        let mut states = self.0.borrow_mut();
        let Some(group) = states
            .get_mut(&request.owner)
            .and_then(|groups| groups.get_mut(&request.model))
            .filter(|group| Rc::ptr_eq(&group.asset, &request.asset))
        else {
            return Ok(());
        };
        let selected = match reply.as_ref().map(Value::resolved).transpose()? {
            None => FlagTexture::from_script_reply(None)?,
            Some(Value::Array(values)) => FlagTexture::from_script_reply(Some(&[
                values.element_value(0)?.resolved()?.integer()?,
                values.element_value(1)?.resolved()?.integer()?,
            ]))?,
            Some(value) => FlagTexture::from_script_reply(Some(&[value.integer()?]))?,
        };
        let Some(RigMesh::Flag(flag)) = &group.asset.meshes[request.mesh].rigging else {
            bail!("missing flag mesh for atlas reply");
        };
        let [columns, rows] = flag.atlas_size();
        ensure!(
            matches!(selected, FlagTexture::Hidden)
                || matches!(selected, FlagTexture::Cell(cell) if cell.column < columns && cell.row < rows),
            "GetRiggingData returned a cell outside the Flag atlas"
        );
        group.cells.insert(request.mesh, selected);
        Ok(())
    }
}

pub(crate) struct Request {
    owner: u64,
    model: u64,
    asset: Rc<ModelAsset>,
    location: Host,
    mesh: usize,
    nation: i32,
    triangle: bool,
    special: bool,
}

impl Request {
    pub(crate) fn message(&self, entities: &EntityManager) -> Result<Option<Message>> {
        let mut fields = vec![
            Field::String(
                match self.location {
                    Host::Ship(_) => "GetShipFlagTexNum",
                    Host::Town => "GetTownFlagTexNum",
                }
                .into(),
            ),
            Field::Long(i32::from(self.triangle)),
            Field::Long(self.nation),
            Field::Long(i32::from(self.special)),
        ];
        let format = match self.location {
            Host::Town => "slll",
            Host::Ship(ship) => {
                let Some(attributes) = entities.attribute_root(ship) else {
                    return Ok(None);
                };
                fields.push(Field::Attribute(attributes));
                "sllla"
            }
        };
        Ok(Some(Message::new(format, fields)?))
    }
}

struct Actor {
    id: EntityId,
    flags: Flags,
    models: Option<Models>,
    weather: Option<Weather>,
    atlas: Atlas,
}

impl Actor {
    fn attach(&self, model: EntityId, nation: i32, location: Host) -> Result<()> {
        let models = self
            .models
            .as_ref()
            .context("Flag requires the MODELR service")?;
        if models.loaded(model)?.is_none() {
            return Ok(());
        }
        let (revision, asset) =
            models.load_flags(model, self.atlas.grid, self.atlas.texture.clone())?;
        let mut states = self.flags.0.borrow_mut();
        let groups = states
            .get_mut(&self.id.raw())
            .context("missing Flag actor state")?;
        if asset.meshes.is_empty() {
            groups.remove(&model.raw());
        } else {
            groups.insert(
                model.raw(),
                Group {
                    revision,
                    asset,
                    location,
                    nation,
                    pending: true,
                    cells: BTreeMap::new(),
                },
            );
        }
        Ok(())
    }
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        self.flags
            .0
            .borrow_mut()
            .insert(self.id.raw(), BTreeMap::new());
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            code @ (INIT_SHIP | INIT_TOWN) => {
                let model = message.entity()?;
                let nation = message.long()?;
                let location = match code {
                    INIT_TOWN => Host::Town,
                    _ if message.current_type().is_some() => Host::Ship(message.entity()?),
                    _ => Host::Ship(model),
                };
                self.attach(model, nation, location)?;
            }
            DEL_GROUP => {
                let model = message.entity()?;
                if let Some(groups) = self.flags.0.borrow_mut().get_mut(&self.id.raw()) {
                    groups.remove(&model.raw());
                }
            }
            code => bail!("unported Flag message {code}"),
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, _: u32) -> Result<()> {
        if stage != Stage::Realize {
            return Ok(());
        }
        let Some(models) = &self.models else {
            return Ok(());
        };
        let mut states = self.flags.0.borrow_mut();
        let groups = states
            .get_mut(&self.id.raw())
            .context("missing Flag actor state")?;
        let (angle, speed) = self
            .weather
            .as_ref()
            .and_then(Weather::wind)
            .unwrap_or((0.0, 12.0));
        let wind = SailWind {
            direction: std::f32::consts::PI - angle,
            speed: speed.clamp(0.0, 40.0),
        };
        let mut retired = Vec::new();
        for (&model, group) in groups.iter() {
            if let Some(draw) = models
                .loaded(EntityId::from_raw(model))?
                .filter(|draw| draw.revision == group.revision)
            {
                let frame = NativeRigFrame {
                    flags: group.cells.clone(),
                    flag_wind: Some(wind),
                    ..Default::default()
                };
                models.queue_draw(crate::model::Draw {
                    asset: group.asset.clone(),
                    rig: Some(Arc::new(frame)),
                    part: DrawPart::Flags,
                    ..draw
                })?;
            } else {
                retired.push(model);
            }
        }
        for model in retired {
            groups.remove(&model);
        }
        Ok(())
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.flags.0.borrow_mut().remove(&self.id.raw());
        Ok(())
    }
}
