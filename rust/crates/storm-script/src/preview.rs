//! Explicit diagnostic bindings; these are not original game native functions.
use crate::{Host, Value};
use anyhow::{Result, bail, ensure};
use serde_json::{Value as Json, json};
use storm_core::{AttributeId, AttributeStore};

pub struct AttributeHost {
    attributes: AttributeStore,
    root: AttributeId,
    writes: u64,
}

impl AttributeHost {
    pub fn new() -> Result<Self> {
        let mut attributes = AttributeStore::default();
        let root = attributes.create_root()?;
        Ok(Self {
            attributes,
            root,
            writes: 0,
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
        };
        json!({"script_runtime":"bounded-source-subset", "original_game_scripts":false,
            "result":result, "attribute_writes":self.writes,
            "attributes":tree(&self.attributes,self.root)})
    }
}

impl Host for AttributeHost {
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        match (name, arguments) {
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
