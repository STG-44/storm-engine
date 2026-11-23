//! Configured SAIL ownership and its MODELR-based cloth draws.
use crate::{
    GameDirectory, NativeHost, config::ini_field, model::Models, random::Random, weather::Weather,
};
use anyhow::{Context, Result, bail, ensure};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityId, EntityInit, Message, ScriptScalar,
    ScriptVariable, Stage,
};
use storm_render::model::{
    ModelAsset, NativeRigFrame, SailDamageSettings, SailPatch, native_model_matrix,
};
use storm_sim::{
    sail_drive::{self, Coefficients, Group, Kind},
    sail_geometry::{self, Vertex},
    sail_motion::SailMotion,
    sail_roll,
    sail_topology::{self, Grid, Topology},
    sail_wave,
};

const INIT: i32 = 40000;
const GET_SPEED: i32 = 40002;
const GET_PARAMS: i32 = 40024;
const DEL_GROUP: i32 = 40030;

struct Settings {
    coefficients: Coefficients,
    damage: SailDamageSettings,
    wave: sail_wave::Settings,
    roll: sail_roll::Parameters,
    rate: f32,
    maximum_sum_wind: f32,
    table: Vec<f32>,
}

impl Settings {
    fn load(game: &GameDirectory) -> Result<Self> {
        let bytes = game.read("RESOURCE/INI/rigging.ini", 1024 * 1024)?;
        let text = String::from_utf8_lossy(&bytes);
        let field = |key| ini_field(&text, "SAILS", key);
        let float = |key, default| -> Result<f32> {
            field(key).map_or(Ok(default), |value| parse_float(key, value))
        };
        let integer = |key, default| -> Result<u32> {
            field(key).map_or(Ok(default), |value| {
                value
                    .parse()
                    .with_context(|| format!("invalid [SAILS] {key}"))
            })
        };
        let count = integer("WINDVECTOR_QNT", 60)?;
        ensure!(
            (1..=65536).contains(&count),
            "invalid [SAILS] WINDVECTOR_QNT"
        );
        let rate = float("ROLLINGSPEED", 0.0003)?;
        let maximum_sum_wind = float("MAXSUMWIND", 0.02)?;
        ensure!(
            rate >= 0.0 && maximum_sum_wind >= 0.0,
            "negative sail motion setting"
        );
        let coefficients = Coefficients::default();
        let wave = sail_wave::Settings::default();
        let roll = sail_roll::Parameters::default();
        Ok(Self {
            coefficients: Coefficients {
                triangle: parse_list(
                    "TreangleWindSpeed",
                    field("TreangleWindSpeed"),
                    coefficients.triangle,
                )?,
                trapezoid: parse_list(
                    "TrapecidalWindSpeed",
                    field("TrapecidalWindSpeed"),
                    coefficients.trapezoid,
                )?,
                square: parse_list(
                    "SquareWindSpeed",
                    field("SquareWindSpeed"),
                    coefficients.square,
                )?,
            },
            damage: SailDamageSettings::from_ini(&text)?,
            wave: sail_wave::Settings {
                square_dependence: float("fSsailWindDepend", wave.square_dependence)?,
                triangle_dependence: float("fTsailWindDepend", wave.triangle_dependence)?,
                wind_adding: float("fWindAdding", wave.wind_adding)?,
                triangle_hole_flex: float("fTHoleFlexDepend", wave.triangle_hole_flex)?,
                flex_speed: float("FLEXSPEED", wave.flex_speed)?,
                square_increment: integer("WINDVECTOR_SINCR", wave.square_increment)?,
                triangle_increment: integer("WINDVECTOR_TINCR", wave.triangle_increment)?,
                square_add: integer("WINDVECTOR_SADD", wave.square_add)?,
                triangle_add: integer("WINDVECTOR_TADD", wave.triangle_add)?,
            },
            roll: sail_roll::Parameters {
                z: float("ROLL_Z_VAL", roll.z)?,
                y: float("ROLL_Y_VAL", roll.y)?,
                wind_delta: float("ROLL_Z_DELTA", roll.wind_delta)?,
                triangle_multiplier: float("tr_form_mul", roll.triangle_multiplier)?,
                square_form: parse_list("rollSSailForm", field("rollSSailForm"), roll.square_form)?,
                triangle_form: parse_list(
                    "rollTSailForm",
                    field("rollTSailForm"),
                    roll.triangle_form,
                )?,
                ..roll
            },
            rate,
            maximum_sum_wind,
            table: (0..count)
                .map(|i| (i as f32 / count as f32 * 2.0 * std::f32::consts::PI).sin())
                .collect(),
        })
    }
}

