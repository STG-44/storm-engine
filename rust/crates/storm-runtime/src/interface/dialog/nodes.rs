mod button;
mod check;
mod collection;
mod edit_box;
mod formatted;
mod frame;
mod image;
mod text_button;

use super::{Shared, color, decode, load_font, load_texture, records};
use crate::{config::ini_integer, controls::Controls, interface::InterfaceScreen};
use anyhow::{Context, Result, bail, ensure};
use button::{ACTIVATE, Action, Button, CANCEL, CLICK, Command, RIGHT_CLICK, bindings};
use check::CheckButtons;
use collection::Collection;
use edit_box::EditBox;
use formatted::FormattedText;
use frame::Frame;
use std::{
    collections::BTreeMap,
    rc::{Rc, Weak},
};
use storm_core::{AttributeId, AttributeStore, Message, Value};
use storm_formats::{
    TxHeader,
    texture::{Mip, Texture},
};
use storm_render::interface_text::{Alignment, InterfaceDraw, PictureColor, PictureDraw, TextDraw};
use text_button::TextButton;

const DEFAULTS: &str = "RESOURCE/INI/INTERFACES/defaultnode.ini";
const PICTURES: &str = "RESOURCE/INI/INTERFACES/pictures.ini";
const FILE_BUDGET: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 256;
const FULL_UV: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const NEUTRAL: [u8; 4] = [128, 128, 128, 255];
type Setting<'a> = dyn Fn(&str) -> Option<&'a str> + 'a;

#[derive(Default)]
pub(in crate::interface) struct Nodes {
    items: Vec<Node>,
    textures: BTreeMap<String, Weak<Texture>>,
    picture_ini: Option<String>,
    focus: Option<usize>,
    mouse_target: Option<usize>,
    previous_pointer: Option<[f64; 2]>,
    waiting_for_release: bool,
    pending: Vec<Pending>,
    check_callback: Option<Message>,
}

struct Pending {
    node: usize,
    action: Action,
    command: Command,
    remaining: u32,
}

pub(in crate::interface) type NodeEvents = Vec<(String, Message)>;
pub(in crate::interface) const CHECK_EVENT: &str = "CheckButtonChange";

struct Node {
    name: String,
    priority: i32,
    visible: bool,
    locked: bool,
    selectable: bool,
    clickable: bool,
    commands: BTreeMap<i32, Command>,
    select_on_hover: bool,
    on_release: bool,
    rect: [i32; 4],
    widget: Widget,
}

impl Node {
    fn interactive(&self) -> bool {
        self.widget.interactive()
    }

    fn hit(&self, point: [f64; 2]) -> bool {
        match &self.widget {
            Widget::CheckButtons(checks) => checks.can_click(self.rect, point),
            _ => inside(self.rect, point),
        }
    }
}

enum Widget {
    Window {
        members: Vec<String>,
        shown: bool,
        active: bool,
    },
    Picture {
        texture: Option<Rc<Texture>>,
        uv: [f32; 4],
        rgba: [u8; 4],
    },
    Title(Vec<InterfaceDraw>),
    Button(Box<Button>),
    TextButton(Box<TextButton>),
    CheckButtons(Box<CheckButtons>),
    EditBox(Box<EditBox>),
    Collection(Box<Collection>),
    FormattedText(Box<FormattedText>),
    Frame(Box<Frame>),
}

impl Widget {
    fn interactive(&self) -> bool {
        matches!(
            self,
            Self::Button(_) | Self::TextButton(_) | Self::CheckButtons(_) | Self::EditBox(_)
        )
    }
}

struct Atlas {
    texture: Option<Rc<Texture>>,
    size: [i32; 2],
    parts: BTreeMap<String, [i32; 4]>,
}

impl Default for Atlas {
    fn default() -> Self {
        Self {
            texture: None,
            size: [1; 2],
            parts: BTreeMap::new(),
        }
    }
}

impl Atlas {
    fn part(&self, name: Option<&str>) -> ([f32; 4], i32) {
        self.named_part(name).unwrap_or(([0.0; 4], 0))
    }

    fn named_part(&self, name: Option<&str>) -> Option<([f32; 4], i32)> {
        let &[left, top, right, bottom] = self.parts.get(&name?.to_ascii_lowercase())?;
        let [width, height] = self.size.map(|n| n as f32);
        Some((
            [
                (left as f32 + 0.5) / width,
                (top as f32 + 0.5) / height,
                (right as f32 - 0.5) / width,
                (bottom as f32 - 0.5) / height,
            ],
            right.saturating_sub(left),
        ))
    }
}

