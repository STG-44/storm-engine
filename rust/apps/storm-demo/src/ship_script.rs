//! Explicit startup-source embedding for the ported SHIP sail/query messages.
use super::ship_fire::{Controller, Controllers, EffectsWorld, PreviewEffects};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, path::Path, rc::Rc};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityId, EntityInit, EntityManager, Message,
};
use storm_render::model::FirePoint;
use storm_script::{DirectorySource, Host, Limits, Runtime, Value, Variable, World};
use storm_sim::Sailing;
use storm_sim::fire::{MSG_SHIP_ACTIVATE_FIRE_PLACE, MSG_SHIP_SAFE_DELETE, ShipFire};
use storm_sim::ship_queries::{MSG_SHIP_GET_CHARACTER_INDEX, MSG_SHIP_GET_NUM_FIRE_PLACES};

struct Ship {
    simulation: Rc<RefCell<Sailing>>,
    fire: Rc<RefCell<Controller>>,
    controllers: Controllers,
    initial_index: i32,
}
fn is_fire(message: &Message) -> bool {
    matches!(
        message.current_value(),
        Some(storm_core::Value::Long(
            MSG_SHIP_ACTIVATE_FIRE_PLACE | MSG_SHIP_SAFE_DELETE
        ))
    )
}
fn is_query(message: &Message) -> bool {
    matches!(
        message.current_value(),
        Some(storm_core::Value::Long(
            MSG_SHIP_GET_CHARACTER_INDEX | MSG_SHIP_GET_NUM_FIRE_PLACES
        ))
    )
}
impl Entity for Ship {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        let root = context
            .attributes
            .context("preview ship requires character attributes")?;
        self.fire
            .try_borrow_mut()?
            .queries
            .bind_character(context.store, Some(root))?;
        if context.store.child(root, "index").is_none() {
            context
                .store
                .set_attribute(root, "index", Some(&self.initial_index.to_string()))?;
        }
        self.fire.borrow_mut().effects.entity = context.id;
        self.controllers
            .borrow_mut()
            .insert(context.id.raw(), Rc::downgrade(&self.fire));
        Ok(true)
    }
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if is_query(message) || is_fire(message) {
            return self.fire.try_borrow_mut()?.process_message(message);
        }
        self.simulation
            .try_borrow_mut()
            .context("reentrant sailing callback")?
            .process_message(message)
    }
    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        attributes: &mut AttributeStore,
    ) -> Result<u64> {
        if is_query(message) || is_fire(message) {
            self.fire
                .try_borrow_mut()?
                .process_message_with_attributes(message, attributes)
        } else {
            self.process_message(message)
        }
    }
}
struct SailHost {
    world: World,
    ship: Variable,
    entities: EntityManager,
    simulation: Rc<RefCell<Sailing>>,
    messages: Vec<Json>,
    advances: Vec<Json>,
    queries_used: bool,
    fire_used: bool,
    controllers: Controllers,
    effects: Rc<RefCell<EffectsWorld>>,
}
impl SailHost {
    fn new(simulation: Sailing, fire_places: Vec<FirePoint>, initial_index: i32) -> Result<Self> {
        let world = World::default();
        let simulation = Rc::new(RefCell::new(simulation));
        let mut entities = EntityManager::default();
        let controllers = Controllers::default();
        let effects = Rc::new(RefCell::new(EffectsWorld::default()));
        effects
            .borrow_mut()
            .set_clock(simulation.borrow().time() as f32)?;
        let factory_simulation = simulation.clone();
        let factory_fires = fire_places.clone();
        let factory_controllers = controllers.clone();
        let factory_effects = effects.clone();
        entities.register_class("preview_ship_sails", move || {
            Ok(Box::new(Ship {
                simulation: factory_simulation.clone(),
                fire: Rc::new(RefCell::new(ShipFire::new(
                    factory_fires.clone(),
                    PreviewEffects {
                        world: factory_effects.clone(),
                        entity: EntityId::INVALID,
                    },
                ))),
                controllers: factory_controllers.clone(),
                initial_index,
            }))
        })?;
        // This adapter exposes no factory named SHIP: Mount and the rest of the
        // original native class are still pending.
        let fire = Rc::new(RefCell::new(ShipFire::new(
            fire_places,
            PreviewEffects {
                world: effects.clone(),
                entity: EntityId::INVALID,
            },
        )));
        let entity = entities.insert(
            "preview_ship_sails",
            Ship {
                simulation: simulation.clone(),
                fire: fire.clone(),
                controllers: controllers.clone(),
                initial_index,
            },
        )?;
        let object = world.object(entity)?;
        object.root().inspect(|store, root| {
            entities.bind_attributes(entity, store, Some(root))?;
            fire.borrow_mut().queries.bind_character(store, Some(root))
        })??;
        fire.borrow_mut().effects.entity = entity;
        controllers
            .borrow_mut()
            .insert(entity.raw(), Rc::downgrade(&fire));
        Ok(Self {
            world,
            ship: Variable::new(Value::Object(object)),
            entities,
            simulation,
            messages: Vec::new(),
            advances: Vec::new(),
            queries_used: false,
            fire_used: false,
            controllers,
            effects,
        })
    }
    fn current_entity(&self) -> Result<EntityId> {
        let Value::Object(object) = self.ship.value()? else {
            bail!("preview ship is not an object");
        };
        Ok(object.entity())
    }
    fn current_fire(&self) -> Result<Option<Rc<RefCell<Controller>>>> {
        let id = self.current_entity()?;
        if !self.entities.is_valid(id) {
            return Ok(None);
        }
        Ok(self
            .controllers
            .borrow()
            .get(&id.raw())
            .and_then(std::rc::Weak::upgrade))
    }
    fn active_fires(&self) -> Result<i32> {
        Ok(self.current_fire()?.map_or(0, |fire| {
            fire.borrow()
                .queries
                .fire_places()
                .iter()
                .filter(|s| s.state.is_active())
                .count() as i32
        }))
    }
}
impl Host for SailHost {
    fn world(&self) -> World {
        self.world.clone()
    }
    fn is_entity(&self, entity: EntityId) -> Result<bool> {
        Ok(self.entities.is_valid(entity))
    }
    fn delete_entity(&mut self, entity: EntityId) -> Result<()> {
        self.entities.erase(entity);
        Ok(())
    }
    fn create_entity(
        &mut self,
        class: &str,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<EntityId> {
        self.entities.create(class, store, root)
    }
    fn send_message(
        &mut self,
        entity: EntityId,
        message: &mut Message,
        attributes: &mut AttributeStore,
    ) -> Result<u64> {
        ensure!(
            self.messages.len() < 256,
            "sail script message report budget"
        );
        let delivered = self.entities.is_valid(entity);
        self.queries_used |= is_query(message);
        self.fire_used |= is_fire(message);
        if delivered
            && let Some(fire) = self
                .controllers
                .borrow()
                .get(&entity.raw())
                .and_then(std::rc::Weak::upgrade)
        {
            fire.borrow_mut()
                .set_transform(super::sailing::pose(&self.simulation.borrow()).matrix()?)?;
        }
        let code = match message.values().first() {
            Some(storm_core::Value::Long(code)) => Some(*code),
            _ => None,
        };
        let result = self
            .entities
            .send_message_with_attributes(entity, message, attributes)?;
        self.messages.push(json!({"code":code, "format":message.format(), "delivered":delivered,
            "result":result, "sail_request_bits":self.simulation.borrow().sail_request().to_bits()}));
        Ok(result)
    }
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        match (name, arguments) {
            ("previewship", []) => Ok(self.ship.reference()),
            ("previewactivefires", []) => Ok(Value::Int(self.active_fires()?)),
            ("previewadvancesailing", [Value::Float(seconds)]) => {
                ensure!(
                    self.advances.len() < 256,
                    "sail script advance report budget"
                );
                let mut simulation = self.simulation.try_borrow_mut()?;
                ensure!(
                    simulation.time() + f64::from(*seconds) <= 3600.0,
                    "sail script simulation time budget"
                );
                simulation.advance(f64::from(*seconds))?;
                let mut record = json!({"seconds":seconds, "time":simulation.time(),
                    "sail_request_bits":simulation.sail_request().to_bits(),
                    "sail_mode":simulation.sail_mode().name(),
                    "normal_deployed":simulation.rig().normal,
                    "full_only_deployed":simulation.rig().full_only});
                let time = simulation.time() as f32;
                let pose = super::sailing::pose(&simulation);
                let deployed = simulation.sail_deployed() as f32;
                drop(simulation);
                self.effects.borrow_mut().set_clock(time)?;
                for (id, fire) in self.controllers.borrow().iter() {
                    if self.entities.is_valid(EntityId::from_raw(*id))
                        && let Some(fire) = fire.upgrade()
                    {
                        super::ship_fire::update(&mut fire.borrow_mut(), *seconds, pose)?;
                    }
                }
                if self.fire_used {
                    record["active_fires"] = json!(self.active_fires()?);
                }
                self.advances.push(record);
                Ok(Value::Float(deployed))
            }
            _ => bail!("unsupported sail script native {name} or argument types"),
        }
    }
}

