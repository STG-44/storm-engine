use anyhow::{Context, Result, ensure};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use storm_core::{AttributeId, AttributeStore, EntityId, EntityManager, LayerType, Message};
use storm_render::texture_paths::TexturePaths;
use storm_script::{Host, SourceLoader, Value, World};

type NativeCallback = dyn FnMut(&[Value], &World, &EntityManager) -> Result<Value>;
pub(crate) struct NativeFunction(Box<NativeCallback>);
impl NativeFunction {
    pub(crate) fn new(mut function: impl FnMut(&[Value]) -> Result<Value> + 'static) -> Self {
        Self::with_world(move |args, _, _| function(args))
    }

    pub(crate) fn with_world(
        function: impl FnMut(&[Value], &World, &EntityManager) -> Result<Value> + 'static,
    ) -> Self {
        Self(Box::new(function))
    }
}
type TraceSink = Box<dyn FnMut(&str) -> Result<()>>;
type LibraryInitializer = Box<dyn FnMut(&mut NativeHost) -> Result<()>>;
const MAX_NATIVE_FUNCTIONS: usize = 256;

/// The same World supplies script objects and native entity attribute bindings.
/// Native classes/functions are explicitly registered by production subsystems.
pub struct NativeHost {
    world: World,
    pub entities: EntityManager,
    functions: BTreeMap<String, NativeFunction>,
    libraries: BTreeMap<String, LibraryInitializer>,
    loaded_libraries: BTreeSet<String>,
    loading_libraries: BTreeSet<String>,
    script_source: Option<(crate::GameSource, crate::GameDirectory)>,
    trace: TraceSink,
    pub(crate) controls: crate::controls::Controls,
    pub(crate) texture_paths: Rc<RefCell<TexturePaths>>,
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
            libraries: BTreeMap::new(),
            loaded_libraries: BTreeSet::new(),
            loading_libraries: BTreeSet::new(),
            script_source: None,
            trace: Box::new(|text| {
                eprintln!("{text}");
                Ok(())
            }),
            controls: crate::controls::Controls::default(),
            texture_paths: Rc::new(RefCell::new(TexturePaths::default())),
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
            self.functions.len() < MAX_NATIVE_FUNCTIONS,
            "native function registration budget"
        );
        let name = name.to_ascii_lowercase();
        ensure!(
            !self.functions.contains_key(&name),
            "duplicate native function: {name}"
        );
        self.functions.insert(name, NativeFunction::new(function));
        Ok(())
    }
    pub fn set_trace_sink(&mut self, sink: impl FnMut(&str) -> Result<()> + 'static) {
        self.trace = Box::new(sink);
    }
    pub(crate) fn configure_script_source(
        &mut self,
        source: crate::GameSource,
        game: crate::GameDirectory,
    ) {
        self.script_source = Some((source, game));
    }
    pub fn register_library(
        &mut self,
        name: &str,
        initializer: impl FnMut(&mut NativeHost) -> Result<()> + 'static,
    ) -> Result<()> {
        ensure!(
            !name.is_empty()
                && name.len() <= 128
                && name.is_ascii()
                && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
            "invalid native library name"
        );
        let name = name.to_ascii_lowercase();
        ensure!(
            self.libraries.len() + self.loading_libraries.len() < 64,
            "native library registration budget"
        );
        ensure!(
            !self.libraries.contains_key(&name) && !self.loading_libraries.contains(&name),
            "duplicate native library: {name}"
        );
        self.libraries.insert(name, Box::new(initializer));
        Ok(())
    }
    pub(crate) fn register_game_library(
        &mut self,
        name: &str,
        game: &crate::GameDirectory,
    ) -> Result<()> {
        use crate::script_libraries::{
            MATH_LIBRARY, interface, location, math_functions, rigging, steam, textures,
        };
        let name = name.to_ascii_lowercase();
        if self.libraries.contains_key(&name) {
            return Ok(());
        }
        match name.as_str() {
            MATH_LIBRARY => {
                self.register_library(MATH_LIBRARY, |host| host.bind_library(math_functions()))
            }
            textures::RENDERER_LIBRARY => {
                let game = game.clone();
                self.register_library(textures::RENDERER_LIBRARY, move |host| {
                    host.bind_library(textures::functions(
                        game.clone(),
                        host.texture_paths.clone(),
                    ))
                })
            }
            steam::STEAM_LIBRARY => self.register_library(steam::STEAM_LIBRARY, |host| {
                host.bind_library(steam::offline_functions())
            }),
            location::LOCATION_LIBRARY => self
                .register_library(location::LOCATION_LIBRARY, |host| {
                    host.bind_library(location::functions())
                }),
            interface::INTERFACE_LIBRARY => {
                let game = game.clone();
                self.register_library(interface::INTERFACE_LIBRARY, move |host| {
                    host.bind_library(interface::functions(&game)?)
                })
            }
            rigging::RIGGING_LIBRARY => self.register_library(rigging::RIGGING_LIBRARY, |host| {
                host.bind_library(rigging::functions())
            }),
            _ => Ok(()),
        }
    }
    fn bind_library<const N: usize>(
        &mut self,
        functions: [(&str, NativeFunction); N],
    ) -> Result<()> {
        ensure!(
            self.functions.len() + functions.len() <= MAX_NATIVE_FUNCTIONS,
            "native function registration budget"
        );
        for (name, _) in &functions {
            ensure!(
                !self.functions.contains_key(*name),
                "duplicate native function: {name}"
            );
        }
        self.functions
            .extend(functions.map(|(name, function)| (name.to_owned(), function)));
        Ok(())
    }
}
impl Host for NativeHost {
    fn load_script_source(&mut self, path: &str) -> Result<String> {
        let (source, _) = self
            .script_source
            .as_mut()
            .context("runtime script source is not configured")?;
        source.load(path)
    }
    fn script_diagnostic(&mut self, message: &str) {
        eprintln!("{message}");
    }
    fn load_library(&mut self, name: &str) -> Result<()> {
        let name = name.to_ascii_lowercase();
        if self.loaded_libraries.contains(&name) {
            return Ok(());
        }
        ensure!(
            !self.loading_libraries.contains(&name),
            "native library initialization reentry: {name}"
        );
        if let Some(game) = self.script_source.as_ref().map(|(_, game)| game.clone()) {
            self.register_game_library(&name, &game)?;
        }
        let mut initializer = self
            .libraries
            .remove(&name)
            .with_context(|| format!("native script library is not registered: {name}"))?;
        self.loading_libraries.insert(name.clone());
        let result = initializer(self);
        self.loading_libraries.remove(&name);
        self.libraries.insert(name.clone(), initializer);
        result?;
        self.loaded_libraries.insert(name);
        Ok(())
    }
    fn world(&self) -> World {
        self.world.clone()
    }
    fn call(&mut self, name: &str, arguments: &[Value]) -> Result<Value> {
        if let Some(function) = self.functions.get_mut(name) {
            return (function.0)(arguments, &self.world, &self.entities);
        }
        self.controls
            .script_call(name, arguments)
            .with_context(|| format!("native function is not registered: {name}"))?
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