impl Nodes {
    pub(in crate::interface) fn load(
        shared: &Shared,
        text: &str,
        screen: &InterfaceScreen,
        title: Option<&str>,
    ) -> Result<Self> {
        let mut nodes = Self::default();
        let language = shared.language.name()?;
        let mut defaults = None;
        for (_, line) in records(text, "MAIN").filter(|(key, _)| key.eq_ignore_ascii_case("item")) {
            let mut fields = line.split(',').map(str::trim_ascii).peekable();
            let priority = fields
                .peek()
                .and_then(|field| ini_integer(field).ok())
                .filter(|&n| n > 0);
            if priority.is_some() {
                fields.next();
            }
            let qualifier = fields.next().unwrap_or_default();
            let kind = match qualifier.to_ascii_lowercase().as_str() {
                "pc" => fields.next().unwrap_or_default(),
                "xbox" => continue,
                "lang" => {
                    if !fields
                        .next()
                        .is_some_and(|name| name.eq_ignore_ascii_case(&language))
                    {
                        continue;
                    }
                    fields.next().unwrap_or_default()
                }
                _ => qualifier,
            };
            if kind.is_empty() {
                continue;
            }
            ensure!(nodes.items.len() < MAX_NODES, "interface node budget");
            let name = fields.next().filter(|name| !name.is_empty()).unwrap_or("?");
            if defaults.is_none() {
                defaults = Some(read_ini(shared, DEFAULTS)?);
            }
            let defaults = defaults.as_deref().unwrap_or_default();
            let setting = |key: &str| field(text, name, key).or_else(|| field(defaults, kind, key));
            let class = field(defaults, kind, "class").unwrap_or(kind);
            let rect = screen_position(screen, &setting)?;
            let widget = match class.to_ascii_lowercase().as_str() {
                "window" => Widget::Window {
                    members: records(text, name)
                        .filter(|(key, _)| key.eq_ignore_ascii_case("nodelist"))
                        .flat_map(|(_, value)| value.split(',').map(str::trim_ascii))
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned)
                        .collect(),
                    shown: true,
                    active: true,
                },
                "picture" => nodes.load_picture(shared, &setting)?,
                "title" => Widget::Title(nodes.load_title(shared, &setting, screen, rect, title)?),
                "frame" => Widget::Frame(Box::new(Frame::load(&mut nodes, shared, &setting)?)),
                "formatedtext" => Widget::FormattedText(Box::new(FormattedText::load(
                    shared,
                    &setting,
                    rect,
                    records(text, name)
                        .filter(|(key, _)| key.eq_ignore_ascii_case("string"))
                        .map(|(_, value)| value),
                )?)),
                "imagecollection" => Widget::Collection(Box::new(Collection::load(
                    &mut nodes,
                    shared,
                    &setting,
                    screen,
                    records(text, name)
                        .filter(|(key, _)| key.eq_ignore_ascii_case("picture"))
                        .map(|(_, value)| value),
                )?)),
                "button" => Widget::Button(Box::new(Button::load(&mut nodes, shared, &setting)?)),
                "textbutton" => Widget::TextButton(Box::new(TextButton::load(
                    &mut nodes,
                    shared,
                    &setting,
                    field(text, name, "strOffset"),
                )?)),
                "checkbutton" => Widget::CheckButtons(Box::new(CheckButtons::load(
                    &mut nodes, shared, &setting, rect,
                )?)),
                "pceditbox" => {
                    Widget::EditBox(Box::new(EditBox::load(&mut nodes, shared, &setting)?))
                }
                _ => bail!("unported XINTERFACE dialog node {class:?} (item={line})"),
            };
            let window = matches!(&widget, Widget::Window { .. });
            let commands = if widget.interactive() {
                bindings(
                    records(text, name)
                        .filter(|(key, _)| key.eq_ignore_ascii_case("command"))
                        .map(|(_, value)| value),
                )?
            } else {
                BTreeMap::new()
            };
            let text_button = matches!(widget, Widget::TextButton(_));
            let selectable = text_button || !commands.is_empty();
            let clickable = selectable || matches!(widget, Widget::CheckButtons(_));
            let priority = priority.unwrap_or(80);
            let index = nodes
                .items
                .partition_point(|node| node.priority <= priority);
            nodes.items.insert(
                index,
                Node {
                    name: name.to_owned(),
                    priority,
                    visible: !flag(setting("bNotUse"), false),
                    locked: false,
                    selectable,
                    clickable,
                    commands,
                    select_on_hover: text_button || flag(setting("moveMouseDoSelect"), false),
                    on_release: flag(setting("UseActionByDeclick"), false),
                    rect,
                    widget,
                },
            );
            // Init can only affect members that have already been created.
            if window {
                nodes.window_state(index, flag(setting("active"), true), false);
                nodes.window_state(index, flag(setting("show"), true), true);
            }
        }
        if let Some(start) = field(text, "MAIN", "start") {
            nodes.focus = nodes
                .find(start)
                .or_else(|| nodes.items.iter().position(Node::interactive));
        }
        nodes.waiting_for_release = true;
        Ok(nodes)
    }

    fn find(&self, name: &str) -> Option<usize> {
        self.items
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case(name))
    }

    pub(in crate::interface) fn set_using(&mut self, name: &str, show: bool) {
        if let Some(index) = self.find(name) {
            let node = &mut self.items[index];
            node.visible = show;
            match &mut node.widget {
                Widget::Button(button) => button.reset_press(),
                Widget::TextButton(button) => button.reset_press(),
                Widget::EditBox(edit) => edit.lose_focus(),
                _ => {}
            }
        }
    }

    pub(in crate::interface) fn set_focus(&mut self, name: &str) {
        if let Some(index) = self.find(name) {
            self.change_focus(index);
        }
    }

    fn change_focus(&mut self, index: usize) {
        if self.focus == Some(index) {
            return;
        }
        if let Some(previous) = self.focus
            && let Widget::EditBox(edit) = &mut self.items[previous].widget
        {
            edit.lose_focus();
        }
        self.focus = Some(index);
    }

    pub(in crate::interface) fn focused_name(&self) -> &str {
        self.focus
            .map_or("", |index| self.items[index].name.as_str())
    }

    pub(in crate::interface) fn set_selectable(&mut self, name: &str, selectable: bool) {
        if let Some(index) = self.find(name) {
            self.items[index].selectable = selectable;
        }
    }

    pub(in crate::interface) fn selectable(&self, name: &str) -> bool {
        self.find(name)
            .is_some_and(|index| self.items[index].selectable && self.items[index].visible)
    }

    pub(in crate::interface) fn window(&mut self, name: &str, value: bool, visibility: bool) {
        if let Some(index) = self.find(name) {
            self.window_state(index, value, visibility);
        }
    }

    pub(in crate::interface) fn window_active(&self, name: &str) -> bool {
        self.find(name).is_some_and(|index| {
            matches!(
                self.items[index].widget,
                Widget::Window { active: true, .. }
            )
        })
    }

    fn window_state(&mut self, index: usize, value: bool, visibility: bool) {
        let Widget::Window { shown, active, .. } = &self.items[index].widget else {
            return;
        };
        if (if visibility { *shown } else { *active }) == value {
            return;
        }
        if visibility && !value && *active {
            self.window_state(index, false, false);
        }
        let Widget::Window {
            members,
            shown,
            active,
        } = &mut self.items[index].widget
        else {
            unreachable!()
        };
        if visibility {
            *shown = value;
        } else {
            *active = value;
        }
        for name in members.clone() {
            if let Some(child) = self.find(&name) {
                if visibility {
                    self.items[child].visible = value;
                } else {
                    self.items[child].locked = !value;
                }
                self.window_state(child, value, visibility);
            }
        }
    }

    fn input_node(&self, index: usize) -> Option<&Node> {
        let node = self.items.get(index)?;
        (node.interactive() && node.visible && !node.locked && node.selectable).then_some(node)
    }

    pub(in crate::interface) fn take_button_events(&mut self, delta_ms: u32) -> Result<NodeEvents> {
        let due = self
            .pending
            .extract_if(.., |pending| {
                pending.remaining = pending.remaining.saturating_sub(delta_ms);
                pending.remaining == 0
            })
            .collect::<Vec<_>>();
        let mut events = Vec::new();
        for Pending {
            node,
            action,
            command,
            ..
        } in due.into_iter().rev()
        {
            for (action, target) in command.redirects.into_iter().rev() {
                if let Some(index) = self.find(&target) {
                    events.extend(self.execute_action(index, action)?);
                }
            }
            if let Some(name) = command.focus {
                self.set_focus(&name);
            }
            if let Some(event) = command.event {
                events.push((
                    event,
                    Message::new(
                        "ls",
                        vec![
                            Value::Long(action.0),
                            Value::String(self.items[node].name.clone()),
                        ],
                    )?,
                ));
            }
        }
        Ok(events)
    }

    pub(in crate::interface) fn button_input(
        &mut self,
        controls: &Controls,
        point: Option<[f64; 2]>,
    ) -> Result<NodeEvents> {
        // Leave the separate 3D main-menu controls in InterfaceBackScene.
        if !self.items.iter().any(Node::interactive) {
            return Ok(Vec::new());
        }
        const PRESS: [&str; 17] = [
            "IRight",
            "ILeft",
            "IUp",
            "IDown",
            "ILShift",
            "IRShift",
            "IAction",
            "ICancel",
            "IAllCancel",
            "ILClick",
            "IRClick",
            "IStartButton",
            "IUp2",
            "IDown2",
            "IRight2",
            "ILeft2",
            "IContextHelp",
        ];
        if self.waiting_for_release && PRESS.iter().any(|name| !controls.idle(name)) {
            return Ok(Vec::new());
        }
        self.waiting_for_release = false;
        let hit = |matches_node: fn(&Node, [f64; 2]) -> bool| {
            point.and_then(|point| {
                (0..self.items.len()).rev().find(|&index| {
                    self.input_node(index)
                        .is_some_and(|node| matches_node(node, point))
                })
            })
        };
        let hovered = hit(|node, point| node.select_on_hover && inside(node.rect, point));
        let clicked = hit(|node, point| {
            node.clickable && node.commands.contains_key(&CLICK.0) && node.hit(point)
        });
        if point != self.previous_pointer
            && let Some(index) = hovered
        {
            self.change_focus(index);
        }
        self.previous_pointer = point;
        if controls.activated("ILClick") {
            self.mouse_target = clicked;
            if let Some(index) = clicked {
                let node = &mut self.items[index];
                if !node.on_release {
                    return self.fire(index, CLICK);
                }
                if let Widget::TextButton(button) = &mut node.widget {
                    button.begin_press();
                }
            }
            return Ok(Vec::new());
        }
        if controls.released("ILClick")
            && let Some(index) = self.mouse_target.take()
        {
            let node = &mut self.items[index];
            if node.on_release {
                if let Widget::TextButton(button) = &mut node.widget {
                    button.reset_press();
                }
                if node.clickable {
                    return self.fire(index, CLICK);
                }
            }
        }
        if !controls.held("ILClick") && controls.activated("IRClick") {
            return clicked.map_or_else(|| Ok(Vec::new()), |index| self.fire(index, RIGHT_CLICK));
        }
        if controls.held("ILClick") || controls.held("IRClick") {
            return Ok(Vec::new());
        }
        let active = |first, second| controls.activated(first) || controls.activated(second);
        let shifted = controls.held("ILShift") || controls.held("IRShift");
        let action = if active("ICancel", "IAllCancel") {
            CANCEL
        } else if active("IAction", "IStartButton") {
            ACTIVATE
        } else if active("IDown", "IDown2") {
            if shifted {
                Action(11, "speeddown")
            } else {
                Action(4, "downstep")
            }
        } else if active("IUp", "IUp2") {
            if shifted {
                Action(10, "speedup")
            } else {
                Action(3, "upstep")
            }
        } else if active("ILeft", "ILeft2") {
            if shifted {
                Action(9, "speedleft")
            } else {
                Action(2, "leftstep")
            }
        } else if active("IRight", "IRight2") {
            if shifted {
                Action(8, "speedright")
            } else {
                Action(1, "rightstep")
            }
        } else {
            return Ok(Vec::new());
        };
        if let Some(index) = self.focus {
            self.fire(index, action)
        } else if action == CANCEL {
            Ok(vec![("exitCancel".to_owned(), Message::new("", vec![])?)])
        } else {
            Ok(Vec::new())
        }
    }

    fn execute_action(
        &mut self,
        index: usize,
        action: Action,
    ) -> Result<Option<(String, Message)>> {
        let node = &mut self.items[index];
        if !node.visible {
            return Ok(None);
        }
        match &mut node.widget {
            Widget::Button(button) => button.press(action, node.selectable),
            Widget::TextButton(button) => button.execute(action, node.on_release),
            Widget::CheckButtons(checks) if action == CLICK => {
                if let Some(change) = self
                    .previous_pointer
                    .and_then(|point| checks.click(node.rect, point))
                {
                    return Ok(Some((CHECK_EVENT.to_owned(), change.message(&node.name)?)));
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn fire(&mut self, index: usize, action: Action) -> Result<NodeEvents> {
        let Some(command) = self
            .input_node(index)
            .and_then(|node| node.commands.get(&action.0))
            .cloned()
        else {
            return Ok(Vec::new());
        };
        let change = self.execute_action(index, action)?;
        self.pending.retain(|pending| pending.node != index);
        if command.focus.is_some() || command.event.is_some() || !command.redirects.is_empty() {
            self.pending.push(Pending {
                node: index,
                action,
                remaining: command.delay,
                command,
            });
        }
        let mut events = vec![(
            "ISound".to_owned(),
            Message::new("l", vec![Value::Long(1)])?,
        )];
        events.extend(change);
        events.push((
            "ievnt_command".to_owned(),
            Message::new(
                "ss",
                vec![
                    Value::String(action.1.to_owned()),
                    Value::String(self.items[index].name.clone()),
                ],
            )?,
        ));
        Ok(events)
    }

    fn texture(&mut self, shared: &Shared, name: &str) -> Result<Option<Rc<Texture>>> {
        let key = name.replace('\\', "/").to_ascii_lowercase();
        if let Some(texture) = self.textures.get(&key).and_then(Weak::upgrade) {
            return Ok(Some(texture));
        }
        let texture = load_texture(shared, name)?.map(Rc::new);
        if let Some(texture) = &texture {
            self.textures.retain(|_, entry| entry.strong_count() > 0);
            self.textures.insert(key, Rc::downgrade(texture));
        }
        Ok(texture)
    }

    fn atlas(&mut self, shared: &Shared, group: &str) -> Result<Atlas> {
        if self.picture_ini.is_none() {
            self.picture_ini = Some(read_ini(shared, PICTURES)?);
        }
        let ini = self.picture_ini.as_deref().unwrap_or_default();
        let mut atlas = Atlas::default();
        let Some(name) = field(ini, group, "sTextureName") else {
            return Ok(atlas);
        };
        let path = format!("INTERFACES/{name}");
        for (key, dimension) in ["wTextureWidth", "wTextureHeight"]
            .into_iter()
            .zip(&mut atlas.size)
        {
            *dimension = field(ini, group, key).map_or(Ok(1), ini_integer)?.max(1);
        }
        for (_, item) in records(ini, group).filter(|(key, _)| key.eq_ignore_ascii_case("picture"))
        {
            if let Some((name, rect)) = item.split_once(',') {
                atlas
                    .parts
                    .entry(name.trim_ascii().to_ascii_lowercase())
                    .or_insert(tuple(rect)?);
            }
        }
        atlas.texture = self.texture(shared, &path)?;
        Ok(atlas)
    }

    fn load_picture(&mut self, shared: &Shared, setting: &Setting<'_>) -> Result<Widget> {
        ensure!(
            setting("videoName").is_none(),
            "unported XINTERFACE video picture"
        );
        ensure!(
            !flag(setting("blind"), false),
            "unported XINTERFACE blinking picture"
        );
        let (texture, uv) = if let Some(group) = setting("groupName") {
            let atlas = self.atlas(shared, group)?;
            let uv = setting("picName").map_or(FULL_UV, |name| atlas.part(Some(name)).0);
            (atlas.texture, uv)
        } else {
            let texture = setting("textureName")
                .map(|name| self.texture(shared, name))
                .transpose()?
                .flatten();
            (texture, setting("textureRect").map_or(Ok(FULL_UV), tuple)?)
        };
        Ok(Widget::Picture {
            texture,
            uv,
            rgba: ini_color(setting("color"), NEUTRAL)?,
        })
    }

    fn load_title(
        &mut self,
        shared: &Shared,
        setting: &Setting<'_>,
        screen: &InterfaceScreen,
        [left, top, right, bottom]: [i32; 4],
        title: Option<&str>,
    ) -> Result<Vec<InterfaceDraw>> {
        let font = setting("font")
            .map(|name| load_font(shared, name))
            .transpose()?
            .flatten();
        let scale = setting("fontScale").map_or(Ok(1.0), str::parse::<f32>)?;
        ensure!(
            (0.0..=16.0).contains(&scale),
            "invalid interface title font scale"
        );
        let caption = match title {
            Some(literal) if literal.starts_with('#') => literal[1..].to_owned(),
            Some(key) => shared.language.common_string(key)?,
            None => String::new(),
        };
        let width = setting("stringWidth").map_or(Ok(0), ini_integer)?;
        let half = if width == 0 {
            font.as_ref().map_or(0, |font| {
                ((font.text_width(&caption) as f32 * scale) as i32) / 2
            })
        } else {
            width / 2
        };
        let center = left.saturating_add(right) / 2;
        let mut draws = Vec::new();
        if let Some(group) = setting("imgGroupName") {
            let atlas = self.atlas(shared, group)?;
            if let Some(texture) = &atlas.texture {
                let (center_uv, _) = atlas.part(setting("titleCenter"));
                let (medium_uv, medium_width) = atlas.part(setting("titleMedium"));
                let (tile_uv, tile_width) = atlas.part(setting("titleTiled"));
                let rgba = ini_color(setting("imageColor"), [255; 4])?;
                let mut push = |l, r, uv| {
                    draws.push(InterfaceDraw::Picture(PictureDraw {
                        texture: texture.clone(),
                        canvas: screen.size,
                        rect: [l, top as f32, r, bottom as f32],
                        uv,
                        rgba,
                        color: PictureColor::Modulate,
                    }))
                };
                let transition = half.saturating_add(medium_width);
                let x = center as f32;
                let [inner, outer] = [half, transition].map(|value| value as f32);
                push(x - inner, x + inner, center_uv);
                push(x - outer, x - inner, medium_uv);
                push(x + outer, x + inner, medium_uv);
                if tile_width > 0 {
                    let remaining = center.saturating_sub(left).saturating_sub(transition);
                    let count = (remaining / tile_width).saturating_add(1).max(0);
                    ensure!(count <= 512, "interface title tile budget");
                    if count > 0 {
                        let left_step = (x - outer - left as f32) / count as f32;
                        let right_step = (right as f32 - x - outer) / count as f32;
                        for i in 0..count {
                            let start = x - outer - i as f32 * left_step;
                            push(start - left_step, start, tile_uv);
                        }
                        for i in 0..count {
                            let start = x + outer + i as f32 * right_step;
                            push(start + right_step, start, tile_uv);
                        }
                    }
                }
            }
        }
        if let Some(font) = font
            && !title.is_some_and(|title| title.starts_with('#'))
            && !caption.is_empty()
        {
            let offset = setting("stringOffset").map_or(Ok(0), ini_integer)?;
            draws.push(InterfaceDraw::Text(TextDraw {
                font,
                text: caption,
                canvas: screen.size,
                position: [center, top.saturating_add(offset)],
                vertical_anchor: None,
                scale,
                alignment: Alignment::Center,
                shadow: true,
                rgba: ini_color(setting("fontColor"), [255; 4])?,
            }));
        }
        Ok(draws)
    }

    pub(in crate::interface) fn set_formatted_text(
        &mut self,
        name: &str,
        input: &str,
    ) -> Result<()> {
        if let Some(index) = self.find(name) {
            let node = &mut self.items[index];
            if let Widget::FormattedText(text) = &mut node.widget {
                text.replace(input, node.rect)?;
            }
        }
        Ok(())
    }

    pub(in crate::interface) fn publish_checks(
        &mut self,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<()> {
        if let Some(root) = root {
            for node in &mut self.items {
                if let Widget::CheckButtons(checks) = &mut node.widget {
                    checks.publish(store, root, &node.name)?;
                }
            }
        }
        Ok(())
    }

    pub(in crate::interface) fn initialize_edits(
        &self,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<()> {
        if let Some(root) = root {
            for node in &self.items {
                if matches!(node.widget, Widget::EditBox(_)) {
                    EditBox::initialize(store, root, &node.name)?;
                }
            }
        }
        Ok(())
    }

    pub(in crate::interface) fn editor_focused(&self) -> bool {
        self.focus
            .and_then(|index| self.input_node(index))
            .is_some_and(|node| matches!(node.widget, Widget::EditBox(_)))
    }

    pub(in crate::interface) fn edit_input(
        &mut self,
        controls: &Controls,
        store: &mut AttributeStore,
        root: Option<AttributeId>,
    ) -> Result<()> {
        if let Some(index) = self.focus {
            let node = &mut self.items[index];
            if let Widget::EditBox(edit) = &mut node.widget {
                if node.visible
                    && !node.locked
                    && node.selectable
                    && let Some(root) = root
                {
                    edit.apply(store, root, &node.name, controls.edit_keys())?;
                } else {
                    edit.lose_focus();
                }
            }
        }
        Ok(())
    }

    pub(in crate::interface) fn edit_exit(
        &self,
        controls: &Controls,
        events: &mut NodeEvents,
    ) -> Result<()> {
        if controls.released("IStartButton") {
            for node in &self.items {
                if node.visible && matches!(node.widget, Widget::EditBox(_)) {
                    events.push((
                        "editexit".to_owned(),
                        Message::new("s", vec![Value::String(node.name.clone())])?,
                    ));
                }
            }
        }
        Ok(())
    }

    pub(in crate::interface) fn has_check_callback(&self) -> bool {
        self.check_callback.is_some()
    }

    pub(in crate::interface) fn take_check_callback(&mut self) -> Option<Message> {
        self.check_callback.take()
    }

    pub(in crate::interface) fn send(
        &mut self,
        shared: &Shared,
        screen: &InterfaceScreen,
        message: &mut Message,
    ) -> Result<u64> {
        let Some(index) = self.find(message.string()?) else {
            return Ok(0);
        };
        let code = message.long()?;
        let node = &mut self.items[index];
        if code == -1 {
            let code = message.long()?;
            return match code {
                1 => {
                    node.clickable = message.long()? != 0;
                    Ok(0)
                }
                2 => Ok(u64::from(node.clickable)),
                4 => {
                    ensure!(
                        matches!(
                            node.widget,
                            Widget::CheckButtons(_) | Widget::TextButton(_) | Widget::EditBox(_)
                        ),
                        "unported XINTERFACE base node position for {:?}",
                        node.name
                    );
                    let rect = [
                        message.long()?,
                        message.long()?,
                        message.long()?,
                        message.long()?,
                    ];
                    if let Widget::CheckButtons(checks) = &mut node.widget {
                        checks.move_to(rect)?;
                    }
                    node.rect = rect;
                    Ok(0)
                }
                _ => bail!(
                    "unported XINTERFACE base node message {code} for {:?}",
                    node.name
                ),
            };
        }
        if matches!(node.widget, Widget::EditBox(_)) {
            return Ok(0);
        }
        if let Widget::TextButton(button) = &mut node.widget {
            match code {
                0 => button.set_text(shared, message.string()?)?,
                1 => {
                    node.rect = [
                        message.long()?,
                        message.long()?,
                        message.long()?,
                        message.long()?,
                    ];
                }
                _ => {}
            }
            return Ok(0);
        }
        if let Widget::CheckButtons(checks) = &mut node.widget {
            let (result, change) = checks.send(shared, node.rect, code, message)?;
            if let Some(change) = change {
                self.check_callback = Some(change.message(&node.name)?);
            }
            return Ok(result);
        }
        if let Widget::FormattedText(text) = &mut node.widget {
            return text.send(code, message, node.rect);
        }
        if let Widget::Frame(frame) = &mut node.widget {
            if code == 0 {
                frame.set_border_color(color(message.long()?));
            }
            return Ok(0);
        }
        if matches!(node.widget, Widget::Button(_)) {
            self.send_button(index, shared, screen, code, message)?;
            return Ok(0);
        }
        if let Widget::Collection(collection) = &mut self.items[index].widget {
            if code == 1 {
                let group = message.string()?;
                if !group.eq_ignore_ascii_case(collection.group()) {
                    let atlas = self.atlas(shared, group)?;
                    let Widget::Collection(collection) = &mut self.items[index].widget else {
                        unreachable!()
                    };
                    collection.set_atlas(group.to_owned(), atlas);
                }
            } else {
                collection.send(code, message)?;
            }
            return Ok(0);
        }
        ensure!(
            matches!(self.items[index].widget, Widget::Picture { .. }),
            "unported XINTERFACE message {code} for node {:?}",
            self.items[index].name
        );
        let replacement = match code {
            2 => {
                ensure!(message.long()? == 0, "unported XINTERFACE video picture");
                Some((self.texture(shared, message.string()?)?, FULL_UV))
            }
            6 => {
                let atlas = self.atlas(shared, message.string()?)?;
                let uv = atlas.part(Some(message.string()?)).0;
                Some((atlas.texture, uv))
            }
            _ => None,
        };
        let node = &mut self.items[index];
        let Widget::Picture { texture, uv, rgba } = &mut node.widget else {
            unreachable!()
        };
        if let Some((image, coordinates)) = replacement {
            *texture = image;
            *uv = coordinates;
            return Ok(0);
        }
        match code {
            0 => {
                node.rect = [
                    message.long()?,
                    message.long()?,
                    message.long()?,
                    message.long()?,
                ]
            }
            1 => {
                let [left, right, top, bottom] = [
                    message.float()?,
                    message.float()?,
                    message.float()?,
                    message.float()?,
                ];
                ensure!(
                    [left, top, right, bottom].into_iter().all(f32::is_finite),
                    "invalid picture UV"
                );
                *uv = [left, top, right, bottom];
            }
            4 => *rgba = color(message.long()?),
            _ => bail!("unported XINTERFACE picture message {code}"),
        }
        Ok(0)
    }

    fn send_button(
        &mut self,
        index: usize,
        shared: &Shared,
        screen: &InterfaceScreen,
        code: i32,
        message: &mut Message,
    ) -> Result<()> {
        let image = if code == 2 {
            let group = message.string()?;
            if group.is_empty() {
                return Ok(());
            }
            let atlas = self.atlas(shared, group)?;
            let uv = atlas.part(Some(message.string()?)).0;
            Some((atlas.texture, uv))
        } else {
            None
        };
        let node = &mut self.items[index];
        let Widget::Button(button) = &mut node.widget else {
            unreachable!()
        };
        match code {
            0 => {
                let rect = [
                    message.long()?,
                    message.long()?,
                    message.long()?,
                    message.long()?,
                ];
                node.rect = absolute_rect(rect, screen, message.long()?);
            }
            1 => {
                let uv = [
                    message.float()?,
                    message.float()?,
                    message.float()?,
                    message.float()?,
                ];
                ensure!(
                    uv.iter().all(|value| value.is_finite()),
                    "invalid button UV"
                );
                button.uv = uv;
            }
            2 => (button.texture, button.uv) = image.expect("button image message parsed"),
            _ => bail!("unported XINTERFACE button message {code}"),
        }
        Ok(())
    }

    pub(in crate::interface) fn draws(
        &mut self,
        screen: &InterfaceScreen,
        foreground: bool,
        delta_ms: u32,
        store: &AttributeStore,
        root: Option<AttributeId>,
        out: &mut Vec<InterfaceDraw>,
    ) {
        for (index, node) in self.items.iter_mut().enumerate() {
            if !node.visible || (node.priority > 90) != foreground {
                continue;
            }
            match &mut node.widget {
                Widget::Button(button) => button.draw(
                    node.rect,
                    screen.size,
                    self.focus == Some(index),
                    node.selectable,
                    delta_ms,
                    out,
                ),
                Widget::TextButton(button) => button.draw(
                    node.rect,
                    screen.size,
                    self.focus == Some(index),
                    node.selectable,
                    node.on_release,
                    out,
                ),
                Widget::Picture {
                    texture: Some(texture),
                    uv,
                    rgba,
                } => {
                    out.push(InterfaceDraw::Picture(PictureDraw {
                        texture: texture.clone(),
                        canvas: screen.size,
                        rect: node.rect.map(|n| n as f32),
                        uv: *uv,
                        rgba: *rgba,
                        color: PictureColor::Modulate,
                    }));
                }
                Widget::Title(draws) => out.extend(draws.iter().cloned()),
                Widget::Collection(collection) => collection.draw(screen.size, out),
                Widget::FormattedText(text) => text.draw(node.rect, screen.size, out),
                Widget::Frame(frame) => frame.draw(node.rect, screen.size, out),
                Widget::CheckButtons(checks) => {
                    checks.draw(node.rect, screen.size, node.selectable, out)
                }
                Widget::EditBox(edit) => {
                    let value = root
                        .and_then(|root| store.child(root, &node.name))
                        .and_then(|field| store.value(field, Some("str")))
                        .unwrap_or_default();
                    edit.draw(
                        node.rect,
                        screen.size,
                        value,
                        self.focus == Some(index) && node.selectable && !node.locked,
                        out,
                    );
                }
                _ => {}
            }
        }
    }
}

fn solid_mask() -> Rc<Texture> {
    Rc::new(Texture {
        header: TxHeader {
            flags: 0,
            width: 1,
            height: 1,
            mips: 1,
            format: 21,
            mip0_bytes: 4,
        },
        mips: vec![Mip {
            width: 1,
            height: 1,
            rgba: vec![255; 4],
        }],
    })
}

fn read_ini(shared: &Shared, path: &str) -> Result<String> {
    shared
        .game
        .read_optional(path, FILE_BUDGET)?
        .map(|bytes| decode(&bytes))
        .transpose()
        .map(Option::unwrap_or_default)
}

fn field<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    records(text, section)
        .find(|(name, _)| name.eq_ignore_ascii_case(key))
        .map(|(_, value)| value)
}

fn flag(value: Option<&str>, default: bool) -> bool {
    value.map_or(default, |value| {
        value.eq_ignore_ascii_case("true") || ini_integer(value).is_ok_and(|value| value != 0)
    })
}

fn ini_color(value: Option<&str>, default: [u8; 4]) -> Result<[u8; 4]> {
    let Some(value) = value else {
        return Ok(default);
    };
    let [a, r, g, b] = tuple(value)?;
    Ok([r, g, b, a])
}

fn tuple<T: std::str::FromStr, const N: usize>(value: &str) -> Result<[T; N]> {
    value
        .split_once(';')
        .map_or(value, |(value, _)| value)
        .split(',')
        .map(|field| field.trim_ascii().parse().ok())
        .collect::<Option<Vec<_>>>()
        .and_then(|items| items.try_into().ok())
        .with_context(|| format!("invalid interface tuple {value:?}"))
}

fn screen_position(screen: &InterfaceScreen, setting: &Setting<'_>) -> Result<[i32; 4]> {
    let rect = setting("position").map_or(Ok(screen.rect), tuple)?;
    let mask = setting("bAbsoluteRectangle").map_or(Ok(0), ini_integer)?;
    Ok(absolute_rect(rect, screen, mask))
}

fn inside([left, top, right, bottom]: [i32; 4], [x, y]: [f64; 2]) -> bool {
    (f64::from(left)..=f64::from(right)).contains(&x)
        && (f64::from(top)..=f64::from(bottom)).contains(&y)
}

fn absolute_rect(mut rect: [i32; 4], screen: &InterfaceScreen, mask: i32) -> [i32; 4] {
    let [left, top, right, bottom] = screen.rect;
    let [width, height] = screen.size;
    let offsets = [
        if mask & 1 == 0 { left } else { 0 },
        if mask & 2 == 0 { top } else { 0 },
        if mask & 4 != 0 {
            width.saturating_sub(right).saturating_add(left)
        } else {
            left
        },
        if mask & 8 != 0 {
            height.saturating_sub(bottom).saturating_add(top)
        } else {
            top
        },
    ];
    for (value, offset) in rect.iter_mut().zip(offsets) {
        *value = value.saturating_add(offset);
    }
    rect
}
