//! Script-bound SHIP mounting and living motion over the configured SEA.
use crate::{
    NativeHost,
    model::{LOAD_GEOMETRY, Models},
    random::Random,
    sea::{Seas, Surface},
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityDestroy, EntityId, EntityInit, EntityManager,
    Message, ScriptScalar, ScriptVariable, ScriptVariableAccess, Stage, Value as Field,
};
use storm_render::texture_paths::TexturePaths;
use storm_sim::{
    ship_cleanup::{self, Links},
    ship_motion::{FlatMovement, MSG_SHIP_ADD_MOVE_IMPULSE, MovementPort, ShipMotion},
    ship_parameters::ShipParameters,
    ship_placement::{RootPose, ShipPlacement},
    ship_presentation,
    ship_queries::{MSG_SHIP_GET_CHARACTER_INDEX, MSG_SHIP_GET_NUM_FIRE_PLACES},
    ship_rocking::{RockingInput, SeaSurface, ShipRocking},
    ship_sails::{MSG_SHIP_GET_SAIL_STATE, MSG_SHIP_SET_SAIL_STATE, ShipSailState},
    ship_stages::Priorities,
};

pub(crate) const CREATE: i32 = 50300;
const SET_LAYERS: i32 = 51015;
const GET_SAIL_SPEED: i32 = 40002;

#[derive(Clone)]
pub(crate) struct Ships(Rc<Shared>);
struct Shared {
    models: Option<Models>,
    seas: Option<Seas>,
    random: Rc<RefCell<Random>>,
    texture_paths: Rc<RefCell<TexturePaths>>,
    states: RefCell<BTreeMap<u64, Rc<RefCell<State>>>>,
}

struct State {
    layers: [u8; 2],
    sail: ShipSailState,
    tick: f32,
    pending: Option<Mount>,
    body: Option<Mounted>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            layers: [0, 1],
            sail: ShipSailState::default(),
            tick: 0.0,
            pending: None,
            body: None,
        }
    }
}
struct Mount {
    character: AttributeId,
    definition: AttributeId,
    step: MountStep,
    texture_paths: Option<TexturePaths>,
}
#[derive(Clone, Copy)]
enum MountStep {
    Start,
    Layers,
    Geometry,
    End,
    Finish,
}
struct Mounted {
    character: AttributeId,
    definition: AttributeId,
    sea: EntityId,
    links: Links,
    motion: ShipMotion,
    placement: ShipPlacement,
    rocking: ShipRocking,
    index: i32,
    fire_count: usize,
    box_size: [f32; 3],
}

impl Ships {
    pub(crate) fn register(host: &mut NativeHost) -> Result<()> {
        let ships = Self(Rc::new(Shared {
            models: host.models.clone(),
            seas: host.seas.clone(),
            random: host.random.clone(),
            texture_paths: host.texture_paths.clone(),
            states: RefCell::default(),
        }));
        let factory = ships.clone();
        if host.entities.register_class("ship", move || {
            Ok(Box::new(Actor {
                ships: factory.clone(),
                state: Rc::default(),
                id: EntityId::INVALID,
            }))
        })? {
            host.ships = Some(ships);
        }
        Ok(())
    }

    fn state(&self, ship: EntityId) -> Option<Rc<RefCell<State>>> {
        self.0.states.borrow().get(&ship.raw()).cloned()
    }

    fn models(&self) -> Result<&Models> {
        self.0
            .models
            .as_ref()
            .context("SHIP requires the MODELR service")
    }

    fn surface(&self, manager: &EntityManager, sea: EntityId) -> Option<Surface> {
        manager.is_valid(sea).then_some(())?;
        self.0.seas.as_ref()?.surface(sea)
    }

    pub(crate) fn mounting(&self, ship: EntityId) -> bool {
        self.state(ship)
            .is_some_and(|state| state.borrow().pending.is_some())
    }

