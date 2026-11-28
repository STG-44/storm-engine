//! The scripted 3D interface background shares MODELR assets and the configured view.
use crate::{
    GameDirectory, NativeHost, WindowSettings,
    config::{configuration_text, root_fields},
    controls::Controls,
    model::{Draw, DrawPart, IDENTITY, Models},
    script_libraries::interface::LanguageService,
};
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use storm_core::{
    AttributeId, AttributeStore, Entity, EntityDestroy, EntityId, EntityInit, EntityManager,
    Message, Stage,
};
use storm_formats::geometry::{Geometry, Label};
use storm_render::model::{ModelAsset, OrbitCamera, native_model_matrix};

const MAX_LOCATOR_BYTES: usize = 64 * 1024 * 1024;
const MAX_MENU_ITEMS: usize = 32;

#[derive(Clone)]
pub(crate) struct BackScenes(Rc<Shared>);

struct Shared {
    game: GameDirectory,
    models: Option<Models>,
    language: LanguageService,
    live: RefCell<BTreeMap<u64, Rc<RefCell<State>>>>,
    active: Cell<Option<EntityId>>,
    pointer: Cell<Option<[f64; 2]>>,
    viewport: Cell<Option<[u32; 2]>>,
}

#[derive(Default)]
struct State {
    background: Option<Rc<ModelAsset>>,
    locators: Vec<Label>,
    camera: Option<OrbitCamera>,
    items: Vec<MenuItem>,
    selected: Option<usize>,
    previous_pointer: Option<[f64; 2]>,
}

struct MenuItem {
    matrix: [f32; 16],
    active: Option<Rc<ModelAsset>>,
    passive: Option<Rc<ModelAsset>>,
    event: String,
    selectable: bool,
}

pub(crate) struct Notifications {
    pub camera: Option<([f32; 3], [f32; 3])>,
    pub selection_changed: bool,
    pub commands: Vec<String>,
}

impl BackScenes {
    pub(crate) fn register(
        host: &mut NativeHost,
        game: GameDirectory,
        language: LanguageService,
    ) -> Result<()> {
        let service = Self(Rc::new(Shared {
            game,
            models: host.models.clone(),
            language,
            live: RefCell::default(),
            active: Cell::default(),
            pointer: Cell::default(),
            viewport: Cell::default(),
        }));
        let factory = service.clone();
        if host
            .entities
            .register_class("InterfaceBackScene", move || {
                Ok(Box::new(Actor {
                    shared: factory.clone(),
                    id: EntityId::INVALID,
                    state: Rc::default(),
                }))
            })?
        {
            host.back_scenes = Some(service);
        }
        Ok(())
    }

    pub(crate) fn set_pointer(&self, pointer: Option<[f64; 2]>) {
        self.0.pointer.set(pointer);
    }

    pub(crate) fn set_viewport(&self, size: [u32; 2]) {
        self.0.viewport.set(Some(size));
    }

    pub(crate) fn camera(&self, entities: &EntityManager) -> Option<OrbitCamera> {
        let id = self.0.active.get().filter(|&id| entities.is_valid(id))?;
        self.0.live.borrow().get(&id.raw())?.borrow().camera.clone()
    }

    pub(crate) fn execute(
        &self,
        id: EntityId,
        controls: &Controls,
    ) -> Result<Option<Notifications>> {
        let live = self.0.live.borrow();
        let Some(state) = live.get(&id.raw()) else {
            return Ok(None);
        };
        let mut state = state.borrow_mut();
        let previous = state.selected;
        let pointer = self.0.pointer.get();
        let click = controls.activated("ILClick");
        let touched = pointer != state.previous_pointer;
        let hit = if let (true, Some(pointer), Some(camera)) =
            (click || touched, pointer, &state.camera)
        {
            let size = match self.0.viewport.get() {
                Some(size) => size,
                None => {
                    let window = WindowSettings::load(&self.0.game)?;
                    [window.width, window.height]
                }
            };
            state.hit(camera, size, pointer)?
        } else {
            None
        };
        state.previous_pointer = pointer;
        if touched && let Some(index) = hit {
            state.selected = Some(index);
        }
        if controls.activated("IUp") {
            state.step(false);
        }
        if controls.activated("IDown") {
            state.step(true);
        }
        let mut commands = Vec::new();
        if click && let Some(index) = hit {
            commands.push(state.items[index].event.clone());
        }
        if controls.activated("IAction")
            && let Some(index) = state.selected
        {
            commands.push(state.items[index].event.clone());
        }
        let camera = state.camera.as_ref().and_then(OrbitCamera::native_pose);
        if camera.is_some() {
            self.0.active.set(Some(id));
        }
        Ok(Some(Notifications {
            camera,
            selection_changed: previous != state.selected,
            commands,
        }))
    }
}

