use anyhow::{Context, Result, ensure};
use std::collections::BTreeMap;
use storm_core::{AttributeId, AttributeStore, EntityId, EntityManager, LayerType, Message};
use storm_script::{Host, Value, World};

type NativeFunction = Box<dyn FnMut(&[Value]) -> Result<Value>>;
type TraceSink = Box<dyn FnMut(&str) -> Result<()>>;

/// The same World supplies script objects and native entity attribute bindings.
/// Native classes/functions are explicitly registered by production subsystems.
pub struct NativeHost {
    world: World,
    pub entities: EntityManager,
    functions: BTreeMap<String, NativeFunction>,
    trace: TraceSink,
    pub(crate) delta_ms: u32,
}
impl NativeHost {
    pub fn new() -> Result<Self> {
        let mut entities = EntityManager::default();
        // CoreImpl::Init and shared/layers.h. Trace/reflection layers stay Common.
        for index in 0..15 {
            let kind = if index < 12 && index % 2 == 0 {
                LayerType::Execute
            } else {
                LayerType::Realize
            };
            entities.set_layer_type(index, kind)?;
        }
        Ok(Self {
            world: World::default(),
            entities,
            functions: BTreeMap::new(),
            trace: Box::new(|text| {
                eprintln!("{text}");
                Ok(())
            }),
            delta_ms: 0,
        })
    }
    pub fn register_function(
        &mut self,
        name: &str,
        function: impl FnMut(&[Value]) -> Result<Value> + 'static,
    ) -> Result<()> {
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && name.is_ascii()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid native function name"
        );
        ensure!(
            self.functions.len() < 256,
            "native function registration budget"
        );
        let name = name.to_ascii_lowercase();
        ensure!(
            !self.functions.contains_key(&name),
            "duplicate native function: {name}"
        );
        self.functions.insert(name, Box::new(function));
        Ok(())
    }
    pub fn set_trace_sink(&mut self, sink: impl FnMut(&str) -> Result<()> + 'static) {
        self.trace = Box::new(sink);
    }
}
impl Host for NativeHost {
    fn world(&self) -> World {
        self.world.clone()
    }
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        self.functions
            .get_mut(name)
            .with_context(|| format!("native function is not registered: {name}"))?(
            arguments
        )
    }
    fn is_entity(&self, id: EntityId) -> Result<bool> {
        Ok(self.entities.is_valid(id))
    }
    fn delete_entity(&mut self, id: EntityId) -> Result<()> {
        self.entities.erase(id);
        Ok(())
    }
    fn create_entity(
        &mut self,
        class: &str,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<EntityId> {
        self.entities
            .create(class, store, root)
            .with_context(|| format!("create native class {class}"))
    }
    fn send_message(
        &mut self,
        id: EntityId,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        self.entities
            .send_message_with_attributes(id, message, store)
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
    fn set_layer_type(&mut self, index: u8, kind: LayerType) -> Result<()> {
        self.entities.set_layer_type(index, kind)
    }
    fn add_to_layer(&mut self, index: u8, id: EntityId, priority: u32) -> Result<()> {
        self.entities.add_to_layer(index, id, priority)
    }
    fn remove_from_layer(&mut self, index: u8, id: EntityId) -> Result<()> {
        self.entities.remove_from_layer(index, id)
    }
    fn freeze_layer(&mut self, index: u8, frozen: bool) -> Result<()> {
        self.entities.freeze_layer(index, frozen)
    }
    fn delta_time(&self) -> Result<u32> {
        Ok(self.delta_ms)
    }
    fn trace(&mut self, text: &str) -> Result<()> {
        (self.trace)(text)
    }
}