    pub(crate) fn next_callback(
        &self,
        ship: EntityId,
        manager: &mut EntityManager,
        store: &mut AttributeStore,
    ) -> Result<Option<(&'static str, Message)>> {
        if !manager.is_valid(ship) {
            self.abort_mount(ship, manager)?;
            return Ok(None);
        }
        let state = self.state(ship).context("missing SHIP mount state")?;
        let mut state = state.borrow_mut();
        let Some(mount) = &mut state.pending else {
            return Ok(None);
        };
        let character = mount.character;
        let definition = mount.definition;
        let event = match mount.step {
            MountStep::Start => {
                mount.texture_paths = Some(self.0.texture_paths.borrow().clone());
                mount.step = MountStep::Layers;
                (
                    "Ship_StartLoad",
                    Message::new("a", vec![Field::Attribute(character)])?,
                )
            }
            MountStep::Layers => {
                mount.step = MountStep::Geometry;
                (
                    "SeaGetLayers",
                    Message::new("i", vec![Field::Entity(ship)])?,
                )
            }
            MountStep::Geometry => {
                mount.step = MountStep::End;
                let body = self.load_ship(manager, store, character, definition)?;
                let index = body.index;
                let links = body.links;
                state.body = Some(body);
                let [execute, realize] = state.layers;
                Priorities::from_script(None, None).register(manager, ship, execute, realize)?;
                self.attach_rig(
                    manager,
                    store,
                    links,
                    ship,
                    character,
                    state.sail.get() != 0.0,
                )?;
                (
                    "ShpCreate",
                    Message::new("li", vec![Field::Long(index), Field::Entity(ship)])?,
                )
            }
            MountStep::End => {
                mount.step = MountStep::Finish;
                (
                    "Ship_EndLoad",
                    Message::new("a", vec![Field::Attribute(character)])?,
                )
            }
            MountStep::Finish => {
                let body = state.body.as_mut().context("SHIP mount lost its model")?;
                body.finish(self.models()?, store, self.surface(manager, body.sea))?;
                state.pending = None;
                return Ok(None);
            }
        };
        Ok(Some(event))
    }

