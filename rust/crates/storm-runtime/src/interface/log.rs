//! Script-driven battle log and quick-action state.
use super::{Shared, load_texture};
use anyhow::{Result, ensure};
use std::rc::Rc;
use storm_core::{AttributeId, AttributeStore, Entity, EntityDestroy, EntityInit, Message, Stage};
use storm_formats::texture::Texture;

const LOG_ADD_STRING: i32 = 45020;
const LOG_SET_ACTIVE_ACTION: i32 = 45021;
const LOG_AND_ACTIONS_INIT: i32 = 45022;
const LOG_AND_ACTIONS_CHANGE: i32 = 45026;
const LI_SET_VISIBLE: i32 = 45027;
const LI_CLEAR_STRINGS: i32 = 45028;
const LI_OTHER_MSG: i32 = 45029;
const MAX_LINES: usize = 1024;

pub struct LogLine {
    pub text: String,
    pub alpha: f32,
    pub offset: f32,
}

pub struct LogLayout {
    pub size: [i32; 2],
    pub left: i32,
    pub right: i32,
    pub top: i32,
    pub font: Option<String>,
    pub font_scale: f32,
    pub color: u32,
    pub line_spacing: i32,
    shift_speed: f32,
    blend_speed: f32,
}

impl LogLayout {
    fn read(store: &AttributeStore, node: Option<AttributeId>) -> Self {
        let number = |name, default| dword(store, node, name, default);
        Self {
            size: [number("width", 200), number("height", 128)],
            left: number("left", if node.is_some() { -1 } else { 64 }),
            right: number("right", -1),
            top: number("up", 0),
            font: text(store, node, "font"),
            font_scale: float(store, node, "fontscale", 1.0),
            color: number("color", 0x00ff_ffff) as u32,
            line_spacing: number("offsetString", 24),
            shift_speed: float(store, node, "speed", 1.0 / 50.0),
            blend_speed: float(store, node, "color_speed", 1.0 / 50.0),
        }
    }
}

pub struct ActionHint {
    pub font: Option<String>,
    pub scale: f32,
    pub color: u32,
    pub shadow: bool,
    pub position: [i32; 2],
    pub text: String,
}

impl ActionHint {
    fn read(store: &AttributeStore, node: AttributeId) -> Self {
        let pos = store.child(node, "pos");
        Self {
            font: text(store, Some(node), "font"),
            scale: store.as_float(node, Some("scale"), 1.0),
            color: store.as_dword(node, Some("color"), u32::MAX),
            shadow: store.as_dword(node, Some("shadow"), 1) != 0,
            position: [dword(store, pos, "x", 0), dword(store, pos, "y", 0)],
            text: text(store, Some(node), "text").unwrap_or_default(),
        }
    }
}

pub struct ActionLayout {
    pub texture: Option<Texture>,
    pub atlas_size: [i32; 2],
    /// left, top, width, height
    pub rect: [i32; 4],
    /// left, top, right, bottom in atlas coordinates
    pub uv: Option<[f32; 4]>,
    pub hints: [Option<ActionHint>; 2],
}

impl ActionLayout {
    fn read(shared: &Shared, store: &AttributeStore, node: AttributeId) -> Result<Self> {
        let texture = text(store, Some(node), "textureName")
            .filter(|name| !name.is_empty())
            .map(|name| load_texture(shared, &name))
            .transpose()?
            .flatten();
        let number = |name, default| dword(store, Some(node), name, default);
        let atlas_size = [number("horzQ", 1), number("vertQ", 1)];
        ensure!(
            atlas_size.iter().all(|&dimension| dimension > 0),
            "invalid quick-action atlas dimensions"
        );
        Ok(Self {
            texture,
            atlas_size,
            rect: [
                number("left", 0),
                number("top", 0),
                number("width", 64),
                number("height", 64),
            ],
            uv: None,
            hints: Self::hints(store, node),
        })
    }

    fn hints(store: &AttributeStore, node: AttributeId) -> [Option<ActionHint>; 2] {
        ["text1", "text2"].map(|name| {
            store
                .child(node, name)
                .map(|hint| ActionHint::read(store, hint))
        })
    }

    fn select(&mut self, index: i32) {
        if index == -1 {
            self.uv = None;
            return;
        }
        let x = (index % self.atlas_size[0]) as f32 / self.atlas_size[0] as f32;
        let y = (index / self.atlas_size[0]) as f32 / self.atlas_size[1] as f32;
        self.uv = Some([
            x,
            y,
            x + 1.0 / self.atlas_size[0] as f32,
            y + 1.0 / self.atlas_size[1] as f32,
        ]);
    }
}

pub struct LogAndActionsState {
    pub viewport: [u32; 2],
    pub log: Option<LogLayout>,
    pub lines: Vec<LogLine>,
    pub action: Option<ActionLayout>,
    pub active_name: Option<String>,
    pub visible: bool,
    pub show_actions: bool,
    pub show_log: bool,
}

