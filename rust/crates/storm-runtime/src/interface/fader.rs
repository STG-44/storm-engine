//! The Storm Fader's timed dimming, reveal, and script notifications.
use super::Shared;
use anyhow::Result;
use std::{mem, rc::Rc};
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, LayerType, Message, Stage};

const FADER_EXECUTE: u8 = 6;
const FADER_REALIZE: u8 = 7;
const LAYER_PRIORITY: u32 = (-256_i32) as u32;
const FADER_OUT: i32 = 33000;
const FADER_IN: i32 = 33001;

pub(super) struct FaderActor {
    shared: Rc<Shared>,
    id: EntityId,
    transition: Option<Transition>,
    delete_after: u8,
}

struct Transition {
    fade_in: bool,
    autodelete: bool,
    alpha: f32,
    speed: f32,
    first_draw: bool,
    finished: bool,
    post_start: bool,
    post_end: bool,
}

pub(crate) struct Notifications {
    pub fade_in: bool,
    pub erase: bool,
    start: bool,
    end: bool,
}

impl Notifications {
    pub fn events(&self) -> impl Iterator<Item = &'static str> {
        let [start, end] = if self.fade_in {
            ["FaderEvent_StartFadeIn", "FaderEvent_EndFadeIn"]
        } else {
            ["FaderEvent_StartFade", "FaderEvent_EndFade"]
        };
        self.start
            .then_some(start)
            .into_iter()
            .chain(self.end.then_some(end))
    }
}

impl FaderActor {
    pub fn new(shared: Rc<Shared>) -> Self {
        Self {
            shared,
            id: EntityId::INVALID,
            transition: None,
            delete_after: 0,
        }
    }

    fn execute(&mut self) {
        let Some(transition) = &mut self.transition else {
            return;
        };
        let mut erase = false;
        if self.delete_after != 0 {
            self.delete_after -= 1;
            erase = self.delete_after == 0;
        }
        let start = mem::take(&mut transition.post_start);
        let end = mem::take(&mut transition.post_end);
        if end {
            self.delete_after = if transition.autodelete { 2 } else { 0 };
        }
        if start || end || erase {
            self.shared.fader_notifications.borrow_mut().insert(
                self.id.raw(),
                Notifications {
                    fade_in: transition.fade_in,
                    erase,
                    start,
                    end,
                },
            );
        }
    }

    fn realize(&mut self, delta_ms: u32) {
        let Some(transition) = &mut self.transition else {
            return;
        };
        if mem::take(&mut transition.first_draw) {
            transition.post_start = true;
        }
        if transition.alpha == 1.0 && !transition.finished {
            transition.finished = true;
            transition.post_end = true;
        }
        let opacity = if transition.fade_in {
            1.0 - transition.alpha
        } else {
            transition.alpha
        };
        let existing = self.shared.fade_opacity.get();
        self.shared
            .fade_opacity
            .set(existing + (1.0 - existing) * opacity);
        if !transition.finished {
            if transition.speed <= 0.0 {
                transition.alpha = 1.0;
            } else if delta_ms != 0 {
                transition.alpha =
                    (transition.alpha + delta_ms as f32 * 0.001 * transition.speed).min(1.0);
            }
        }
    }
}

impl Entity for FaderActor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        for (index, kind) in [
            (FADER_EXECUTE, LayerType::Execute),
            (FADER_REALIZE, LayerType::Realize),
        ] {
            context.manager.set_layer_type(index, kind)?;
            context
                .manager
                .add_to_layer(index, self.id, LAYER_PRIORITY)?;
        }
        Ok(true)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared
            .fader_notifications
            .borrow_mut()
            .remove(&self.id.raw());
        Ok(())
    }

    fn process_stage(&mut self, stage: Stage, delta_ms: u32) {
        match stage {
            Stage::Execute => self.execute(),
            Stage::Realize => self.realize(delta_ms),
        }
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        let command = message.long()?;
        if matches!(command, FADER_IN | FADER_OUT) {
            let fade_in = command == FADER_IN;
            let duration = message.float()?;
            let autodelete = message.long()? != 0;
            let speed = if fade_in && duration < 0.00001 {
                100_000.0
            } else if duration > 0.0 {
                duration.recip()
            } else {
                0.0
            };
            self.transition = Some(Transition {
                fade_in,
                autodelete,
                alpha: 0.0,
                speed,
                first_draw: true,
                finished: false,
                post_start: false,
                post_end: false,
            });
        }
        Ok(0)
    }
}