fn parse_float(key: &str, value: &str) -> Result<f32> {
    let number: f32 = value
        .trim()
        .parse()
        .with_context(|| format!("invalid [SAILS] {key}"))?;
    ensure!(number.is_finite(), "non-finite [SAILS] {key}");
    Ok(number)
}

fn parse_list<const N: usize>(
    key: &str,
    value: Option<&str>,
    mut numbers: [f32; N],
) -> Result<[f32; N]> {
    if let Some(value) = value {
        let mut fields = value.split(',');
        for number in &mut numbers {
            *number = parse_float(
                key,
                fields
                    .next()
                    .with_context(|| format!("too few values in [SAILS] {key}"))?,
            )?;
        }
        ensure!(fields.next().is_none(), "too many values in [SAILS] {key}");
    }
    Ok(numbers)
}

struct Cloth {
    mesh: usize,
    turning: bool,
    motion: SailMotion,
    input: sail_wave::Input,
    wave: sail_wave::State,
    vertices: Vec<Vertex>,
    full_indices: Topology,
    rolled_indices: Topology,
}

impl Cloth {
    fn new(
        mesh: usize,
        patch: &SailPatch,
        kind: Kind,
        up: bool,
        settings: &Settings,
        rng: &mut Random,
    ) -> Result<Self> {
        let turning = patch
            .origin
            .as_ref()
            .is_some_and(|origin| origin.node.starts_with("rey_"));
        let mut motion = SailMotion::new(kind, !up);
        motion.hard_points = patch.native_points();
        let vertical = rng.next() as usize % settings.table.len();
        let horizontal = rng.next() as usize % settings.table.len();
        let limit = patch.native_limit_point();
        let parameters = sail_geometry::sample_parameters(
            kind,
            motion.hard_points,
            turning,
            0,
            settings.damage.quad_flex,
            limit,
            || rng.unit(),
        )?;
        let sphere = sail_geometry::rebuild(&mut motion, parameters)?;
        let (_, height) = sail_geometry::dimensions(kind, motion.hard_points)?;
        let maximum_sum_wind = height * settings.maximum_sum_wind;
        let wave = sail_wave::State {
            vertical,
            horizontal,
            wind_up: (rng.next() & 1) == 1,
            sum_wind: maximum_sum_wind * rng.unit(),
            maximum_sum_wind,
            ..Default::default()
        };
        let vertices = sail_geometry::fill(&motion, sphere, [1.0; 2])?;
        Ok(Self {
            mesh,
            turning,
            motion,
            input: sail_wave::Input {
                parameters,
                sphere,
                height,
                local_wind: [0.0; 3],
                wind_base: 0.4,
                limit_point: limit.is_some(),
                rows: Grid::NEAR.rows,
                columns: Grid::NEAR.columns,
                ties: [false; 4],
            },
            wave,
            vertices,
            full_indices: sail_topology::fill(kind, false, Grid::NEAR, 0)?,
            rolled_indices: sail_topology::fill(kind, true, Grid::ROLLED, 0)?,
        })
    }

    fn deform(&mut self, settings: &Settings, wind: [f32; 3], base: f32, delta: u32) -> Result<()> {
        if self.motion.rolling {
            let increment = if self.motion.kind == Kind::Triangle {
                settings.wave.triangle_increment
            } else {
                settings.wave.square_increment
            };
            sail_roll::advance(
                &mut self.wave,
                &self.motion,
                &mut self.vertices,
                sail_roll::Parameters {
                    height: self.input.height,
                    turning: self.turning,
                    ..settings.roll
                },
                &settings.table,
                increment,
            )
        } else {
            sail_wave::advance(
                &mut self.wave,
                &self.motion,
                &mut self.vertices,
                sail_wave::Input {
                    local_wind: wind,
                    wind_base: base,
                    ..self.input
                },
                &settings.table,
                settings.wave,
                delta,
            )
        }
    }
}

