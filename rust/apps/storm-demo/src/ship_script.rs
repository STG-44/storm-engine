//! Explicit startup-source embedding for the two ported SHIP sail messages.
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, path::Path, rc::Rc};
use storm_core::{AttributeStore, Entity, EntityId, EntityManager, Message};
use storm_script::{DirectorySource, Host, Limits, Runtime, Value, Variable, World};
use storm_sim::Sailing;

struct Ship(Rc<RefCell<Sailing>>);
impl Entity for Ship {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        self.0
            .try_borrow_mut()
            .context("reentrant sailing callback")?
            .process_message(message)
    }
}
struct SailHost {
    world: World,
    ship: Variable,
    entities: EntityManager,
    simulation: Rc<RefCell<Sailing>>,
    messages: Vec<Json>,
    advances: Vec<Json>,
}
impl SailHost {
    fn new(simulation: Sailing) -> Result<Self> {
        let world = World::default();
        let simulation = Rc::new(RefCell::new(simulation));
        let mut entities = EntityManager::default();
        // This adapter exposes no factory named SHIP: Mount and the rest of the
        // original native class are still pending.
        let entity = entities.insert("preview_ship_sails", Ship(simulation.clone()))?;
        let object = world.object(entity)?;
        object
            .root()
            .inspect(|store, root| entities.bind_attributes(entity, store, Some(root)))??;
        Ok(Self {
            world,
            ship: Variable::new(Value::Object(object)),
            entities,
            simulation,
            messages: Vec::new(),
            advances: Vec::new(),
        })
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
    fn send_message(
        &mut self,
        entity: EntityId,
        message: &mut Message,
        _attributes: &mut AttributeStore,
    ) -> Result<u64> {
        ensure!(
            self.messages.len() < 256,
            "sail script message report budget"
        );
        let delivered = self.entities.is_valid(entity);
        let code = match message.values().first() {
            Some(storm_core::Value::Long(code)) => Some(*code),
            _ => None,
        };
        let result = self.entities.send_message(entity, message)?;
        self.messages.push(json!({"code":code, "format":message.format(), "delivered":delivered,
            "result":result, "sail_request_bits":self.simulation.borrow().sail_request().to_bits()}));
        Ok(result)
    }
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        match (name, arguments) {
            ("previewship", []) => Ok(self.ship.reference()),
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
                self.advances
                    .push(json!({"seconds":seconds, "time":simulation.time(),
                    "sail_request_bits":simulation.sail_request().to_bits(),
                    "sail_mode":simulation.sail_mode().name(),
                    "normal_deployed":simulation.rig().normal,
                    "full_only_deployed":simulation.rig().full_only}));
                Ok(Value::Float(simulation.sail_deployed() as f32))
            }
            _ => bail!("unsupported sail script native {name} or argument types"),
        }
    }
}

fn execute(simulation: Sailing, mut runtime: Runtime) -> Result<(Sailing, Json)> {
    let mut host = SailHost::new(simulation)?;
    let result = runtime.call("main", &[], &mut host, Limits::default())?;
    let result = match result {
        Value::Int(value) => json!({"type":"int", "value":value}),
        Value::Float(value) => json!({"type":"float", "bits":value.to_bits()}),
        Value::Ptr(value) => json!({"type":"ptr", "value":value}),
        Value::String(value) => json!({"type":"string", "value":value}),
        Value::Void => json!({"type":"void"}),
        _ => bail!("sail script main must return a scalar or void"),
    };
    let report = json!({"original_game_scripts":false, "native_class":"preview_ship_sails",
        "sources":runtime.sources(), "result":result, "messages":host.messages,
        "advances":host.advances});
    drop(runtime);
    drop(host.entities);
    let simulation = Rc::try_unwrap(host.simulation)
        .map_err(|_| anyhow::anyhow!("sailing adapter still has a live owner"))?
        .into_inner();
    Ok((simulation, report))
}

pub fn run(simulation: Sailing, path: &Path) -> Result<(Sailing, Json)> {
    let entry = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("sail script filename must be UTF-8")?;
    let parent = path.parent().filter(|p| !p.as_os_str().is_empty());
    let mut loader = DirectorySource::new(parent.unwrap_or(Path::new(".")))?;
    let runtime = Runtime::compile_with_loader(entry, &mut loader)?;
    execute(simulation, runtime)
}

#[cfg(test)]
mod tests {
    use super::*;
    use storm_sim::{ShipConfig, Wind};

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
