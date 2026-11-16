//! The retained CAviPlayer failure path and its video layer membership.
use super::Shared;
use anyhow::{Result, bail, ensure};
use std::rc::Rc;
use storm_core::{Entity, EntityDestroy, EntityId, EntityInit, LayerType, Message};

const VIDEO_EXECUTE: u8 = 10;
const VIDEO_REALIZE: u8 = 11;
const MSG_SET_VIDEO_PLAY: i32 = 49000;
const MSG_SET_VIDEO_FLAGS: i32 = 49002;
pub(super) const END_EVENT: &str = "ievntEndVideo";

pub(super) struct VideoActor {
    shared: Rc<Shared>,
    id: EntityId,
}

impl VideoActor {
    pub fn new(shared: Rc<Shared>) -> Self {
        Self {
            shared,
            id: EntityId::INVALID,
        }
    }
}

impl Entity for VideoActor {
    fn init(&mut self, context: &mut EntityInit<'_>) -> Result<bool> {
        self.id = context.id;
        let manager = &mut context.manager;
        manager.set_layer_type(VIDEO_REALIZE, LayerType::Realize)?;
        manager.add_to_layer(VIDEO_REALIZE, self.id, u32::MAX)?;
        manager.set_layer_type(VIDEO_EXECUTE, LayerType::Execute)?;
        manager.add_to_layer(VIDEO_EXECUTE, self.id, 1)?;
        Ok(true)
    }

    fn destroy(&mut self, _: &mut EntityDestroy<'_>) -> Result<()> {
        self.shared
            .finished_videos
            .borrow_mut()
            .remove(&self.id.raw());
        Ok(())
    }

    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        match message.long()? {
            MSG_SET_VIDEO_PLAY => {
                let name = message.string()?;
                ensure!(!name.is_empty(), "empty video name");
                let resource = format!("RESOURCE/VIDEOS/{name}");
                if self.shared.game.resolve_optional(&resource)?.is_some() {
                    bail!("video decoding is unavailable for {resource}");
                }
                // The retained non-Windows Execute path completes an unavailable
                // stream even when the simulation clock is paused. The script
                // owns deletion and the scheduling of the next logo.
                self.shared
                    .finished_videos
                    .borrow_mut()
                    .insert(self.id.raw(), true);
            }
            MSG_SET_VIDEO_FLAGS => {
                message.long()?;
            }
            _ => {}
        }
        Ok(0)
    }
}
