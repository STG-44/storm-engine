use super::{
    FULL_UV, NEUTRAL, Nodes, Setting, Shared, flag, ini_color, ini_integer, load_font, tuple,
};
use anyhow::{Result, ensure};
use std::{collections::BTreeMap, rc::Rc};
use storm_formats::texture::Texture;
use storm_render::interface_text::{
    Alignment, BitmapFont, InterfaceDraw, PictureColor, PictureDraw, TextDraw,
};

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) struct Action(pub(super) i32, pub(super) &'static str);

pub(super) const CLICK: Action = Action(12, "click");
pub(super) const ACTIVATE: Action = Action(5, "activate");
pub(super) const CANCEL: Action = Action(6, "deactivate");
pub(super) const RIGHT_CLICK: Action = Action(14, "rclick");

const ACTIONS: [Action; 15] = [
    Action(1, "rightstep"),
    Action(2, "leftstep"),
    Action(3, "upstep"),
    Action(4, "downstep"),
    ACTIVATE,
    CANCEL,
    Action(7, "select"),
    Action(8, "speedright"),
    Action(9, "speedleft"),
    Action(10, "speedup"),
    Action(11, "speeddown"),
    CLICK,
    Action(13, "dblclick"),
    RIGHT_CLICK,
    Action(15, "dblrclick"),
];

#[derive(Clone, Default)]
pub(super) struct Command {
    pub(super) event: Option<String>,
    pub(super) focus: Option<String>,
    pub(super) delay: u32,
    pub(super) redirects: Vec<(Action, String)>,
}

pub(super) struct Button {
    pub(super) texture: Option<Rc<Texture>>,
    pub(super) uv: [f32; 4],
    pub(super) commands: BTreeMap<i32, Command>,
    pub(super) select_on_hover: bool,
    pub(super) on_release: bool,
    label: Option<(Rc<BitmapFont>, String, [u8; 4], i32)>,
    face: [u8; 4],
    disabled: [u8; 4],
    shadow_color: [u8; 4],
    light: [u8; 4],
    dark: [u8; 4],
    shadow: [i32; 2],
    press_offset: [i32; 2],
    press_shadow: [i32; 2],
    press_frames: u32,
    remaining_frames: u32,
    flash_rate: f32,
    flash: f32,
    rising: bool,
}

fn action(name: &str) -> Option<Action> {
    ACTIONS
        .iter()
        .copied()
        .find(|kind| kind.1.eq_ignore_ascii_case(name))
}

impl Button {
    pub(super) fn load<'a>(
        nodes: &mut Nodes,
        shared: &Shared,
        setting: &Setting<'_>,
        commands: impl Iterator<Item = &'a str>,
    ) -> Result<Self> {
        ensure!(
            setting("videoTexture").is_none(),
            "unported XINTERFACE video button"
        );
        let group = setting("group");
        let (texture, uv, label) = if let Some(group) = group {
            let atlas = nodes.atlas(shared, group)?;
            let uv = atlas.part(setting("picture")).0;
            let text = shared.language.common_string(group)?;
            let font = setting("font")
                .map(|font| load_font(shared, font))
                .transpose()?
                .flatten();
            let label = if !text.is_empty() {
                Some((
                    font,
                    text,
                    ini_color(setting("fontColor"), [255; 4])?,
                    setting("strOffset").map_or(Ok(0), ini_integer)?,
                ))
            } else {
                None
            };
            let label = label.and_then(|(font, text, color, offset)| {
                font.map(|font| (font, text, color, offset))
            });
            (atlas.texture, uv, label)
        } else {
            (None, FULL_UV, None)
        };

        let mut bindings = BTreeMap::<i32, Command>::new();
        for line in commands {
            let mut fields = line.split(',').map(str::trim_ascii);
            let Some(kind) = fields.next().and_then(action) else {
                continue;
            };
            let binding = bindings.entry(kind.0).or_default();
            for field in fields {
                if let Some(value) = field.strip_prefix("event:") {
                    binding.event = Some(value.to_owned());
                } else if let Some(value) = field.strip_prefix("select:") {
                    binding.focus = Some(value.to_owned());
                } else if let Some(value) = field.strip_prefix("delay:") {
                    binding.delay = ini_integer(value)?.max(0) as u32;
                } else if let Some((kind, target)) = field
                    .strip_prefix("com:")
                    .and_then(|value| value.split_once(':'))
                    && let Some(kind) = action(kind)
                {
                    binding.redirects.push((kind, target.to_owned()));
                }
            }
        }
        let rate = setting("blindTimeSec").map_or(Ok(-1.0), str::parse::<f32>)?;
        let offsets = |key| setting(key).map_or(Ok([0; 2]), tuple);
        Ok(Self {
            texture,
            uv,
            commands: bindings,
            label,
            select_on_hover: flag(setting("moveMouseDoSelect"), false),
            on_release: flag(setting("UseActionByDeclick"), false),
            face: ini_color(setting("faceColor"), [255; 4])?,
            disabled: ini_color(setting("disableColor"), NEUTRAL)?,
            shadow_color: ini_color(setting("shadowColor"), [0, 0, 0, 255])?,
            light: ini_color(setting("lightColor"), [255; 4])?,
            dark: ini_color(setting("darkColor"), [255; 4])?,
            shadow: offsets("shadowOffset")?,
            press_offset: offsets("pressPictureOffset")?,
            press_shadow: offsets("pressShadowOffset")?,
            press_frames: setting("pressDelay").map_or(Ok(20), ini_integer)?.max(0) as u32,
            remaining_frames: 0,
            flash_rate: if rate > 0.0 { 0.001 / rate } else { 1.0 },
            flash: 1.0,
            rising: false,
        })
    }

    pub(super) fn press(&mut self, action: Action, selectable: bool) {
        if action == ACTIVATE || (action == CLICK && selectable) {
            self.remaining_frames = self.press_frames;
        }
    }

    pub(super) fn reset_press(&mut self) {
        self.remaining_frames = 0;
    }

    pub(super) fn draw(
        &mut self,
        rect: [i32; 4],
        canvas: [i32; 2],
        focused: bool,
        selectable: bool,
        delta_ms: u32,
        out: &mut Vec<InterfaceDraw>,
    ) {
        self.remaining_frames = self.remaining_frames.saturating_sub(1);
        let pressed = self.remaining_frames > 0;
        let face_offset = if pressed { self.press_offset } else { [0; 2] };
        let shadow_offset = if pressed {
            self.press_shadow
        } else {
            self.shadow
        };
        let face_color = if focused && self.flash_rate > 0.0 {
            let color = std::array::from_fn(|i| {
                let light = f32::from(self.light[i]);
                let dark = f32::from(self.dark[i]);
                (dark + (light - dark) * self.flash) as u8
            });
            let step = self.flash_rate * delta_ms as f32;
            self.flash += if self.rising { step } else { -step };
            if self.flash < 0.0 || self.flash > 1.0 {
                self.rising = self.flash < 0.0;
                self.flash = self.flash.clamp(0.0, 1.0);
            }
            color
        } else {
            self.face
        };
        let face = offset(rect, face_offset);
        if let Some(texture) = &self.texture {
            let quad = |rect: [i32; 4], rgba, color| {
                InterfaceDraw::Picture(PictureDraw {
                    texture: texture.clone(),
                    canvas,
                    rect: rect.map(|value| value as f32),
                    uv: self.uv,
                    rgba,
                    color,
                })
            };
            if self.shadow_color[3] != 0 {
                out.push(quad(
                    offset(face, shadow_offset),
                    self.shadow_color,
                    PictureColor::AlphaMask,
                ));
            }
            out.push(quad(
                face,
                if selectable {
                    face_color
                } else {
                    self.disabled
                },
                PictureColor::Modulate,
            ));
        }
        if let Some((font, text, rgba, baseline)) = &self.label {
            out.push(InterfaceDraw::Text(TextDraw {
                font: font.clone(),
                text: text.clone(),
                canvas,
                position: [
                    face[0].saturating_add(face[2]) / 2,
                    face[1].saturating_add(*baseline),
                ],
                scale: 1.0,
                alignment: Alignment::Center,
                shadow: false,
                rgba: *rgba,
            }));
        }
    }
}

fn offset([l, t, r, b]: [i32; 4], [x, y]: [i32; 2]) -> [i32; 4] {
    [
        l.saturating_add(x),
        t.saturating_add(y),
        r.saturating_add(x),
        b.saturating_add(y),
    ]
}