    fn load_ship(
        &self,
        manager: &mut EntityManager,
        store: &mut AttributeStore,
        character: AttributeId,
        definition: AttributeId,
    ) -> Result<Mounted> {
        let name = store
            .value(definition, Some("name"))
            .context("missing SHIP model name")?;
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\', ':']),
            "invalid SHIP model name"
        );
        let file = format!("ships/{name}/{name}");
        let parameters = ShipParameters::load(store, definition)?;
        let model = manager.create("MODELR", store, None)?;
        let result = (|| {
            manager.send_message(
                model,
                &mut Message::new("ls", vec![Field::Long(LOAD_GEOMETRY), Field::String(file)])?,
            )?;
            let asset = self
                .models()?
                .loaded(model)?
                .context("SHIP model has no geometry")?
                .asset;
            let root_size = asset
                .root_box_size
                .context("missing SHIP root dimensions")?;
            let box_size = asset
                .native_box_size
                .context("missing assembled SHIP dimensions")?;
            let mut motion = ShipMotion::default();
            let mut placement = ShipPlacement::default();
            parameters.configure(&mut motion.state, &mut placement, root_size)?;
            let lookup = |class| manager.first_id(class).unwrap_or(EntityId::INVALID);
            Ok(Mounted {
                character,
                definition,
                motion,
                placement,
                box_size,
                index: store.as_dword(character, Some("index"), u32::MAX) as i32,
                fire_count: asset.ship.fires.len(),
                rocking: ShipRocking::new([root_size[0], root_size[2]]),
                sea: lookup("sea"),
                links: Links {
                    model,
                    sail: lookup("sail"),
                    rope: lookup("rope"),
                    flag: lookup("flag"),
                    vants: ["vant", "vantl", "vantz"].map(lookup),
                    blots: EntityId::INVALID,
                },
            })
        })();
        if result.is_err() {
            manager.erase(model);
        }
        result
    }

    fn attach_rig(
        &self,
        manager: &mut EntityManager,
        store: &AttributeStore,
        links: Links,
        ship: EntityId,
        character: AttributeId,
        sail_up: bool,
    ) -> Result<()> {
        let owner = Field::Entity(ship);
        let model = Field::Entity(links.model);
        manager.send_message(
            links.sail,
            &mut Message::new(
                "liil",
                vec![
                    Field::Long(40000),
                    owner.clone(),
                    model.clone(),
                    Field::Long(i32::from(sail_up)),
                ],
            )?,
        )?;
        manager.send_message(
            links.rope,
            &mut Message::new(
                "lii",
                vec![Field::Long(40100), owner.clone(), model.clone()],
            )?,
        )?;
        let nation = store.as_dword(character, Some("nation"), 0) as i32;
        manager.send_message(
            links.flag,
            &mut Message::new(
                "lili",
                vec![
                    Field::Long(40200),
                    model.clone(),
                    Field::Long(nation),
                    owner.clone(),
                ],
            )?,
        )?;
        for id in links.vants {
            manager.send_message(
                id,
                &mut Message::new(
                    "lii",
                    vec![Field::Long(40300), owner.clone(), model.clone()],
                )?,
            )?;
        }
        Ok(())
    }

    pub(crate) fn abort_mount(&self, ship: EntityId, manager: &mut EntityManager) -> Result<()> {
        let Some(state) = self.state(ship) else {
            return Ok(());
        };
        let mut state = state.borrow_mut();
        if let Some(mount) = state.pending.take() {
            if let Some(paths) = mount.texture_paths {
                self.0.texture_paths.replace(paths);
            }
            if let Some(body) = state.body.take() {
                let result = cleanup(manager, ship, body.links);
                for layer in state.layers {
                    manager.remove_from_layer(layer, ship)?;
                }
                result?;
            }
        }
        Ok(())
    }

    pub(crate) fn update_event(
        &self,
        ship: EntityId,
        delta_ms: u32,
        store: &AttributeStore,
    ) -> Result<Option<Message>> {
        let Some(state) = self.state(ship) else {
            return Ok(None);
        };
        let mut state = state.borrow_mut();
        if state.pending.is_some() || state.body.is_none() {
            return Ok(None);
        }
        state.tick -= (delta_ms as f32 * 0.001).min(0.1);
        if state.tick > 0.0 {
            return Ok(None);
        }
        // DTimer consumes a CRT draw even when the interval is fixed.
        self.0.random.borrow_mut().scaled(0.0);
        state.tick = 1.0;
        let body = state.body.as_ref().expect("mounted ship");
        if stopped(store, body.character)? {
            return Ok(None);
        }
        Ok(Some(Message::new(
            "lf",
            vec![Field::Long(body.index), Field::Float(state.sail.get())],
        )?))
    }

    pub(crate) fn execute(
        &self,
        ship: EntityId,
        delta_ms: u32,
        manager: &mut EntityManager,
        store: &mut AttributeStore,
    ) -> Result<()> {
        let Some(state) = self.state(ship) else {
            return Ok(());
        };
        let mut state = state.borrow_mut();
        if !manager.is_valid(ship) || state.pending.is_some() {
            return Ok(());
        }
        let sail_request = state.sail.get();
        let Some(body) = &mut state.body else {
            return Ok(());
        };
        let sea = self.surface(manager, body.sea);
        body.execute(
            ship,
            sail_request,
            delta_ms,
            manager,
            store,
            self.models()?,
            sea,
        )
    }

    pub(crate) fn owns(&self, id: EntityId) -> bool {
        self.0.states.borrow().contains_key(&id.raw())
    }
}

impl Mounted {
    fn finish(
        &mut self,
        models: &Models,
        store: &mut AttributeStore,
        mut sea: Option<Surface>,
    ) -> Result<()> {
        let position = store.lookup_path(self.character, "Ship.Pos")?;
        let angles = store.lookup_path(self.character, "Ship.Ang")?;
        let read = |root: Option<AttributeId>, axis| {
            root.map_or(0.0, |id| store.as_float(id, Some(axis), 0.0))
        };
        let x = read(position, "x");
        let z = read(position, "z");
        let y = sea.as_mut().map_or(Ok(0.0), |sea| sea.wave_xz(x, z))?;
        self.motion.state.position = [x, y, z];
        self.motion.state.angles[1] = read(angles, "y");
        self.placement.calculate_immersion(store, self.character)?;
        self.set_pose(
            models,
            RootPose {
                position: self.motion.state.position,
                angles: self.motion.state.angles,
            },
        )?;
        let root = store.create_path(self.character, "Ship.BoxSize")?;
        for (axis, size) in ["x", "y", "z"].into_iter().zip(self.box_size) {
            store.set_float(root, Some(axis), size)?;
        }
        Ok(())
    }

