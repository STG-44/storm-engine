//! Explicit diagnostic bindings; these are not original game native functions.
use crate::{Host, Value, Variable, World};
use anyhow::{Result, bail, ensure};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, collections::VecDeque, rc::Rc};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityId, EntityInit, EntityManager, Message,
};

#[derive(Default)]
struct Notifications {
    total: u64,
    recent: VecDeque<Json>,
}
struct Observer(Rc<RefCell<Notifications>>);
impl Entity for Observer {
    fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
        if let Some(root) = ctx.attributes {
            ctx.store.set_attribute(root, "initialized", Some("1"))?;
        }
        Ok(true)
    }
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
struct Reject;
impl Entity for Reject {
    fn init(&mut self, ctx: &mut EntityInit<'_>) -> Result<bool> {
        if let Some(root) = ctx.attributes {
            ctx.store.set_attribute(root, "rejected", Some("1"))?;
        }
        Ok(false)
    }
}

struct MessageProbe(Rc<RefCell<Notifications>>);
impl Entity for MessageProbe {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let format = message.format().to_owned();
        let result = match format.as_str() {
            "lfsiap" => {
                ensure!(message.long()? == 548, "diagnostic message command");
                ensure!(message.float()? == 7.0, "diagnostic float field");
                ensure!(message.string()? == "парус", "diagnostic string field");
                ensure!(
                    message.entity()? != EntityId::INVALID,
                    "diagnostic entity field"
                );
                message.attribute()?;
                ensure!(message.pointer()? == u64::MAX, "diagnostic pointer field");
                0xfedc_ba98_7654_3210
            }
            "p" => message.pointer()?,
            "" => 0,
            _ => bail!("unsupported diagnostic message format"),
        };
        ensure!(!message.param_valid(), "unread diagnostic message fields");
        let mut records = self.0.borrow_mut();
        records.total += 1;
        if records.recent.len() == 256 {
            records.recent.pop_front();
        }
        records
            .recent
            .push_back(json!({"format":format,"result":result}));
        Ok(result)
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
    messages: Rc<RefCell<Notifications>>,
    ship_used: bool,
}

impl AttributeHost {
    pub fn new() -> Result<Self> {
        let mut attributes = AttributeStore::default();
        let root = attributes.create_root()?;
        let world = World::default();
        let notifications = Rc::new(RefCell::new(Notifications::default()));
        let mut entities = EntityManager::default();
        let records = notifications.clone();
        entities.register_class("script_attribute_observer", move || {
            Ok(Box::new(Observer(records.clone())))
        })?;
        entities.register_class("script_attribute_reject", || Ok(Box::new(Reject)))?;
        let messages = Rc::new(RefCell::new(Notifications::default()));
        let message_records = messages.clone();
        entities.register_class("script_message_probe", move || {
            Ok(Box::new(MessageProbe(message_records.clone())))
        })?;
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
            messages,
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
            Value::Ptr(n) => json!({"type":"ptr", "value":n}),
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
        let messages = self.messages.borrow();
        if messages.total != 0 {
            report["entity_messages"] = json!({"total":messages.total,"recent":messages.recent});
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
    fn create_entity(
        &mut self,
        class: &str,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<EntityId> {
        self.entities.create(class, store, root)
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
    fn send_message(
        &mut self,
        entity: EntityId,
        message: &mut Message,
        _attributes: &mut AttributeStore,
    ) -> Result<u64> {
        self.entities.send_message(entity, message)
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