fn execute_with_ship(
    simulation: Sailing,
    mut runtime: Runtime,
    fire_places: Vec<FirePoint>,
    initial_index: i32,
) -> Result<(Sailing, Json, Option<super::ship_fire::Session>)> {
    let mut host = SailHost::new(simulation, fire_places, initial_index)?;
    let result = runtime.call("main", &[], &mut host, Limits::default())?;
    let result = match result {
        Value::Int(value) => json!({"type":"int", "value":value}),
        Value::Float(value) => json!({"type":"float", "bits":value.to_bits()}),
        Value::Ptr(value) => json!({"type":"ptr", "value":value}),
        Value::String(value) => json!({"type":"string", "value":value}),
        Value::Void => json!({"type":"void"}),
        _ => bail!("sail script main must return a scalar or void"),
    };
    let mut report = json!({"original_game_scripts":false, "native_class":"preview_ship_sails",
        "sources":runtime.sources(), "result":result, "messages":host.messages,
        "advances":host.advances});
    if host.queries_used {
        fn tree(store: &AttributeStore, root: AttributeId) -> Json {
            let node = store.get(root).expect("checked live script attributes");
            json!({"name":store.name(root),"value":node.value(),
                "children":node.children().iter().map(|child|tree(store,*child)).collect::<Vec<_>>()})
        }
        report["ship_query_attributes"] = host.ship.value()?.attributes()?.inspect(tree)?;
    }
    let controller = if host.fire_used {
        host.current_fire()?
    } else {
        None
    };
    drop(runtime);
    drop(host.entities);
    let fire = host.fire_used.then(|| super::ship_fire::Session {
        controller,
        world: host.effects.clone(),
    });
    let simulation = Rc::try_unwrap(host.simulation)
        .map_err(|_| anyhow::anyhow!("sailing adapter still has a live owner"))?
        .into_inner();
    Ok((simulation, report, fire))
}

