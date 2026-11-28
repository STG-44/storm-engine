use crate::interface::{Interfaces, LoadingProgress};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{Cell, Ref, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
};
use storm_core::{AttributeId, AttributeStore, EntityId, EntityManager, LayerType, Message};
use storm_render::{
    GlowParameters, color::ColorCorrection, fade::FadeDraw, texture_paths::TexturePaths,
};
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
    pub(crate) models: Option<crate::model::Models>,
    functions: BTreeMap<String, NativeFunction>,
    libraries: BTreeMap<String, LibraryInitializer>,
    loaded_libraries: BTreeSet<String>,
    loading_libraries: BTreeSet<String>,
    script_source: Option<(crate::GameSource, crate::GameDirectory)>,
    trace: TraceSink,
    pub(crate) random: Rc<RefCell<crate::random::Random>>,
    pub(crate) controls: crate::controls::Controls,
    pub(crate) interfaces: Option<Interfaces>,
    pub(crate) back_scenes: Option<crate::back_scene::BackScenes>,
    pub(crate) coast_foams: Option<crate::coast_foam::CoastFoams>,
    pub(crate) seas: Option<crate::sea::Seas>,
    pub(crate) ships: Option<crate::ship::Ships>,
    pub(crate) ship_lights: Option<crate::ship_lights::ShipLights>,
    pub(crate) weather: Option<crate::weather::Weather>,
    pub(crate) flags: Option<crate::flag::Flags>,
    pub(crate) rain: Option<crate::rain::Rain>,
    pub(crate) lightning: Option<crate::lightning::Lightning>,
    pub(crate) sun_glow: Option<crate::sun_glow::SunGlow>,
    pub(crate) astronomy: Option<crate::astronomy::Astronomy>,
    pub(crate) skies: Option<crate::sky::Skies>,
    pub(crate) interface_language: Option<crate::script_libraries::interface::LanguageService>,
    pub(crate) texture_paths: Rc<RefCell<TexturePaths>>,
    pub(crate) time_scale: Rc<Cell<f32>>,
    pub(crate) ambient_light: Rc<Cell<Option<[u8; 3]>>>,
    glow_parameters: Rc<Cell<GlowParameters>>,
    color_correction: Rc<Cell<Option<ColorCorrection>>>,
    pub(crate) delta_ms: u32,
}
impl NativeHost {
    pub fn new() -> Result<Self> {
        Self::with_random_seed(crate::random::Random::clock_seed())
    }
    pub(crate) fn with_random_seed(seed: u32) -> Result<Self> {
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
            models: None,
            functions: BTreeMap::new(),
            libraries: BTreeMap::new(),
            loaded_libraries: BTreeSet::new(),
            loading_libraries: BTreeSet::new(),
            script_source: None,
            trace: Box::new(|text| {
                eprintln!("{text}");
                Ok(())
            }),
            random: Rc::new(RefCell::new(crate::random::Random::new(seed))),
            controls: crate::controls::Controls::default(),
            interfaces: None,
            back_scenes: None,
            seas: None,
            coast_foams: None,
            ships: None,
            ship_lights: None,
            weather: None,
            flags: None,
            rain: None,
            lightning: None,
            sun_glow: None,
            astronomy: None,
            skies: None,
            interface_language: None,
            texture_paths: Rc::new(RefCell::new(TexturePaths::default())),
            time_scale: Rc::new(Cell::new(1.0)),
            ambient_light: Rc::default(),
            glow_parameters: Rc::default(),
            color_correction: Rc::default(),
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
    pub fn set_interface_window(&mut self, width: u32, height: u32) {
        if let Some(interfaces) = &self.interfaces {
            interfaces.set_window(width, height);
        }
        if let Some(scenes) = &self.back_scenes {
            scenes.set_viewport([width, height]);
        }
    }
    pub fn set_interface_pointer(&mut self, pointer: Option<[f64; 2]>) {
        if let Some(interfaces) = &self.interfaces {
            interfaces.set_pointer(pointer);
        }
        if let Some(scenes) = &self.back_scenes {
            scenes.set_pointer(pointer);
        }
    }
    pub fn interface_editor_focused(&self) -> bool {
        self.interfaces
            .as_ref()
            .is_some_and(Interfaces::editor_focused)
    }
    pub fn authored_camera(&self) -> Option<storm_render::model::OrbitCamera> {
        self.back_scenes.as_ref()?.camera(&self.entities)
    }
    pub fn glow_parameters(&self) -> GlowParameters {
        self.glow_parameters.get()
    }
    pub fn color_correction(&self) -> Option<ColorCorrection> {
        self.color_correction.get()
    }
    /// Lets a synchronous loading presenter observe changes made inside the VM.
    pub fn color_correction_reader(&self) -> impl Fn() -> Option<ColorCorrection> + 'static {
        let state = self.color_correction.clone();
        move || state.get()
    }
    pub fn mouse_axis(&self, name: &str) -> Option<f32> {
        self.controls.mouse_axis(name)
    }
    pub fn screenshot_image(&self, handle: i32) -> Option<Rc<storm_formats::texture::Mip>> {
        self.interfaces.as_ref()?.screenshot_image(handle)
    }
    pub fn coast_foam_draws(&self) -> Vec<storm_render::model::CoastFoamDraw> {
        self.coast_foams
            .as_ref()
            .map_or_else(Vec::new, |foams| foams.draws())
    }
    pub fn sea_draw(&self) -> Option<crate::sea::Surface> {
        self.seas.as_ref()?.draw()
    }
    pub fn rain_draws(&self) -> Vec<storm_render::model::RainDraw> {
        self.rain
            .as_ref()
            .map_or_else(Vec::new, |rain| rain.draws())
    }
    pub fn sun_draws(&self) -> Vec<storm_render::model::SunDraw> {
        self.sun_glow
            .as_ref()
            .map_or_else(Vec::new, |sun| sun.draws())
    }
    pub fn astronomy_draws(&self) -> Vec<storm_render::model::CelestialDraw> {
        self.astronomy
            .as_ref()
            .map_or_else(Vec::new, |astronomy| astronomy.draws())
    }
    pub fn lightning_draws(&self) -> Vec<storm_render::model::LightningDraw> {
        self.lightning
            .as_ref()
            .map_or_else(Vec::new, |lightning| lightning.draws())
    }
    pub fn sky_draw(&self) -> Option<storm_render::model::SkyDraw> {
        self.skies.as_ref()?.draw()
    }
    pub fn sea_surface(&self, id: EntityId) -> Option<crate::sea::Surface> {
        self.entities.is_valid(id).then_some(())?;
        self.seas.as_ref()?.surface(id)
    }
    pub fn loading_draw(&self) -> Option<Rc<storm_render::loading::LoadingPictures>> {
        self.interfaces.as_ref()?.loading_draw()
    }
    pub fn interface_draws(&self) -> Vec<storm_render::interface_text::InterfaceDraw> {
        self.interfaces
            .as_ref()
            .map_or_else(Vec::new, Interfaces::draws)
    }
    pub fn interface_text_draws(&self) -> Vec<storm_render::interface_text::TextDraw> {
        self.interfaces
            .as_ref()
            .map_or_else(Vec::new, Interfaces::text_draws)
    }
    pub fn fade_draws(&self) -> Option<Ref<'_, [FadeDraw]>> {
        self.interfaces.as_ref().map(Interfaces::fade_draws)
    }
    /// Combined opacity of the full-screen fade layers.
    pub fn fade_opacity(&self) -> f32 {
        self.fade_draws()
            .as_deref()
            .unwrap_or_default()
            .iter()
            .fold(0.0, |opacity, draw| {
                opacity + (1.0 - opacity) * draw.opacity
            })
    }
    pub fn loading_progress(&self) -> Option<LoadingProgress> {
        self.interfaces.as_ref()?.loading_progress()
    }
    pub fn set_progress_presenter(
        &mut self,
        presenter: impl FnMut(&LoadingProgress) -> Result<()> + 'static,
    ) {
        if let Some(interfaces) = &self.interfaces {
            interfaces.set_progress_presenter(presenter);
        }
    }
    pub fn interface_state(
        &self,
        id: EntityId,
    ) -> Option<std::cell::Ref<'_, crate::interface::InterfaceState>> {
        if !self.entities.is_valid(id) {
            return None;
        }
        self.interfaces.as_ref()?.state(id)
    }
    pub fn log_and_actions_state(
        &self,
        id: EntityId,
    ) -> Option<std::cell::Ref<'_, crate::interface::LogAndActionsState>> {
        if !self.entities.is_valid(id) {
            return None;
        }
        self.interfaces.as_ref()?.log(id)
    }
    pub fn battle_timer_state(
        &self,
        id: EntityId,
    ) -> Option<std::cell::Ref<'_, crate::interface::BattleTimerState>> {
        if !self.entities.is_valid(id) {
            return None;
        }
        self.interfaces.as_ref()?.timer(id)
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
                        host.glow_parameters.clone(),
                    ))
                })
            }
            steam::STEAM_LIBRARY => self.register_library(steam::STEAM_LIBRARY, |host| {
                host.bind_library(steam::offline_functions())
            }),
            location::LOCATION_LIBRARY => {
                self.register_library(location::LOCATION_LIBRARY, |host| {
                    let interfaces = host
                        .interfaces
                        .clone()
                        .context("native interface unavailable")?;
                    host.bind_library(location::functions(
                        interfaces,
                        host.random.clone(),
                        host.ambient_light.clone(),
                    ))
                })
            }
            interface::INTERFACE_LIBRARY => {
                let game = game.clone();
                self.register_library(interface::INTERFACE_LIBRARY, move |host| {
                    let language = host
                        .interface_language
                        .clone()
                        .unwrap_or_else(|| interface::LanguageService::new(game.clone()));
                    host.bind_library(interface::functions(
                        &language,
                        host.color_correction.clone(),
                        host.interfaces
                            .clone()
                            .context("native interface unavailable")?,
                    )?)
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
        if name == "settimescale" {
            let scale = match arguments {
                [Value::Float(scale)] => *scale,
                [Value::Int(scale)] => *scale as f32,
                _ => bail!("SetTimeScale expects one numeric argument"),
            };
            self.time_scale.set(scale);
            return Ok(Value::Void);
        }
        self.controls
            .script_call(name, arguments)
            .or_else(|| self.random.borrow_mut().script_call(name, arguments))
            .with_context(|| format!("native function is not registered: {name}"))?
    }
    fn is_entity(&self, id: EntityId) -> Result<bool> {
        Ok(self.entities.is_valid(id))
    }
    fn delete_entity(&mut self, id: EntityId) -> Result<()> {
        self.entities.erase(id);
        Ok(())
    }
    fn delete_entities(&mut self) -> Result<()> {
        self.entities.erase_all()
    }
    fn delete_entities_by_type(&mut self, class: &str) -> Result<()> {
        for id in self.entities.ids(class) {
            self.entities.erase(id);
        }
        Ok(())
    }
    fn entity_attribute_root(&self, id: EntityId) -> Option<AttributeId> {
        self.entities.attribute_root(id)
    }
    fn first_entity_of_type(&self, class: &str) -> Result<Option<EntityId>> {
        Ok(self.entities.first_id(class))
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
    fn entity_init_events(&self, entity: EntityId) -> Result<Vec<(&'static str, Message)>> {
        self.interfaces
            .as_ref()
            .map_or_else(|| Ok(Vec::new()), |interfaces| interfaces.events(entity))
    }
    fn entity_init_result(
        &mut self,
        entity: EntityId,
        event: &str,
        value: Option<Value>,
    ) -> Result<()> {
        self.interfaces
            .as_ref()
            .context("native interface service unavailable")?
            .result(entity, event, value)
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
    fn entity_message_has_callbacks(&self, id: EntityId, message: &Message) -> bool {
        self.interfaces
            .as_ref()
            .is_some_and(|interfaces| interfaces.has_node_callback(id))
            || (matches!(
                message.values().first(),
                Some(storm_core::Value::Long(crate::ship::CREATE))
            ) && self.ships.as_ref().is_some_and(|ships| ships.mounting(id)))
    }
    fn next_entity_message_callback(
        &mut self,
        id: EntityId,
        store: &mut AttributeStore,
    ) -> Result<Option<(&'static str, Message)>> {
        if let Some(callback) = self
            .interfaces
            .as_ref()
            .and_then(|interfaces| interfaces.take_node_callback(id))
        {
            return Ok(Some(callback));
        }
        match self.ships.as_ref().filter(|ships| ships.mounting(id)) {
            Some(ships) => ships.next_callback(id, &mut self.entities, store),
            None => Ok(None),
        }
    }
    fn abort_entity_message_callbacks(&mut self, id: EntityId) -> Result<()> {
        if let Some(ships) = &self.ships {
            ships.abort_mount(id, &mut self.entities)?;
        }
        Ok(())
    }
    fn take_entity_event(&mut self, id: EntityId) -> Option<&'static str> {
        self.interfaces.as_ref()?.take_event(id)
    }
    fn take_entity_posts(&mut self, id: EntityId) -> Vec<(&'static str, i32)> {
        let Some(interfaces) = &self.interfaces else {
            return Vec::new();
        };
        interfaces
            .take_video_post(id)
            .into_iter()
            .chain(interfaces.take_dialog_post(id))
            .collect()
    }
    fn event_handler_instructions(&self, event: &str, function: &str) -> Option<u64> {
        (self.script_source.is_some() && event == "frame" && function == "main_start")
            .then_some(crate::config::GAME_MAIN_START_INSTRUCTIONS)
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