struct Attachment {
    model: EntityId,
    revision: u64,
    asset: Rc<ModelAsset>,
    drive: Group,
    cloth: Vec<Cloth>,
    frame: Arc<NativeRigFrame>,
}

impl Attachment {
    fn update(&mut self, settings: &Settings, wind: [f32; 3], base: f32, delta: u32) -> Result<()> {
        for (cloth, sail) in self.cloth.iter_mut().zip(&mut self.drive.sails) {
            cloth.motion.advance(delta, settings.rate)?;
            sail.rolling = cloth.motion.rolling;
            sail.rolling_delta = cloth.motion.transition.map(|t| t.delta);
        }
        let outputs = self.drive.evaluate(wind, settings.coefficients)?;
        let mut frame = NativeRigFrame::default();
        for (cloth, output) in self.cloth.iter_mut().zip(outputs) {
            cloth.deform(settings, output.wind, base, delta)?;
            frame.sails.insert(cloth.mesh, cloth.motion.deployment());
            frame.cloth.insert(cloth.mesh, cloth.vertices.clone());
            let indices = if cloth.motion.rolling {
                &cloth.rolled_indices
            } else {
                &cloth.full_indices
            };
            frame.cloth_indices.insert(cloth.mesh, indices.clone());
        }
        self.frame = Arc::new(frame);
        Ok(())
    }
}

pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
    let models = host.models.clone();
    let weather = host.weather.clone();
    let rng = host.random.clone();
    host.entities.register_class("Sail", move || {
        Ok(Box::new(Actor {
            settings: Settings::load(&game).context("load Sail settings")?,
            models: models.clone(),
            weather: weather.clone(),
            rng: rng.clone(),
            wind: (0.0, 4.8),
            minimum: 0.0,
            groups: Vec::new(),
        }))
    })?;
    Ok(())
}

struct Actor {
    settings: Settings,
    models: Option<Models>,
    weather: Option<Weather>,
    rng: Rc<RefCell<Random>>,
    wind: (f32, f32),
    minimum: f32,
    groups: Vec<Attachment>,
}

impl Actor {
    fn attach(&mut self, ship: EntityId, model: EntityId, up: bool) -> Result<()> {
        ensure!(
            !self
                .groups
                .iter()
                .any(|g| !g.drive.deleted && (g.drive.ship == ship.raw() || g.model == model)),
            "duplicate active Sail group"
        );
        let models = self
            .models
            .as_ref()
            .context("Sail requires the MODELR service")?;
        let (revision, asset) = models.load_sails(model, self.settings.damage)?;
        let model_draw = models.loaded(model)?.context("missing loaded Sail model")?;
        let mut drive = Group {
            ship: ship.raw(),
            deleted: false,
            current_set: if up { 2 } else { 0 },
            speed: 0.0,
            maximum: 0.0,
            reference: 0.0,
            speed_multiplier: 1.0,
            sails: Vec::with_capacity(asset.sails),
        };
        let mut cloth = Vec::with_capacity(asset.sails);
        let mut rng = self.rng.borrow_mut();
        for (mesh, data) in asset.meshes.iter().enumerate() {
            let Some(patch) = &data.sail else { continue };
            let sail = patch.native_drive(0);
            cloth.push(Cloth::new(
                mesh,
                patch,
                sail.kind,
                up,
                &self.settings,
                &mut rng,
            )?);
            drive.reference += sail.maximum;
            drive.sails.push(sail);
        }
        let mut group = Attachment {
            model,
            revision,
            asset,
            drive,
            cloth,
            frame: Arc::default(),
        };
        let (wind, base) = model_wind(self.wind, model_draw.matrix)?;
        group.update(&self.settings, wind, base, 0)?;
        group.drive.speed = 0.0; // First measured speed becomes available at Execute.
        self.groups.retain(|group| {
            !group.drive.deleted || (group.drive.ship != ship.raw() && group.model != model)
        });
        self.groups.push(group);
        Ok(())
    }