impl State {
    fn locator(&self, name: &str) -> Option<&Label> {
        self.locators
            .iter()
            .find(|label| label.name.eq_ignore_ascii_case(name))
    }

    fn step(&mut self, next: bool) {
        let bound = self.selected.unwrap_or(self.items.len());
        let selected = if next {
            let first = self.selected.map_or(0, |index| index + 1);
            (first..self.items.len()).find(|&index| self.items[index].selectable)
        } else if bound < self.items.len() {
            (0..bound).rev().find(|&index| self.items[index].selectable)
        } else {
            None
        };
        if selected.is_some() {
            self.selected = selected;
        }
    }

    fn hit(
        &self,
        camera: &OrbitCamera,
        size: [u32; 2],
        pointer: [f64; 2],
    ) -> Result<Option<usize>> {
        for (index, item) in self.items.iter().enumerate() {
            if item.selectable
                && let Some(asset) = &item.active
                && camera.hits_menu_model(size, pointer, item.matrix, asset)?
            {
                return Ok(Some(index));
            }
        }
        Ok(None)
    }
}

struct Actor {
    shared: BackScenes,
    id: EntityId,
    state: Rc<RefCell<State>>,
}

impl Actor {
    fn models(&self) -> Result<&Models> {
        self.shared
            .0
            .models
            .as_ref()
            .context("InterfaceBackScene requires MODELR")
    }

    fn load_model(&self, name: &str, state: &mut State) -> Result<()> {
        let game = &self.shared.0.game;
        let file = format!("RESOURCE/MODELS/{name}_locators.gm");
        let bytes = game.read(&file, MAX_LOCATOR_BYTES)?;
        let locators = Geometry::read(&bytes)
            .with_context(|| format!("read background locators {file}"))?
            .labels;
        let language = self.shared.0.language.name()?;
        let prefix = texture_prefix(&language);
        let (_, background) = self.models()?.load_geometry(name, &prefix)?;
        state.background = Some(background);
        state.locators = locators;
        Ok(())
    }

    fn menu_items(
        &self,
        store: &AttributeStore,
        root: AttributeId,
        state: &State,
    ) -> Result<Vec<MenuItem>> {
        let children = store
            .get(root)
            .context("missing InterfaceBackScene menu attributes")?
            .children();
        ensure!(
            children.len() <= MAX_MENU_ITEMS,
            "InterfaceBackScene menu item budget"
        );
        children
            .iter()
            .map(|&item| {
                let value = |key| store.value(item, Some(key));
                let locator = value("locname")
                    .and_then(|name| state.locator(name))
                    .with_context(|| {
                        format!("InterfaceBackScene menu locator {:?}", value("locname"))
                    })?;
                native_model_matrix(locator.matrix)?;
                let prefix = texture_prefix(value("path").unwrap_or_default());
                let asset = |field| -> Result<Option<Rc<ModelAsset>>> {
                    value(field)
                        .filter(|name| !name.is_empty())
                        .map(|name| {
                            self.models()?
                                .load_geometry(name, &prefix)
                                .map(|(_, asset)| asset)
                        })
                        .transpose()
                };
                let active = asset("sel")?;
                let passive = if value("norm") == value("sel") {
                    active.clone()
                } else {
                    asset("norm")?
                };
                Ok(MenuItem {
                    matrix: locator.matrix,
                    selectable: active.is_some(),
                    active,
                    passive,
                    event: value("event").unwrap_or_default().to_owned(),
                })
            })
            .collect()
    }
}