impl LogAndActionsState {
    fn add_line(&mut self, value: &str, immortal: bool) -> Result<()> {
        if immortal && let Some(index) = self.lines.iter().position(|line| line.alpha > 255.0) {
            if value.is_empty() {
                self.lines.remove(index);
            } else {
                self.lines[index].text = value.to_owned();
            }
            return Ok(());
        }
        if self
            .lines
            .last()
            .is_some_and(|line| line.text.eq_ignore_ascii_case(value))
        {
            return Ok(());
        }
        let spacing = self.log.as_ref().map_or(24, |log| log.line_spacing) as f32;
        let offset = self.lines.last().map_or(0.0, |line| line.offset + spacing);
        let had_lines = !self.lines.is_empty();
        ensure!(self.lines.len() < MAX_LINES, "battle log line limit");
        self.lines.push(LogLine {
            text: value.to_owned(),
            offset,
            alpha: if immortal { 10000.0 } else { 255.0 },
        });
        if let Some(layout) = &self.log
            && had_lines
            && offset + spacing > layout.size[1] as f32
        {
            let shift = offset.trunc() + spacing - layout.size[1] as f32;
            for line in &mut self.lines {
                line.offset -= shift;
            }
            self.lines.retain(|line| line.offset >= 0.0);
        }
        Ok(())
    }

    fn advance(&mut self, delta_ms: u32) {
        let Some(log) = &self.log else { return };
        let fade = delta_ms as f32 * log.blend_speed;
        let shift = delta_ms as f32 * log.shift_speed;
        let mut top = 0.0;
        self.lines.retain_mut(|line| {
            if line.alpha <= 255.0 {
                line.alpha -= fade;
                if line.alpha <= 0.0 {
                    return false;
                }
            }
            if line.offset > top {
                line.offset = (line.offset - shift).max(top);
            }
            top += log.line_spacing as f32;
            true
        });
    }
}

pub(super) struct LogActor {
    shared: Rc<Shared>,
    id: u64,
    root: Option<AttributeId>,
}

impl LogActor {
    pub fn new(shared: Rc<Shared>) -> Self {
        Self {
            shared,
            id: 0,
            root: None,
        }
    }
}

impl Entity for LogActor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id.raw();
        self.root = context.attributes;
        let viewport = self.shared.viewport()?;
        self.shared.logs.borrow_mut().insert(
            self.id,
            LogAndActionsState {
                viewport,
                log: None,
                lines: Vec::new(),
                action: None,
                active_name: None,
                visible: true,
                show_actions: false,
                show_log: false,
            },
        );
        Ok(true)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared.logs.borrow_mut().remove(&self.id);
        Ok(())
    }

    fn process_stage(&mut self, stage: Stage, delta_ms: u32) {
        if stage == Stage::Execute
            && let Some(state) = self.shared.logs.borrow_mut().get_mut(&self.id)
            && state.visible
        {
            state.advance(delta_ms);
        }
    }

    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        let code = message.long()?;
        let mut logs = self.shared.logs.borrow_mut();
        let state = logs.get_mut(&self.id).expect("initialized log entity");
        let child = |name| self.root.and_then(|root| store.child(root, name));
        match code {
            LOG_ADD_STRING => {
                let immortal = message.long()? != 0;
                state.add_line(message.string()?, immortal)?;
            }
            LOG_SET_ACTIVE_ACTION => {
                let name = message.string()?;
                if let Some(actions) = child("ActiveActions")
                    && let Some(definition) = store.child(actions, name)
                    && let Some(action) = &mut state.action
                    && name.len() <= 63
                {
                    let index = store.as_dword(definition, Some("IconNum"), 0) as i32;
                    state.active_name = Some(name.to_owned());
                    action.select(index);
                    if index != -1 {
                        action.hints = ActionLayout::hints(store, actions);
                    }
                }
            }
            LOG_AND_ACTIONS_INIT | LOG_AND_ACTIONS_CHANGE => {
                let show_actions = message.long()? != 0;
                let show_log = message.long()? != 0;
                let action = child("ActiveActions")
                    .map(|node| ActionLayout::read(&self.shared, store, node))
                    .transpose()?;
                state.show_actions = show_actions;
                state.show_log = show_log;
                state.action = action;
                if code == LOG_AND_ACTIONS_INIT {
                    state.log = Some(LogLayout::read(store, child("Log")));
                }
            }
            LI_SET_VISIBLE => state.visible = message.long()? != 0,
            LI_CLEAR_STRINGS => state.lines.clear(),
            LI_OTHER_MSG if message.string()?.eq_ignore_ascii_case("SetTimeScale") => {
                self.shared.time_scale.set(message.float()?);
            }
            _ => {}
        }
        Ok(0)
    }
}

fn dword(store: &AttributeStore, node: Option<AttributeId>, name: &str, default: i32) -> i32 {
    node.map_or(default, |node| {
        store.as_dword(node, Some(name), default as u32) as i32
    })
}

fn float(store: &AttributeStore, node: Option<AttributeId>, name: &str, default: f32) -> f32 {
    node.map_or(default, |node| store.as_float(node, Some(name), default))
}

fn text(store: &AttributeStore, node: Option<AttributeId>, name: &str) -> Option<String> {
    store.value(node?, Some(name)).map(str::to_owned)
}
