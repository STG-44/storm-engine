//! XINTERFACE startup state, including its original script callbacks.
mod log;
mod quest_text;
mod settings;
mod timer;

use crate::{GameDirectory, NativeHost, WindowSettings};
use anyhow::{Context, Result, bail};
use quest_text::QuestText;
use std::{
    cell::{Cell, Ref, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, LayerType, Message, Value as Field};
use storm_formats::texture::Texture;
use storm_render::texture_paths::TexturePaths;
use storm_script::{Reference, Value};

pub use log::{ActionHint, ActionLayout, LogAndActionsState, LogLayout, LogLine};
pub use settings::{InterfaceCursor, InterfaceScreen};
pub use timer::{BattleTimerState, TimerImage};
const INIT_EVENTS: &[&str] = &["GetQuestTextFileName", "evGetMouseWeel"];

pub struct InterfaceState {
    pub screen: InterfaceScreen,
    pub cursor: Option<InterfaceCursor>,
    pub game_time: [u32; 3],
    quests: QuestText,
    wheel: Option<Reference>,
}

impl InterfaceState {
    pub fn quest_title(&self, id: &str) -> Option<&str> {
        self.quests.title(id)
    }
    pub fn quest_text(&self, id: &str, entry: &str) -> Option<&str> {
        self.quests.text(id, entry)
    }
    pub fn wheel_steps(&self) -> Result<i32> {
        self.wheel.as_ref().map_or(Ok(0), |cell| {
            Value::Reference(cell.clone()).resolved()?.integer()
        })
    }
}

struct Shared {
    game: GameDirectory,
    paths: Rc<RefCell<TexturePaths>>,
    window: Cell<Option<[u32; 2]>>,
    states: RefCell<BTreeMap<u64, InterfaceState>>,
    logs: RefCell<BTreeMap<u64, LogAndActionsState>>,
    timers: RefCell<BTreeMap<u64, BattleTimerState>>,
}

impl Shared {
    fn viewport(&self) -> Result<[u32; 2]> {
        if let Some(window) = self.window.get() {
            return Ok(window);
        }
        let settings = WindowSettings::load(&self.game)?;
        Ok([settings.width, settings.height])
    }
}

pub(crate) struct Interfaces(Rc<Shared>);

impl Interfaces {
    pub(crate) fn register(host: &mut NativeHost, game: GameDirectory) -> Result<()> {
        let interfaces = Self(Rc::new(Shared {
            game,
            paths: host.texture_paths.clone(),
            window: Cell::new(None),
            states: RefCell::default(),
            logs: RefCell::default(),
            timers: RefCell::default(),
        }));
        let shared = interfaces.0.clone();
        host.entities
            .register_class("xinterface", move || Ok(Box::new(Actor(shared.clone()))))?;
        let shared = interfaces.0.clone();
        host.entities.register_class("ILogAndActions", move || {
            Ok(Box::new(log::LogActor::new(shared.clone())))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("BITimer", move || {
            Ok(Box::new(timer::TimerActor::new(shared.clone())))
        })?;
        host.interfaces = Some(interfaces);
        Ok(())
    }

    pub fn set_window(&self, width: u32, height: u32) {
        if width != 0 && height != 0 {
            self.0.window.set(Some([width, height]));
        }
    }

    pub fn state(&self, id: EntityId) -> Option<Ref<'_, InterfaceState>> {
        Ref::filter_map(self.0.states.borrow(), |states| states.get(&id.raw())).ok()
    }

    pub fn log(&self, id: EntityId) -> Option<Ref<'_, LogAndActionsState>> {
        Ref::filter_map(self.0.logs.borrow(), |logs| logs.get(&id.raw())).ok()
    }
    pub fn timer(&self, id: EntityId) -> Option<Ref<'_, BattleTimerState>> {
        Ref::filter_map(self.0.timers.borrow(), |timers| timers.get(&id.raw())).ok()
    }

    pub fn take_timer_event(&self, id: EntityId) -> Option<String> {
        self.0.timers.borrow_mut().get_mut(&id.raw())?.take_event()
    }

    pub fn events(&self, id: EntityId) -> Result<Vec<(&'static str, Message)>> {
        if self.0.states.borrow().contains_key(&id.raw()) {
            return INIT_EVENTS
                .iter()
                .map(|&name| Ok((name, Message::new("", vec![])?)))
                .collect();
        }
        if let Some(log) = self.log(id) {
            let [width, height] = log.viewport;
            let payload = [width as i32, height as i32, 0]
                .into_iter()
                .map(Field::Long)
                .collect();
            return Ok(vec![("SetWindowSize", Message::new("lll", payload)?)]);
        }
        Ok(Vec::new())
    }

    pub fn initialize_render_attributes(&self, render: &storm_script::Object) -> Result<()> {
        let [width, height] = self.0.viewport()?;
        let fullscreen = WindowSettings::load(&self.0.game)?.fullscreen;
        let root = render.root();
        for (name, value) in [
            ("full_screen", u32::from(fullscreen)),
            ("screen_x", width),
            ("screen_y", height),
        ] {
            root.create_path(name)?.set_value(&value.to_string())?;
        }
        Ok(())
    }

    pub fn result(&self, id: EntityId, event: &str, value: Option<Value>) -> Result<()> {
        if event == "SetWindowSize" {
            return Ok(());
        }
        let mut states = self.0.states.borrow_mut();
        let state = states
            .get_mut(&id.raw())
            .context("interface instance was removed during Init")?;
        let Some(value) = value else { return Ok(()) };
        match event {
            "GetQuestTextFileName" => {
                let value = value.resolved()?;
                if let Value::Array(files) = value {
                    for index in 0..files.len() {
                        let file = files.element_value(index as i32)?;
                        state.quests.load(&self.0.game, file.text()?)?;
                    }
                } else if let Value::String(file) = value {
                    state.quests.load(&self.0.game, &file)?;
                }
            }
            "evGetMouseWeel" => {
                if let Value::Reference(cell) = value {
                    state.wheel = Some(cell);
                }
            }
            _ => unreachable!("interface initialization event"),
        }
        Ok(())
    }
}

