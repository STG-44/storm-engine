//! XINTERFACE startup state, including its original script callbacks.
mod dialog;
mod fader;
mod info;
mod log;
mod options;
mod progress;
mod quest_text;
mod screenshot;
mod settings;
mod timer;
mod video;

use crate::{
    GameDirectory, NativeHost, WindowSettings, script_libraries::interface::LanguageService,
};
use anyhow::{Context, Result, bail};
use progress::{BorderSelection, PictureLayer};
use quest_text::QuestText;
use std::{
    cell::{Cell, Ref, RefCell},
    collections::BTreeMap,
    rc::Rc,
    sync::Arc,
    time::Instant,
};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityDestroy, EntityId, EntityInit, LayerType, Message,
    Value as Field,
};
use storm_formats::texture::Texture;
use storm_render::{
    fade::FadeDraw,
    interface_text::{BitmapFont, InterfaceDraw, TextDraw},
    loading::LoadingPictures,
    texture_paths::TexturePaths,
};
use storm_script::{Reference, Value};

pub use log::{ActionHint, ActionLayout, LogAndActionsState, LogLayout, LogLine};
pub use progress::LoadingProgress;
pub use settings::{InterfaceCursor, InterfaceScreen};
pub use timer::{BattleTimerState, TimerImage};
const INIT_EVENTS: &[&str] = &["GetQuestTextFileName", "evGetMouseWeel"];
const DEFAULT_LOADING_BORDER: &str = "interfaces\\int_border.tga";