fn texture_prefix(language: &str) -> String {
    if language.is_empty() {
        "MainMenu/".into()
    } else {
        format!("MainMenu/{language}/")
    }
}

fn lens(game: &GameDirectory) -> Result<(f32, [f32; 2])> {
    let text = configuration_text(game)?;
    let fields = root_fields(&text)?;
    let get = |key, default| {
        fields
            .get(key)
            .and_then(Option::as_deref)
            .map_or(Ok(default), |value| {
                value
                    .split_ascii_whitespace()
                    .next()
                    .unwrap_or_default()
                    .parse::<f32>()
                    .with_context(|| format!("invalid camera setting {key}"))
            })
    };
    Ok((
        1.287 * get("fov_multiplier", 1.0)?,
        [get("nearclipplane", 0.1)?, get("farclipplane", 4000.0)?],
    ))
}

impl Entity for Actor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        self.shared
            .0
            .live
            .borrow_mut()
            .insert(self.id.raw(), self.state.clone());
        Ok(true)
    }

    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        let mut state = self.state.borrow_mut();
        match message.long()? {
            0 => self.load_model(message.string()?, &mut state)?,
            1 => {
                let name = message.string()?;
                let label = state
                    .locator(name)
                    .with_context(|| format!("missing background camera locator {name}"))?;
                let (fov, clip) = lens(&self.shared.0.game)?;
                state.camera = Some(OrbitCamera::from_native_locator(label.matrix, fov, clip)?);
            }
            2 => {
                let name = message.string()?.to_owned();
                let root = message.attribute()?;
                let position = store.create_path(root, "Ship.Pos")?;
                let angles = store.create_path(root, "Ship.Ang")?;
                if let Some(label) = state.locator(&name) {
                    for (axis, value) in ["x", "y", "z"]
                        .into_iter()
                        .zip(label.matrix[12..15].iter().copied())
                    {
                        store.set_float(position, Some(axis), value)?;
                    }
                    store.set_float(angles, Some("y"), label.matrix[8].atan2(label.matrix[10]))?;
                }
            }
            3 => {
                let initial = message.long()?;
                let items = self.menu_items(store, message.attribute()?, &state)?;
                state.selected = usize::try_from(initial)
                    .ok()
                    .filter(|&i| items.get(i).is_some_and(|item| item.selectable));
                state.items = items;
            }
            4 => state.step(message.long()? & 1 != 0),
            5 => return Ok(state.selected.map_or(u64::MAX, |index| index as u64)),
            6 => {
                if let Ok(index) = usize::try_from(message.long()?)
                    && state.items.get(index).is_some_and(|item| item.selectable)
                {
                    state.selected = Some(index);
                }
            }
            7 => {
                let index = usize::try_from(message.long()?).ok();
                let enabled = message.long()? != 0;
                if let Some(item) = index.and_then(|i| state.items.get_mut(i))
                    && enabled != item.selectable
                {
                    item.selectable = enabled && item.active.is_some() && item.passive.is_some();
                }
            }
            code => bail!("InterfaceBackScene message {code} is not ported"),
        }
        Ok(0)
    }

    fn process_stage_checked(&mut self, stage: Stage, _: u32) -> Result<()> {
        if stage != Stage::Realize {
            return Ok(());
        }
        let state = self.state.borrow();
        let draw = |asset: &Rc<ModelAsset>, part, matrix| -> Result<()> {
            self.models()?.queue_draw(Draw {
                entity: self.id,
                revision: 0,
                asset: asset.clone(),
                matrix,
                ambient_light: None,
                rig: None,
                part,
            })
        };
        if let Some(asset) = &state.background {
            draw(asset, DrawPart::Body, IDENTITY)?;
        }
        for (index, item) in state.items.iter().enumerate() {
            let active = state.selected == Some(index) && item.active.is_some();
            let asset = if active { &item.active } else { &item.passive };
            if let Some(asset) = asset {
                draw(asset, DrawPart::MenuItem(index as u8, active), item.matrix)?;
            }
        }
        Ok(())
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared.0.live.borrow_mut().remove(&self.id.raw());
        if self.shared.0.active.get() == Some(self.id) {
            self.shared.0.active.set(None);
        }
        Ok(())
    }
}