pub fn run(
    simulation: Sailing,
    path: &Path,
    fire_places: Vec<FirePoint>,
    initial_index: i32,
) -> Result<(Sailing, Json, Option<super::ship_fire::Session>)> {
    let entry = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("sail script filename must be UTF-8")?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let mut loader = DirectorySource::new(parent.unwrap_or(Path::new(".")))?;
    let runtime = Runtime::compile_with_loader(entry, &mut loader)?;
    execute_with_ship(simulation, runtime, fire_places, initial_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use storm_sim::{ShipConfig, Wind};
    #[test]
    fn original_fire_source_retains_a_live_controller_after_cleanup_and_recreation() {
        let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
        let runtime = Runtime::compile(include_str!("../fixtures/ship_fires.c")).unwrap();
        let (simulation, report, fire) =
            execute_with_ship(simulation(), runtime, asset.ship.fires.clone(), 19).unwrap();
        assert_eq!(
            report["result"],
            json!({"type":"int","value":asset.ship.fires.len()})
        );
        assert_eq!(simulation.time(), 2.125);
        assert_eq!(
            report["advances"]
                .as_array()
                .unwrap()
                .iter()
                .map(|s| s["active_fires"].as_u64().unwrap())
                .collect::<Vec<_>>(),
            [1, 0, 1]
        );
        let fire = fire.unwrap();
        let snapshot = fire.report();
        assert_eq!(snapshot["places"][0]["active"], true);
        assert_eq!(snapshot["places"][0]["remaining_bits"], 1.0f32.to_bits());
        assert_eq!(snapshot["places"][0]["owner"], 23);
        let events = snapshot["effects"]["recent"].as_array().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|e| e["op"] == "create_particle")
                .count(),
            6
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| e["op"] == "pause_particle")
                .count(),
            4
        );
        assert!(!events.iter().any(|e| e["profile"] == "ignored"));
    }
    #[test]
    fn deletion_runs_native_cleanup_while_emitted_particles_outlive_the_entity() {
        let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
        let runtime = Runtime::compile(
            r#"
            native ref PreviewShip();
            void main(){ref ship;ship=PreviewShip();CreateEntity(&ship,"preview_ship_sails");
                SendMessage(ship,"llsssfl",50304,0,"s","f","a",2.0,7);
                DeleteClass(ship);SendMessage(ship,"l",50315);}
        "#,
        )
        .unwrap();
        let (_, _, fire) =
            execute_with_ship(simulation(), runtime, asset.ship.fires.clone(), 19).unwrap();
        let fire = fire.unwrap();
        assert!(fire.controller.is_none());
        let snapshot = fire.report();
        assert_eq!(snapshot["places"], json!([]));
        assert_eq!(snapshot["emissions"].as_array().unwrap().len(), 2);
        assert!(
            snapshot["emissions"]
                .as_array()
                .unwrap()
                .iter()
                .all(|e| e["stopped_bits"] == 0)
        );
        assert_eq!(
            snapshot["effects"]["recent"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["op"] == "pause_particle")
                .count(),
            2
        );
    }
    fn execute(simulation: Sailing, runtime: Runtime) -> Result<(Sailing, Json)> {
        let (simulation, report, _) = execute_with_ship(simulation, runtime, Vec::new(), 19)?;
        Ok((simulation, report))
    }
    #[test]
    fn initialized_query_source_reads_live_character_and_loaded_model_fireplaces() {
        let asset = storm_render::model::ModelAsset::ship_demo().unwrap();
        let count = asset.ship.fires.len();
        assert!(count > 0);
        let runtime = Runtime::compile(include_str!("../fixtures/ship_queries.c")).unwrap();
        let (_, report, _) =
            execute_with_ship(simulation(), runtime, asset.ship.fires.clone(), 37).unwrap();
        assert_eq!(report["result"], json!({"type":"int","value":count}));
        let attributes = report["ship_query_attributes"]["children"]
            .as_array()
            .unwrap();
        let value = |name: &str| {
            attributes.iter().find(|a| a["name"] == name).unwrap()["value"]
                .as_str()
                .unwrap()
        };
        assert_eq!(value("initial_index"), "37");
        assert_eq!(value("updated_index"), "27");
        assert_eq!(value("negative_index"), "-7");
        assert_eq!(value("deleted_index"), "-7");
        assert_eq!(value("fire_place_count"), count.to_string());
        assert_eq!(value("deleted_count"), count.to_string());
        let messages = report["messages"].as_array().unwrap();
        assert_eq!(messages.len(), 7);
        assert!(
            messages[..5]
                .iter()
                .all(|m| m["delivered"] == true && m["result"] == 0)
        );
        assert!(
            messages[5..]
                .iter()
                .all(|m| m["delivered"] == false && m["result"] == 0)
        );
    }
    #[test]
    fn factories_keep_character_bindings_separate_and_preserve_existing_indices() {
        let runtime = Runtime::compile(
            r#"
            int main() {
                object a; object b;
                a.index = 7;
                CreateEntity(&a, "preview_ship_sails");
                CreateEntity(&b, "preview_ship_sails");
                int first; int second;
                SendMessage(a, "le", 50307, &first);
                SendMessage(b, "le", 50307, &second);
                if (first != 7 || second != 37) { return -1; }
                a.index = 11;
                SendMessage(a, "le", 50307, &first);
                SendMessage(b, "le", 50307, &second);
                return first + second;
            }
        "#,
        )
        .unwrap();
        let (_, report, _) = execute_with_ship(simulation(), runtime, Vec::new(), 37).unwrap();
        assert_eq!(report["result"], json!({"type":"int","value":48}));
    }
    #[test]
    fn missing_character_index_and_attribute_less_creation_fail_explicitly() {
        for (source, expected) in [
            (
                r#"native ref PreviewShip(); void main() {ref ship; ship=PreviewShip();
                CreateEntity(&ship,"preview_ship_sails"); DeleteAttribute(ship,"index");
                int output; SendMessage(ship,"le",50307,&output);}"#,
                "no index attribute",
            ),
            (
                r#"void main() {CreateClass("preview_ship_sails");}"#,
                "requires character attributes",
            ),
        ] {
            let error = execute(simulation(), Runtime::compile(source).unwrap())
                .err()
                .unwrap();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
        }
    }

    fn simulation() -> Sailing {
        Sailing::new(
            ShipConfig::default(),
            Wind {
                speed_mps: 12.0,
                direction: 0.0,
            },
            0.0,
        )
        .unwrap()
    }
    #[test]
    fn source_messages_drive_real_rig_and_pending_deletion_keeps_output_unchanged() {
        let runtime = Runtime::compile(include_str!("../fixtures/ship_sails.c")).unwrap();
        let (sim, report) = execute(simulation(), runtime).unwrap();
        assert_eq!(report["result"], json!({"type":"int","value":3}));
        assert_eq!(sim.time(), 12.0);
        assert_eq!(sim.sail_request(), 1.0);
        assert_eq!(
            sim.rig(),
            storm_sim::RigState::for_mode(storm_sim::SailMode::Full)
        );
        assert_eq!(report["advances"][0]["full_only_deployed"], 0.0);
        assert_eq!(report["advances"][0]["normal_deployed"], 1.0);
        assert_eq!(report["advances"][1]["normal_deployed"], 0.0);
        assert_eq!(report["messages"].as_array().unwrap().len(), 8);
        assert_eq!(report["messages"][6]["delivered"], false);
        assert_eq!(report["messages"][7]["delivered"], false);
    }
    #[test]
    fn native_get_retypes_caller_but_by_value_output_is_temporary() {
        let runtime = Runtime::compile(
            r#"
            native ref PreviewShip();
            float main() {
                ref ship; ship = PreviewShip();
                SendMessage(ship, "lf", 50305, 0.25);
                int output; output = 19;
                SendMessage(ship, "le", 50325, output);
                if (output != 19) { return -1.0; }
                SendMessage(ship, "le", 50325, &output);
                return output;
            }
        "#,
        )
        .unwrap();
        let (sim, report) = execute(simulation(), runtime).unwrap();
        assert_eq!(sim.sail_request(), 0.25);
        assert_eq!(
            report["result"],
            json!({"type":"float","bits":0.25f32.to_bits()})
        );
    }
    #[test]
    fn unported_ship_messages_and_invalid_solver_commands_are_explicit_errors() {
        for (code, arguments, expected) in [
            (50300, "", "not implemented"),
            (50305, ", 1.5", "invalid sail mode setting"),
        ] {
            let runtime = Runtime::compile(&format!(
                r#"
                native ref PreviewShip(); native float PreviewAdvanceSailing(float seconds);
                void main() {{ ref ship; ship = PreviewShip();
                    SendMessage(ship, "{}", {code}{arguments});
                    PreviewAdvanceSailing(0.0);
                }}
            "#,
                if arguments.is_empty() { "l" } else { "lf" }
            ))
            .unwrap();
            let error = execute(simulation(), runtime).err().unwrap();
            assert!(format!("{error:#}").contains(expected), "{error:#}");
        }
    }
}
