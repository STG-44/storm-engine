//! Battle interface timer, updated during the Realize stage.
use super::{Shared, load_texture};
use anyhow::Result;
use std::{rc::Rc, str::FromStr};
use storm_core::{Entity, EntityDestroy, EntityInit, Message, Stage};
use storm_formats::texture::Texture;

pub struct TimerImage {
    pub rect: [i32; 4],
    pub uv: [f32; 4],
    pub color: u32,
    pub texture: Option<Texture>,
}

pub struct BattleTimerState {
    pub background: TimerImage,
    pub foreground: TimerImage,
    pub remaining_seconds: f32,
    duration_seconds: f32,
    event: String,
    expired: bool,
}

impl BattleTimerState {
    /// The fraction of the foreground visible above the original timer cut.
    pub fn remaining_fraction(&self) -> f32 {
        if self.remaining_seconds > 0.0 && self.duration_seconds > 0.0 {
            self.remaining_seconds / self.duration_seconds
        } else {
            0.0
        }
    }

    pub(super) fn take_event(&mut self) -> Option<String> {
        if std::mem::take(&mut self.expired) && !self.event.is_empty() {
            Some(self.event.clone())
        } else {
            None
        }
    }
}

pub(super) struct TimerActor {
    shared: Rc<Shared>,
    id: u64,
}

impl TimerActor {
    pub fn new(shared: Rc<Shared>) -> Self {
        Self { shared, id: 0 }
    }
}

impl Entity for TimerActor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id.raw();
        let get = |name| {
            context
                .attributes
                .and_then(|root| context.store.value(root, Some(name)))
        };
        let color = |name| {
            context.attributes.map_or(u32::MAX, |root| {
                context.store.as_dword(root, Some(name), u32::MAX)
            })
        };
        let texture = |name| {
            get(name)
                .filter(|value| !value.is_empty())
                .map(|value| load_texture(&self.shared, value))
                .transpose()
                .map(Option::flatten)
        };
        let rect = read_rect(get("timerpos"), [0i32; 4]);
        let offset = read_rect(get("timeroffset"), [0i32; 4]);
        let foreground = [
            rect[0].wrapping_add(offset[0]),
            rect[1].wrapping_add(offset[1]),
            rect[2].wrapping_sub(offset[2]),
            rect[3].wrapping_sub(offset[3]),
        ];
        self.shared.timers.borrow_mut().insert(
            self.id,
            BattleTimerState {
                background: TimerImage {
                    rect,
                    uv: read_rect(get("timerbackuv"), [0.0, 0.0, 1.0, 1.0]),
                    color: color("timerbackcolor"),
                    texture: texture("timerbacktexture")?,
                },
                foreground: TimerImage {
                    rect: foreground,
                    uv: read_rect(get("timerforeuv"), [0.0, 0.0, 1.0, 1.0]),
                    color: color("timerforecolor"),
                    texture: texture("timerforetexture")?,
                },
                remaining_seconds: 0.0,
                duration_seconds: 0.0,
                event: String::new(),
                expired: false,
            },
        );
        Ok(true)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared.timers.borrow_mut().remove(&self.id);
        Ok(())
    }

    fn process_stage(&mut self, stage: Stage, delta_ms: u32) {
        if stage == Stage::Realize
            && let Some(state) = self.shared.timers.borrow_mut().get_mut(&self.id)
            && state.remaining_seconds > 0.0
            && state.duration_seconds > 0.0
        {
            state.remaining_seconds -= delta_ms as f32 * 0.001;
            if state.remaining_seconds <= 0.0 {
                state.remaining_seconds = 0.0;
                state.expired = true;
            }
        }
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let mut timers = self.shared.timers.borrow_mut();
        let state = timers.get_mut(&self.id).expect("initialized timer entity");
        match message.long()? {
            0 => {
                let duration = message.float()?;
                let event = message.string()?.to_owned();
                state.remaining_seconds = duration;
                state.duration_seconds = duration;
                state.event = event;
                state.expired = false;
            }
            1 => {
                state.remaining_seconds = 0.0;
                state.expired = false;
            }
            _ => {}
        }
        Ok(0)
    }
}

fn read_rect<T: FromStr>(value: Option<&str>, mut rect: [T; 4]) -> [T; 4] {
    if let Some(value) = value {
        for (field, component) in rect.iter_mut().zip(value.split(',')) {
            let Ok(component) = component.trim_ascii().parse() else {
                break;
            };
            *field = component;
        }
    }
    rect
}