    fn speed(&mut self, ship: EntityId, output: ScriptVariable, request: f32) -> Result<()> {
        let group = self.groups.iter_mut().find(|g| g.drive.ship == ship.raw());
        let mut drive = group.as_ref().map(|g| g.drive.clone());
        let mut speed = 0.0;
        let requests = sail_drive::request_speed(
            drive.as_mut_slice(),
            ship.raw(),
            Some(&mut speed),
            request,
            self.minimum,
        )?;
        output.set(ScriptScalar::Float(speed))?;
        if let (Some(group), Some(drive)) = (group, drive) {
            for request in requests {
                group.cloth[request.sail]
                    .motion
                    .set_rolling_fixed_yard(request.roll_up)?;
            }
            group.drive = drive;
        }
        Ok(())
    }
}

fn model_wind((angle, speed): (f32, f32), matrix: [f32; 16]) -> Result<([f32; 3], f32)> {
    let matrix = native_model_matrix(matrix)?;
    let world = [angle.sin(), 0.0, -angle.cos()].into();
    let wind = [matrix.x_axis, matrix.y_axis, matrix.z_axis].map(|axis| axis.truncate().dot(world));
    Ok((wind, (speed / 12.0).min(1.0)))
}

// Legacy native callers supply a float pointer. The script bridge can provide
// a checked variable handle or a null pointer, which the original ignored.
fn output(message: &mut Message) -> Result<Option<ScriptVariable>> {
    if message.current_type() == Some('e') {
        let output = message.script_variable()?;
        output.float()?;
        return Ok(Some(output));
    }
    ensure!(
        message.pointer()? == 0,
        "Sail float output requires a checked variable"
    );
    Ok(None)
}

impl Entity for Actor {
    fn init(&mut self, _: &mut EntityInit<'_>) -> Result<bool> {
        Ok(true)
    }

    fn attribute_changed(&mut self, store: &AttributeStore, id: AttributeId) -> Result<u32> {
        if store
            .name(id)
            .is_some_and(|name| name.eq_ignore_ascii_case("MinSpeed"))
        {
            let value = store.as_float(id, None, 0.0);
            ensure!(value.is_finite(), "non-finite Sail.MinSpeed");
            self.minimum = value;
        }
        Ok(0)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            INIT => self.attach(message.entity()?, message.entity()?, message.long()? != 0)?,
            GET_SPEED => {
                let ship = message.entity()?;
                let output = output(message)?;
                let request = message.float()?;
                if let Some(output) = output {
                    self.speed(ship, output, request)?;
                }
            }
            GET_PARAMS => {
                let ship = message.entity()?;
                if let Some(output) = output(message)? {
                    let maximum = self
                        .groups
                        .iter()
                        .find(|g| g.drive.ship == ship.raw())
                        .map_or(0.0, |g| g.drive.maximum);
                    output.set(ScriptScalar::Float(maximum))?;
                }
            }
            DEL_GROUP => {
                let ship = message.entity()?;
                if let Some(group) = self.groups.iter_mut().find(|g| g.drive.ship == ship.raw()) {
                    group.drive.deleted = true;
                }
            }
            code => bail!("unported Sail message {code}"),
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, delta: u32) -> Result<()> {
        let Some(models) = &self.models else {
            return Ok(());
        };
        match stage {
            Stage::Execute => {
                if let Some(wind) = self.weather.as_ref().and_then(Weather::wind) {
                    self.wind = wind;
                }
                for group in &mut self.groups {
                    if group.drive.deleted {
                        continue;
                    }
                    let draw = models.loaded(group.model)?;
                    if let Some(draw) = draw.filter(|draw| draw.revision == group.revision) {
                        let (wind, base) = model_wind(self.wind, draw.matrix)?;
                        group.update(&self.settings, wind, base, delta)?;
                    } else {
                        group.drive.deleted = true;
                    }
                }
                self.groups.retain(|group| !group.drive.deleted);
            }
            Stage::Realize => {
                for group in self
                    .groups
                    .iter()
                    .filter(|g| !g.drive.deleted && !g.cloth.is_empty())
                {
                    if let Some(draw) = models
                        .loaded(group.model)?
                        .filter(|draw| draw.revision == group.revision)
                    {
                        models.queue_draw(crate::model::Draw {
                            asset: group.asset.clone(),
                            rig: Some(group.frame.clone()),
                            ..draw
                        })?;
                    }
                }
            }
        }
        Ok(())
    }
}
