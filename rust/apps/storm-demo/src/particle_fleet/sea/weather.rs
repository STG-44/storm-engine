//! Script-to-entity attribute adapter; not the complete original weather service.
use super::*;
use std::cell::Cell;
use storm_core::{AttributeId, AttributeStore, EntityManager};
use storm_script::{Host, Limits, Runtime, Variable, World};
type Records = Rc<RefCell<Vec<Json>>>;
struct Adapter {
    sea: Rc<RefCell<Sea>>,
    root: Rc<Cell<AttributeId>>,
    records: Records,
    stage: Rc<RefCell<StageState>>,
}
#[derive(Default)]
struct StageState {
    enabled: bool,
    error: Option<String>,
    calls: u32,
    applied: u32,
}
impl Entity for Adapter {
    fn process_stage(&mut self, stage: storm_core::Stage, delta_ms: u32) {
        if stage != storm_core::Stage::Realize || !self.stage.borrow().enabled {
            return;
        }
        let result = self.sea.borrow_mut().realize(delta_ms);
        let mut state = self.stage.borrow_mut();
        state.calls += 1;
        match result {
            Ok(true) => state.applied += 1,
            Ok(false) => {}
            Err(e) => state.error = Some(e.to_string()),
        }
    }
    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let mut sea = self.sea.borrow_mut();
        let stopped = sea.weather().unwrap().stopped;
        let effect = sea.attribute_changed(store, self.root.get(), changed)?;
        let surface = sea.surface();
        let weather = sea.weather().unwrap();
        self.records.borrow_mut().push(json!({"name":store.name(changed),"parent":store.get(changed).and_then(|n|n.parent()).and_then(|p|store.name(p)),"value":store.value(changed,None),
            "surface_attribute":effect.surface_attribute,"realize_zero_requested":effect.realize_zero_requested,"realize_zero_applied":effect.realize_zero_requested && !stopped,
            "raw_amplitude_bits":weather.raw_amplitude.map(f32::to_bits),"amplitude_bits":surface.parameters.amplitude.map(f32::to_bits),"stopped":weather.stopped,"frame_bits":surface.animation.frame.map(f32::to_bits)}));
        Ok(0)
    }
}
struct WeatherHost {
    world: World,
    object: Variable,
    entities: EntityManager,
    id: EntityId,
}
impl Host for WeatherHost {
    fn world(&self) -> World {
        self.world.clone()
    }
    fn attribute_changed(
        &mut self,
        id: EntityId,
        store: &AttributeStore,
        changed: AttributeId,
    ) -> Result<()> {
        self.entities.notify_attribute_changed(id, store, changed)?;
        Ok(())
    }
    fn call(&mut self, name: &str, args: &[storm_script::Value]) -> Result<storm_script::Value> {
        match (name, args) {
            ("previewsea", []) => Ok(self.object.reference()),
            _ => anyhow::bail!("unsupported sea fixture native {name}"),
        }
    }
}
pub(super) struct Session {
    runtime: Runtime,
    host: WeatherHost,
    records: Records,
    delivered: usize,
    stage: Rc<RefCell<StageState>>,
}
impl Session {
    pub fn new(sea: Rc<RefCell<Sea>>) -> Result<Self> {
        let world = World::default();
        let mut entities = EntityManager::default();
        let root = Rc::new(Cell::new(AttributeId::INVALID));
        let records = Rc::new(RefCell::new(Vec::new()));
        let stage = Rc::new(RefCell::new(StageState::default()));
        let id = entities.insert(
            "sea",
            Adapter {
                sea,
                root: root.clone(),
                records: records.clone(),
                stage: stage.clone(),
            },
        )?;
        let object = world.object(id)?;
        object.root().inspect(|store, binding| {
            root.set(binding);
            entities.bind_attributes(id, store, Some(binding))
        })??;
        let mut host = WeatherHost {
            world,
            object: Variable::new(storm_script::Value::Object(object)),
            entities,
            id,
        };
        let mut runtime = Runtime::compile(include_str!("../../../fixtures/sea_weather.c"))?;
        ensure!(
            matches!(
                runtime.call("Main", &[], &mut host, Limits::default())?,
                storm_script::Value::Int(6)
            ),
            "sea weather fixture initialization failed"
        );
        Ok(Self {
            runtime,
            host,
            records,
            delivered: 0,
            stage,
        })
    }
    pub fn advance(&mut self, delta_ms: u32) -> Result<()> {
        self.delivered +=
            self.runtime
                .advance_frame(delta_ms, &mut self.host, Limits::default())?;
        Ok(())
    }
    pub fn enable_stages(&mut self) -> Result<()> {
        self.host
            .entities
            .set_layer_type(0, storm_core::LayerType::Realize)?;
        self.host.entities.add_to_layer(0, self.host.id, 0)?;
        self.stage.borrow_mut().enabled = true;
        Ok(())
    }
    pub fn execute_boundary(&mut self, delta_ms: u32) -> Result<()> {
        self.host
            .entities
            .process_stage(storm_core::Stage::Execute, delta_ms)
    }
    pub fn realize(&mut self, delta_ms: u32) -> Result<bool> {
        let before = self.stage.borrow().applied;
        self.host
            .entities
            .process_stage(storm_core::Stage::Realize, delta_ms)?;
        let mut state = self.stage.borrow_mut();
        if let Some(error) = state.error.take() {
            anyhow::bail!(error);
        }
        Ok(state.applied != before)
    }
    pub fn stages_report(&self) -> Json {
        let s = self.stage.borrow();
        json!({"dispatcher":"storm_core_entity_manager_realize_layer","layer":0,"priority":0,"calls":s.calls,"applied":s.applied,"full_global_layers":false})
    }
    pub fn report(&self, sea: &Sea) -> Json {
        let w = sea.weather().unwrap();
        json!({"source":"sea_weather.c","original_game_scripts":false,"native_class":"sea","entity":self.host.id.raw(),"full_native_entity":false,"particle_frame_math":"pinned_trig_scalar_pose_history",
            "raw_amplitude_bits":w.raw_amplitude.map(f32::to_bits),"max_height_bits":w.max_height.to_bits(),"stopped":w.stopped,"vertical_speed_bits":w.speed_y.map(f32::to_bits),"vertical_movement_bits":w.movement_y.map(f32::to_bits),
            "delivered_events":self.delivered,"pending_events":self.runtime.pending_events(),"attribute_callbacks":self.records.borrow().len(),"changes":*self.records.borrow()})
    }
}