    fn set_pose(&self, models: &Models, pose: RootPose) -> Result<()> {
        let pose =
            self.placement
                .root(pose.position, pose.angles, self.motion.offsets, [0.0; 2])?;
        models.set_matrix(self.links.model, pose.matrix())
    }

    #[allow(clippy::too_many_arguments)]
    fn execute(
        &mut self,
        ship: EntityId,
        requested_sail: f32,
        delta_ms: u32,
        manager: &mut EntityManager,
        store: &mut AttributeStore,
        models: &Models,
        mut sea: Option<Surface>,
    ) -> Result<()> {
        self.placement.calculate_immersion(store, self.character)?;
        if let Some(id) = store.child(self.definition, "Rocking") {
            self.rocking.parameters.height_factor = store.as_float(id, Some("y"), 1.0);
            self.rocking.parameters.angle_factor = store.as_float(id, Some("az"), 1.0);
        }
        let demand = Rc::new(Cell::new(self.motion.strengths[0].speed[2]));
        manager.send_message(
            self.links.sail,
            &mut Message::new(
                "lief",
                vec![
                    Field::Long(GET_SAIL_SPEED),
                    Field::Entity(ship),
                    Field::ScriptVariable(ScriptVariable::new(FloatOutput(demand.clone()))),
                    Field::Float(requested_sail),
                ],
            )?,
        )?;
        self.motion.strengths[0].speed[2] = demand.get();
        let mut movement = Movement {
            limits: FlatMovement::from_character(store, self.character)?,
            rocking: &mut self.rocking,
            sea: sea.as_mut(),
        };
        let pose = ship_presentation::advance_living(&mut self.motion, delta_ms, &mut movement)?;
        self.set_pose(models, pose)?;
        ship_presentation::publish_character(store, self.character, &self.motion)
    }
}

struct FloatOutput(Rc<Cell<f32>>);
impl ScriptVariableAccess for FloatOutput {
    fn read(&self) -> Result<ScriptScalar> {
        Ok(ScriptScalar::Float(self.0.get()))
    }
    fn write(&self, value: ScriptScalar) -> Result<()> {
        let ScriptScalar::Float(value) = value else {
            bail!("sail speed requires float output")
        };
        self.0.set(value);
        Ok(())
    }
}

struct Movement<'a> {
    limits: FlatMovement,
    rocking: &'a mut ShipRocking,
    sea: Option<&'a mut Surface>,
}
impl MovementPort for Movement<'_> {
    fn max_speed_z(&mut self) -> f32 {
        self.limits.max_speed_z
    }
    fn max_speed_y(&mut self) -> f32 {
        self.limits.max_speed_y
    }
    fn rocking(
        &mut self,
        position: [f32; 3],
        angles: [f32; 3],
        delta: f32,
    ) -> Result<(f32, [f32; 3])> {
        let state = self.rocking.advance(
            RockingInput {
                position,
                angles,
                previous_angles: angles,
            },
            delta,
            self.sea
                .as_deref_mut()
                .map(|sea| sea as &mut dyn SeaSurface),
        )?;
        Ok((state.height, state.angles))
    }
}

struct Actor {
    ships: Ships,
    id: EntityId,
    state: Rc<RefCell<State>>,
}
impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        let mut random = self.ships.0.random.borrow_mut();
        // SHIP's two DTimer constructors consume the mast and parameter delays.
        random.scaled(0.5);
        self.state.borrow_mut().tick = random.scaled(1.0);
        self.ships
            .0
            .states
            .borrow_mut()
            .insert(self.id.raw(), self.state.clone());
        Ok(true)
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        self.receive(message, None)
    }

    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        self.receive(message, Some(store))
    }

    fn process_stage_checked(&mut self, stage: Stage, _: u32) -> Result<()> {
        if stage == Stage::Realize {
            let state = self.state.borrow();
            if state.pending.is_none()
                && let Some(body) = &state.body
            {
                let models = self.ships.models()?;
                if let Some(draw) = models.loaded(body.links.model)? {
                    models.queue_draw(draw)?;
                }
            }
        }
        Ok(())
    }
    fn destroy(&mut self, context: &mut EntityDestroy<'_>) -> Result<()> {
        self.ships.abort_mount(self.id, context.manager)?;
        self.ships.0.states.borrow_mut().remove(&self.id.raw());
        if let Some(body) = self.state.borrow_mut().body.take() {
            cleanup(context.manager, self.id, body.links)?;
        }
        Ok(())
    }
}