struct Actor(Rc<Shared>);

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        let shared = &self.0;
        let window = shared.viewport()?;
        let (screen, mut cursor) = settings::load(&shared.game, window)?;
        if let Some(cursor) = &mut cursor {
            cursor.texture = load_texture(shared, &cursor.name)?;
        }
        let game_time = context
            .attributes
            .and_then(|root| context.store.child(root, "GameTime"))
            .map_or([0; 3], |root| {
                ["sec", "min", "hour"].map(|name| context.store.as_dword(root, Some(name), 0))
            });
        context.manager.set_layer_type(4, LayerType::Execute)?;
        context.manager.set_layer_type(5, LayerType::Realize)?;
        shared.states.borrow_mut().insert(
            context.id.raw(),
            InterfaceState {
                screen,
                cursor,
                game_time,
                quests: QuestText::default(),
                wheel: None,
            },
        );
        Ok(true)
    }

    fn destroy(&mut self, context: &mut EntityDestroy<'_>) -> Result<()> {
        self.0.states.borrow_mut().remove(&context.id.raw());
        Ok(())
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        bail!("unsupported XINTERFACE message {}", message.long()?);
    }
}

fn load_texture(shared: &Shared, name: &str) -> Result<Option<Texture>> {
    for candidate in shared.paths.borrow().candidates(name)? {
        let path = format!("RESOURCE/Textures/{}", candidate.file);
        if let Some(bytes) = shared.game.read_optional(&path, 16 * 1024 * 1024)?
            && let Ok(texture) = Texture::read(&bytes)
        {
            return Ok(Some(texture));
        }
    }
    Ok(None)
}
