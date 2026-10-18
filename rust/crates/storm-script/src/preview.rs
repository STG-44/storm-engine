//! Explicit diagnostic bindings; these are not original game native functions.
use crate::{Host, Value, Variable, World};
use anyhow::{Result, bail, ensure};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use storm_core::{AttributeId, AttributeStore, Entity, EntityId, EntityManager};

#[derive(Default)]
struct Notifications {
    total: u64,
    recent: VecDeque<Json>,
}
struct Observer(Rc<RefCell<Notifications>>);
impl Entity for Observer {
    fn attribute_changed(&mut self, store: &AttributeStore, changed: AttributeId) -> Result<u32> {
        let mut records = self.0.borrow_mut();
        records.total += 1;
        if records.recent.len() == 256 {
            records.recent.pop_front();
        }
        records.recent.push_back(
            json!({"name":store.name(changed),"value":store.get(changed).and_then(|n|n.value())}),
        );
        Ok(0)
    }
}

pub struct AttributeHost {
    attributes: AttributeStore,
    root: AttributeId,
    writes: u64,
    world: World,
    ship: Variable,
    entities: EntityManager,
    notifications: Rc<RefCell<Notifications>>,
    ship_used: bool,
}

impl AttributeHost {
    pub fn new() -> Result<Self> {
        let mut attributes = AttributeStore::default();
        let root = attributes.create_root()?;
        let world = World::default();
        let notifications = Rc::new(RefCell::new(Notifications::default()));
        let mut entities = EntityManager::default();
        let entity =
            entities.insert("script_attribute_observer", Observer(notifications.clone()))?;
        let object = world.object(entity)?;
        object
            .root()
            .inspect(|store, id| entities.bind_attributes(entity, store, Some(id)))??;
        Ok(Self {
            attributes,
            root,
            writes: 0,
            ship: Variable::new(Value::Object(object)),
            world,
            entities,
            notifications,
            ship_used: false,
        })
    }

    pub fn report(&self, result: &Value) -> Json {
        fn tree(attributes: &AttributeStore, id: AttributeId) -> Json {
            let node = attributes.get(id).unwrap();
            json!({"name":attributes.name(id).unwrap(), "value":node.value(),
                "children":node.children().iter().map(|id|tree(attributes,*id)).collect::<Vec<_>>()})
        }
        let result = match result {
            Value::Int(n) => json!({"type":"int", "value":n}),
            Value::Float(n) => json!({"type":"float", "bits":n.to_bits()}),
            Value::String(s) => json!({"type":"string", "value":s}),
            Value::Void => json!({"type":"void"}),
            value => json!({"type":format!("{:?}",value.kind()),"valid":value.resolved().is_ok()}),
        };
        let mut report = json!({"script_runtime":"bounded-source-subset", "original_game_scripts":false,
            "result":result, "attribute_writes":self.writes,
            "attributes":tree(&self.attributes,self.root)});
        if self.ship_used {
            report["script_ship"] = self
                .ship
                .value()
                .expect("host ship")
                .attributes()
                .expect("object")
                .inspect(tree)
                .expect("live ship");
            let records = self.notifications.borrow();
            report["ship_attribute_notifications"] = json!(records.total);
            report["ship_attribute_changes"] = json!(records.recent);
        }
        report
    }
}

impl Host for AttributeHost {
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
    fn attribute_changed(
        &mut self,
        entity: EntityId,
        store: &AttributeStore,
        changed: AttributeId,
    ) -> Result<()> {
        if entity != EntityId::INVALID {
            self.entities
                .notify_attribute_changed(entity, store, changed)?;
        }
        Ok(())
    }
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        match (name, arguments) {
            ("previewship", []) => {
                self.ship_used = true;
                Ok(self.ship.reference())
            }
            ("previewsetattribute", [path, value]) => {
                // Bound total host state as well as VM allocations.
                let path = path.text()?;
                let value = value.text()?;
                ensure!(
                    self.attributes.len() + path.split('.').count() <= 4096 && self.writes < 4096,
                    "preview attribute limit"
                );
                let id = self.attributes.create_path(self.root, path)?;
                self.attributes.set_value(id, Some(value))?;
                self.writes += 1;
                Ok(Value::Void)
            }
            ("previewgetattribute", [path]) => {
                let id = self.attributes.lookup_path(self.root, path.text()?)?;
                let value = id
                    .and_then(|id| self.attributes.get(id))
                    .and_then(|n| n.value());
                Ok(Value::String(value.unwrap_or("").to_owned()))
            }
            _ => bail!("unsupported preview native {name} or argument count"),
        }
    }
}