impl Actor {
    fn receive(&mut self, message: &mut Message, store: Option<&AttributeStore>) -> Result<u64> {
        let mut state = self.state.borrow_mut();
        match message.long()? {
            CREATE => {
                let store = store.context("SHIP creation requires script attributes")?;
                ensure!(
                    state.pending.is_none() && state.body.is_none(),
                    "SHIP is already mounting or mounted"
                );
                let character = message.attribute()?;
                let definition = message.attribute()?;
                ensure!(
                    store.get(character).is_some() && store.get(definition).is_some(),
                    "invalid SHIP attribute binding"
                );
                if state.sail.get().is_nan() {
                    let speed = store
                        .lookup_path(character, "Ship.Speed.z")?
                        .map_or(0.0, |id| store.as_float(id, None, 0.0));
                    state
                        .sail
                        .set(if !stopped(store, character)? && speed > 0.0 {
                            1.0
                        } else {
                            0.0
                        });
                }
                state.pending = Some(Mount {
                    character,
                    definition,
                    step: MountStep::Start,
                    texture_paths: None,
                });
            }
            SET_LAYERS => {
                let [execute, realize] = [message.long()?, message.long()?];
                ensure!(
                    (0..32).contains(&execute) && (0..32).contains(&realize),
                    "invalid SHIP layers"
                );
                state.layers = [execute as u8, realize as u8];
            }
            MSG_SHIP_SET_SAIL_STATE | MSG_SHIP_GET_SAIL_STATE => {
                message.move_to_start();
                return state.sail.process_message(message);
            }
            MSG_SHIP_ADD_MOVE_IMPULSE => {
                let body = state.body.as_mut().context("SHIP is not mounted")?;
                message.move_to_start();
                return body.motion.process_message(message);
            }
            code @ (MSG_SHIP_GET_CHARACTER_INDEX | MSG_SHIP_GET_NUM_FIRE_PLACES) => {
                let body = state.body.as_ref().context("SHIP is not mounted")?;
                let value = if code == MSG_SHIP_GET_CHARACTER_INDEX {
                    body.index
                } else {
                    body.fire_count as i32
                };
                message.script_variable()?.set(ScriptScalar::Int(value))?;
            }
            code => bail!("unported SHIP message {code}"),
        }
        Ok(0)
    }
}

fn stopped(store: &AttributeStore, character: AttributeId) -> Result<bool> {
    Ok(store
        .lookup_path(character, "Ship.Stopped")?
        .is_some_and(|id| store.as_dword(id, None, 0) != 0))
}

fn cleanup(manager: &mut EntityManager, ship: EntityId, links: Links) -> Result<()> {
    manager.erase(links.model);
    use ship_cleanup::{
        MSG_FLAG_DEL_GROUP, MSG_ROPE_DEL_GROUP, MSG_SAIL_DEL_GROUP, MSG_VANT_DEL_GROUP,
    };
    for (service, code, owner) in [
        (links.sail, MSG_SAIL_DEL_GROUP, ship),
        (links.rope, MSG_ROPE_DEL_GROUP, links.model),
        (links.flag, MSG_FLAG_DEL_GROUP, links.model),
        (links.vants[0], MSG_VANT_DEL_GROUP, links.model),
        (links.vants[1], MSG_VANT_DEL_GROUP, links.model),
        (links.vants[2], MSG_VANT_DEL_GROUP, links.model),
    ] {
        manager.send_message(
            service,
            &mut Message::new("li", vec![Field::Long(code), Field::Entity(owner)])?,
        )?;
    }
    Ok(())
}