pub struct InterfaceState {
    pub screen: InterfaceScreen,
    pub cursor: Option<InterfaceCursor>,
    pub game_time: [u32; 3],
    binding: Option<AttributeId>,
    dialog: dialog::Dialog,
    dialog_revision: u64,
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
    language: LanguageService,
    fonts: RefCell<BTreeMap<String, std::rc::Weak<BitmapFont>>>,
    draws: RefCell<Vec<(EntityId, InterfaceDraw)>>,
    paths: Rc<RefCell<TexturePaths>>,
    time_scale: Rc<Cell<f32>>,
    window: Cell<Option<[u32; 2]>>,
    pointer: Cell<Option<[f64; 2]>>,
    states: RefCell<BTreeMap<u64, InterfaceState>>,
    logs: RefCell<BTreeMap<u64, LogAndActionsState>>,
    timers: RefCell<BTreeMap<u64, BattleTimerState>>,
    screenshots: RefCell<screenshot::Screenshots>,
    loading_draw: RefCell<Option<Rc<LoadingPictures>>>,
    previous_loading_draw: RefCell<Option<Rc<LoadingPictures>>>,
    progress: RefCell<progress::Progress>,
    fade_draws: RefCell<Vec<FadeDraw>>,
    fader_tips_enabled: Cell<bool>,
    fader_notifications: RefCell<BTreeMap<u64, fader::Notifications>>,
    options_break: Cell<Option<EntityId>>,
    // Each pending completion also indicates whether to enqueue its delayed event.
    finished_videos: RefCell<BTreeMap<u64, bool>>,
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

#[derive(Clone)]
pub(crate) struct Interfaces(Rc<Shared>);

impl Interfaces {
    pub(crate) fn register(
        host: &mut NativeHost,
        game: GameDirectory,
        language: LanguageService,
    ) -> Result<()> {
        let interfaces = Self(Rc::new(Shared {
            game,
            language,
            fonts: RefCell::default(),
            draws: RefCell::default(),
            paths: host.texture_paths.clone(),
            time_scale: host.time_scale.clone(),
            window: Cell::new(None),
            pointer: Cell::new(None),
            states: RefCell::default(),
            logs: RefCell::default(),
            timers: RefCell::default(),
            screenshots: RefCell::default(),
            loading_draw: RefCell::default(),
            previous_loading_draw: RefCell::default(),
            progress: RefCell::default(),
            fade_draws: RefCell::default(),
            fader_tips_enabled: Cell::default(),
            fader_notifications: RefCell::default(),
            options_break: Cell::default(),
            finished_videos: RefCell::default(),
        }));
        let shared = interfaces.0.clone();
        host.entities.register_class("xinterface", move || {
            Ok(Box::new(Actor {
                shared: shared.clone(),
                id: EntityId::INVALID,
            }))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("ILogAndActions", move || {
            Ok(Box::new(log::LogActor::new(shared.clone())))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("BITimer", move || {
            Ok(Box::new(timer::TimerActor::new(shared.clone())))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("CAviPlayer", move || {
            Ok(Box::new(video::VideoActor::new(shared.clone())))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("InfoHandler", move || {
            Ok(Box::new(info::InfoActor::new(shared.clone())))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("Fader", move || {
            Ok(Box::new(fader::FaderActor::new(shared.clone())))
        })?;
        let shared = interfaces.0.clone();
        host.entities.register_class("scrshoter", move || {
            Ok(Box::new(screenshot::Actor::new(shared.clone())))
        })?;
        host.interfaces = Some(interfaces);
        Ok(())
    }

    pub(crate) fn dialog_active(&self, id: EntityId) -> bool {
        self.state(id).is_some_and(|state| state.dialog.active)
    }

    pub(crate) fn active_dialog_revision(&self, id: EntityId) -> Option<u64> {
        self.state(id)
            .filter(|state| state.dialog.active)
            .map(|state| state.dialog_revision)
    }

    pub(crate) fn realize_dialog(&self, id: EntityId, store: &AttributeStore, delta_ms: u32) {
        if let Some(state) = self.0.states.borrow_mut().get_mut(&id.raw()) {
            self.0.draws.borrow_mut().extend(
                state
                    .dialog
                    .draws(store, state.binding, &state.screen, delta_ms)
                    .into_iter()
                    .map(|draw| (id, draw)),
            );
        }
    }

    pub(crate) fn take_button_events(
        &self,
        id: EntityId,
        delta_ms: u32,
        store: &mut AttributeStore,
    ) -> Result<dialog::NodeEvents> {
        let mut states = self.0.states.borrow_mut();
        let Some(state) = states
            .get_mut(&id.raw())
            .filter(|state| state.dialog.active)
        else {
            return Ok(Vec::new());
        };
        let events = state.dialog.nodes.take_button_events(delta_ms)?;
        state
            .dialog
            .nodes
            .publish_widget_attributes(store, state.binding)?;
        Ok(events)
    }

    pub(crate) fn button_input(
        &self,
        id: EntityId,
        controls: &crate::controls::Controls,
        store: &mut AttributeStore,
    ) -> Result<dialog::NodeEvents> {
        let mut states = self.0.states.borrow_mut();
        let Some(state) = states
            .get_mut(&id.raw())
            .filter(|state| state.dialog.active)
        else {
            return Ok(Vec::new());
        };
        let mut point = self.0.pointer.get();
        if let Some(point) = &mut point {
            let viewport = self.0.viewport()?;
            for (axis, value) in point.iter_mut().enumerate() {
                *value = (*value * f64::from(state.screen.size[axis]) / f64::from(viewport[axis]))
                    .trunc();
            }
        }
        state
            .dialog
            .nodes
            .edit_input(controls, store, state.binding)?;
        let mut events = state.dialog.nodes.button_input(controls, point)?;
        state.dialog.nodes.edit_exit(controls, &mut events)?;
        state
            .dialog
            .nodes
            .publish_widget_attributes(store, state.binding)?;
        Ok(events)
    }

    pub(crate) fn editor_focused(&self) -> bool {
        self.0.states.borrow().values().any(|state| {
            state.dialog.active && state.binding.is_some() && state.dialog.nodes.editor_focused()
        })
    }

    pub(crate) fn has_node_callback(&self, id: EntityId) -> bool {
        self.0
            .states
            .borrow()
            .get(&id.raw())
            .is_some_and(|state| state.dialog.nodes.has_check_callback())
    }

    pub(crate) fn take_node_callback(&self, id: EntityId) -> Option<(&'static str, Message)> {
        let mut states = self.0.states.borrow_mut();
        let message = states
            .get_mut(&id.raw())?
            .dialog
            .nodes
            .take_check_callback()?;
        Some((dialog::CHECK_EVENT, message))
    }

    fn publish_dialog_attributes(
        &self,
        id: EntityId,
        store: &mut AttributeStore,
        init_edits: bool,
    ) -> Result<()> {
        if let Some(state) = self.0.states.borrow_mut().get_mut(&id.raw()) {
            if init_edits {
                state
                    .dialog
                    .nodes
                    .initialize_attributes(store, state.binding)?;
            }
            state
                .dialog
                .nodes
                .publish_widget_attributes(store, state.binding)?;
        }
        Ok(())
    }

    pub fn set_pointer(&self, point: Option<[f64; 2]>) {
        self.0.pointer.set(point);
    }

    pub(crate) fn draws(&self) -> Vec<InterfaceDraw> {
        self.0
            .draws
            .borrow()
            .iter()
            .map(|(_, draw)| draw.clone())
            .collect()
    }

    pub(crate) fn text_draws(&self) -> Vec<TextDraw> {
        self.0
            .draws
            .borrow()
            .iter()
            .filter_map(|(_, draw)| match draw {
                InterfaceDraw::Text(text) => Some(text.clone()),
                InterfaceDraw::Picture(_) => None,
            })
            .collect()
    }

    pub(crate) fn window_state(&self, id: EntityId, name: &str, value: bool, visibility: bool) {
        if let Some(state) = self.0.states.borrow_mut().get_mut(&id.raw()) {
            state.dialog.nodes.window(name, value, visibility);
            self.0.draws.borrow_mut().retain(|(owner, _)| *owner != id);
        }
    }

    pub(crate) fn window_active(&self, id: EntityId, name: &str) -> bool {
        self.state(id)
            .is_some_and(|state| state.dialog.nodes.window_active(name))
    }

    pub(crate) fn take_dialog_post(&self, id: EntityId) -> Option<(&'static str, i32)> {
        let mut states = self.0.states.borrow_mut();
        std::mem::take(&mut states.get_mut(&id.raw())?.dialog.missing).then_some(("exitCancel", 1))
    }

    pub fn set_window(&self, width: u32, height: u32) {
        if width != 0 && height != 0 {
            self.0.window.set(Some([width, height]));
        }
    }

    pub fn state(&self, id: EntityId) -> Option<Ref<'_, InterfaceState>> {
        Ref::filter_map(self.0.states.borrow(), |states| states.get(&id.raw())).ok()
    }

    pub(crate) fn take_event(&self, id: EntityId) -> Option<&'static str> {
        if self.0.options_break.get() == Some(id) {
            self.0.options_break.set(None);
            Some("evntOptionsBreak")
        } else {
            None
        }
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
    pub(crate) fn screenshot_pending(&self, id: EntityId) -> bool {
        self.0.screenshots.borrow().pending(id)
    }
    pub(crate) fn finish_screenshot(&self, id: EntityId, frame: storm_render::Frame) -> Result<()> {
        screenshot::finish(&self.0, id, frame)
    }
    pub(crate) fn screenshot_image(&self, handle: i32) -> Option<Rc<storm_formats::texture::Mip>> {
        self.0.screenshots.borrow().image(handle)
    }
    pub fn clear_draws(&self) {
        self.0.draws.borrow_mut().clear();
        self.0.loading_draw.replace(None);
        self.0.previous_loading_draw.replace(None);
        self.0.fade_draws.borrow_mut().clear();
        self.end_loading_progress();
    }

    pub fn begin_frame(&self) {
        self.0.draws.borrow_mut().clear();
        self.0.fade_draws.borrow_mut().clear();
        self.0
            .previous_loading_draw
            .replace(self.0.loading_draw.take());
    }

    pub fn loading_draw(&self) -> Option<Rc<LoadingPictures>> {
        self.0.loading_draw.borrow().clone()
    }

    pub fn fade_draws(&self) -> Ref<'_, [FadeDraw]> {
        Ref::map(self.0.fade_draws.borrow(), Vec::as_slice)
    }

    pub fn take_fader_notifications(&self, id: EntityId) -> Option<fader::Notifications> {
        self.0.fader_notifications.borrow_mut().remove(&id.raw())
    }

    pub fn set_loading_progress_foreground(&self, name: String) {
        self.0
            .progress
            .borrow_mut()
            .set_picture(PictureLayer::Foreground, name);
    }

    pub fn fader_tips_enabled(&self) -> bool {
        self.0.fader_tips_enabled.get()
    }

    pub fn start_loading_progress(&self) -> Result<()> {
        self.0
            .progress
            .borrow_mut()
            .start(Instant::now(), |selection| {
                let pictures = self
                    .loading_draw()
                    .or_else(|| self.0.previous_loading_draw.borrow().clone());
                if selection.background.is_none()
                    && selection.foreground.is_none()
                    && matches!(selection.border, BorderSelection::Inherit)
                {
                    return Ok(pictures);
                }
                let mut pictures = pictures.as_deref().cloned().unwrap_or_default();
                if let Some(name) = &selection.background {
                    pictures.background = load_loading_picture(&self.0, name)?;
                }
                if let Some(name) = &selection.foreground {
                    pictures.foreground = load_loading_picture(&self.0, name)?;
                }
                if pictures.foreground.is_none() {
                    pictures.border = None;
                } else if let BorderSelection::Selected(border) = &selection.border {
                    pictures.border = border.clone();
                }
                Ok((pictures.background.is_some()
                    || pictures.foreground.is_some()
                    || pictures.text.is_some())
                .then(|| Rc::new(pictures)))
            })
    }

    pub fn update_loading_progress(&self) -> Result<()> {
        self.0.progress.borrow_mut().update(Instant::now())
    }

    pub fn end_loading_progress(&self) {
        self.0.progress.borrow_mut().end();
    }

    pub fn loading_progress(&self) -> Option<LoadingProgress> {
        self.0.progress.borrow().view()
    }

    pub fn set_progress_presenter(
        &self,
        presenter: impl FnMut(&LoadingProgress) -> Result<()> + 'static,
    ) {
        self.0.progress.borrow_mut().set_presenter(presenter);
    }
    pub fn take_video_event(&self, id: EntityId) -> Option<&'static str> {
        self.0
            .finished_videos
            .borrow_mut()
            .remove(&id.raw())
            .map(|_| video::END_EVENT)
    }
    pub fn take_video_post(&self, id: EntityId) -> Option<(&'static str, i32)> {
        let mut videos = self.0.finished_videos.borrow_mut();
        let needs_post = videos.get_mut(&id.raw())?;
        std::mem::take(needs_post).then_some((video::END_EVENT, 1))
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

struct Actor {
    shared: Rc<Shared>,
    id: EntityId,
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        let shared = &self.shared;
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
                binding: context.attributes,
                dialog: dialog::Dialog::default(),
                dialog_revision: 0,
                quests: QuestText::default(),
                wheel: None,
            },
        );
        Ok(true)
    }

    fn destroy(&mut self, context: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared.states.borrow_mut().remove(&context.id.raw());
        self.shared
            .draws
            .borrow_mut()
            .retain(|(id, _)| *id != context.id);
        if self.shared.options_break.get() == Some(context.id) {
            self.shared.options_break.set(None);
        }
        Ok(())
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        self.receive(message, None, None)
    }

    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        attributes: &mut AttributeStore,
    ) -> Result<u64> {
        match message.long()? {
            options::MSG_INTERFACE_SAVEOPTIONS => {
                let path = message.string()?.to_owned();
                let root = message.attribute()?;
                options::save(&self.shared.game, &path, attributes, root)?;
                Ok(0)
            }
            options::MSG_INTERFACE_LOADOPTIONS => {
                let path = message.string()?.to_owned();
                let root = message.attribute()?;
                if options::load(&self.shared.game, &path, attributes, root)? {
                    self.shared.options_break.set(Some(self.id));
                }
                Ok(0)
            }
            code => {
                let title = (code == dialog::INIT)
                    .then(|| {
                        self.shared
                            .states
                            .borrow()
                            .get(&self.id.raw())
                            .and_then(|state| state.binding)
                    })
                    .flatten()
                    .and_then(|root| attributes.value(root, Some("title")));
                message.move_to_start();
                let result = self.receive(message, title, Some(attributes))?;
                if matches!(code, dialog::INIT | dialog::MSG_TO_NODE) {
                    Interfaces(self.shared.clone()).publish_dialog_attributes(
                        self.id,
                        attributes,
                        code == dialog::INIT,
                    )?;
                }
                Ok(result)
            }
        }
    }
}

impl Actor {
    fn receive(
        &mut self,
        message: &mut Message,
        title: Option<&str>,
        store: Option<&AttributeStore>,
    ) -> Result<u64> {
        let code = message.long()?;
        let mut states = self.shared.states.borrow_mut();
        let state = states
            .get_mut(&self.id.raw())
            .context("missing XINTERFACE state")?;
        use dialog::{
            CHANGE_COLOR, CREATE_STRING, DELETE_STRING, DISABLE_STRING, ENABLE_STRING,
            GET_CURRENT_NODE, GET_SELECTABLE, INIT, MSG_TO_NODE, NEW_CURRENT_NODE, RELEASE,
            SET_FORMATTED_TEXT, SET_NODE_USING, SET_SELECTABLE,
        };
        match code {
            INIT => {
                state
                    .dialog
                    .activate(&self.shared, message.string()?, &state.screen, title)?;
                state.dialog_revision = state.dialog_revision.wrapping_add(1);
            }
            SET_NODE_USING => {
                let name = message.string()?.to_owned();
                let show = message.long()? != 0;
                state.dialog.nodes.set_using(&name, show);
            }
            MSG_TO_NODE => {
                let result = state.dialog.nodes.send(
                    &self.shared,
                    &state.screen,
                    message,
                    store.zip(state.binding),
                )?;
                self.shared
                    .draws
                    .borrow_mut()
                    .retain(|(id, _)| *id != self.id);
                return Ok(result);
            }
            SET_FORMATTED_TEXT => {
                let node = message.string()?.to_owned();
                state
                    .dialog
                    .nodes
                    .set_formatted_text(&node, message.string()?)?;
            }
            NEW_CURRENT_NODE => state.dialog.nodes.set_focus(message.string()?),
            GET_CURRENT_NODE => {
                message
                    .script_variable()?
                    .set(storm_core::ScriptScalar::String(
                        state.dialog.nodes.focused_name().to_owned(),
                    ))?;
                return Ok(0);
            }
            SET_SELECTABLE => {
                let name = message.string()?.to_owned();
                let selectable = message.long()? != 0;
                state.dialog.nodes.set_selectable(&name, selectable);
            }
            GET_SELECTABLE => {
                return Ok(u64::from(state.dialog.nodes.selectable(message.string()?)));
            }
            RELEASE => {
                state.dialog = dialog::Dialog::default();
                state.dialog_revision = state.dialog_revision.wrapping_add(1);
            }
            CREATE_STRING => state.dialog.create_string(&self.shared, message)?,
            ENABLE_STRING | DISABLE_STRING | DELETE_STRING | CHANGE_COLOR => {
                state.dialog.update_string(code, message)?;
            }
            _ => bail!("unsupported XINTERFACE message {code}"),
        }
        // A later Realize publishes the replacement dialog and its strings.
        self.shared
            .draws
            .borrow_mut()
            .retain(|(id, _)| *id != self.id);
        Ok(0)
    }
}
fn load_loading_picture(shared: &Shared, name: &str) -> Result<Option<Arc<Texture>>> {
    if name.is_empty() {
        return Ok(None);
    }
    Ok(load_texture(shared, name)?.map(Arc::new))
}

fn load_texture(shared: &Shared, name: &str) -> Result<Option<Texture>> {
    shared.game.load_texture(&shared.paths.borrow(), name)
}
