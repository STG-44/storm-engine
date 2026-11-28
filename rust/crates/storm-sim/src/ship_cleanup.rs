//! Retained SHIP destructor service requests. Group geometry is owned by ports.
use anyhow::Result;
use std::{cell::RefCell, rc::Rc};
use storm_core::{Entity, EntityId, Message};

pub const MSG_SAIL_DEL_GROUP: i32 = 40030;
pub const MSG_ROPE_DEL_GROUP: i32 = 40102;
pub const MSG_FLAG_DEL_GROUP: i32 = 40201;
pub const MSG_VANT_DEL_GROUP: i32 = 40301;

#[derive(Clone, Copy, Debug)]
pub struct Links {
    pub model: EntityId,
    pub sail: EntityId,
    pub rope: EntityId,
    pub flag: EntityId,
    pub vants: [EntityId; 3],
    pub blots: EntityId,
}

#[derive(Clone, Debug, Default)]
pub struct Groups {
    pub owners: Vec<(EntityId, bool)>,
    pub pending: bool,
}
impl Groups {
    /// The retained delete-group branches mark the first matching owner; buffer
    /// compaction belongs to the later service update, not this notification.
    pub fn mark_deleted(&mut self, owner: EntityId) {
        if let Some((_, deleted)) = self.owners.iter_mut().find(|(id, _)| *id == owner) {
            *deleted = true;
            self.pending = true;
        }
    }
}
pub struct GroupService {
    pub code: i32,
    pub groups: Rc<RefCell<Groups>>,
}
impl Entity for GroupService {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if message.long()? == self.code {
            self.groups.borrow_mut().mark_deleted(message.entity()?);
        }
        Ok(0)
    }
}
pub trait CleanupPort {
    fn erase(&mut self, id: EntityId) -> Result<()>;
    fn send(&mut self, service: EntityId, code: i32, owner: EntityId) -> Result<()>;
    /// GetEntityId("ShipTracks"), followed by the native pointer check.
    fn tracks(&mut self) -> Option<EntityId>;
    fn del_ship(&mut self, tracks: EntityId, ship: EntityId) -> Result<()>;
}
pub fn release(ship: EntityId, links: Links, port: &mut impl CleanupPort) -> Result<()> {
    port.erase(links.model)?;
    port.send(links.sail, MSG_SAIL_DEL_GROUP, ship)?;
    port.send(links.rope, MSG_ROPE_DEL_GROUP, links.model)?;
    port.send(links.flag, MSG_FLAG_DEL_GROUP, links.model)?;
    for service in links.vants {
        port.send(service, MSG_VANT_DEL_GROUP, links.model)?;
    }
    port.erase(links.blots)?;
    if let Some(tracks) = port.tracks() {
        port.del_ship(tracks, ship)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
