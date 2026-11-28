use super::{Observer, Records};
use anyhow::{Result, bail};
use serde_json::json;
use std::{cell::RefCell, rc::Rc};
use storm_core::{AttributeId, AttributeStore, EntityId, EntityManager};
use storm_script::{AttributeReference, Host, Value, Variable, World};

pub(super) struct DamageHost {
    world: World,
    characters: Variable,
    entities: EntityManager,
    pub observer: EntityId,
    records: Rc<RefCell<Records>>,
}
impl DamageHost {
    pub fn new(records: Rc<RefCell<Records>>) -> Result<Self> {
        let world = World::default();
        let mut entities = EntityManager::default();
        let observer =
            entities.insert("preview_sail_damage_observer", Observer(records.clone()))?;
        let object = world.object(observer)?;
        object
            .root()
            .inspect(|store, root| entities.bind_attributes(observer, store, Some(root)))??;
        Ok(Self {
            world,
            characters: Variable::new(Value::Object(object)),
            entities,
            observer,
            records,
        })
    }
    pub fn root(&self) -> Result<AttributeReference> {
        self.characters.value()?.attributes()
    }
}
impl Host for DamageHost {
    fn world(&self) -> World {
        self.world.clone()
    }
    fn attribute_changed(
        &mut self,
        entity: EntityId,
        store: &AttributeStore,
        changed: AttributeId,
    ) -> Result<()> {
        self.entities
            .notify_attribute_changed(entity, store, changed)?;
        Ok(())
    }
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        match (name, arguments) {
            ("previewcharacters", []) => Ok(self.characters.reference()),
            (
                "previewsaildamage",
                [
                    Value::Int(character),
                    Value::Float(x),
                    Value::Float(y),
                    Value::Float(z),
                    Value::Int(owner),
                ],
            ) => {
                let mut records = self.records.borrow_mut();
                records.script_damage_calls += 1;
                records.script_last_damage =
                    Some(json!({"character":character,"position":[x,y,z],"owner":owner}));
                Ok(Value::Void)
            }
            _ => bail!("unsupported damage script native {name} or argument types"),
        }
    }
}
