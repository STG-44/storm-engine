//! Explicit registry/model/group ports for the retained SHIP destructor.
//! Original geometry allocation, service rendering and buffer compaction remain external.
use super::*;
use std::{cell::Cell, rc::Weak};
use storm_core::{EntityDestroy, EntityManager};
use storm_sim::ship_cleanup::{self as native, CleanupPort, GroupService, Groups, Links};
pub(super) const NAMES: [&str; 6] = ["sail", "rope", "flag", "vant", "vantl", "vantz"];
pub(super) const CODES: [i32; 6] = [
    native::MSG_SAIL_DEL_GROUP,
    native::MSG_ROPE_DEL_GROUP,
    native::MSG_FLAG_DEL_GROUP,
    native::MSG_VANT_DEL_GROUP,
    native::MSG_VANT_DEL_GROUP,
    native::MSG_VANT_DEL_GROUP,
];
pub(super) struct World {
    groups: [Rc<RefCell<Groups>>; 6],
    services: [EntityId; 6],
    tracks_service: EntityId,
    tracks: Vec<EntityId>,
    ships: Vec<EntityId>,
    links: Vec<Links>,
    retired: Vec<usize>,
    applied: [bool; 2],
    events: Vec<Json>,
}
#[derive(Clone)]
pub(super) struct Binding {
    links: Links,
    world: Rc<RefCell<World>>,
    index: usize,
    controller: Weak<RefCell<Controller>>,
}
impl Binding {
    pub fn safe_delete(&self, message: &mut Message) -> Result<u64> {
        if let Some(controller) = self.controller.upgrade() {
            controller.borrow_mut().process_message(message)
        } else {
            Ok(0)
        }
    }
    pub fn destroy(&self, context: &mut EntityDestroy<'_>) -> Result<()> {
        native::release(
            context.id,
            self.links,
            &mut Port {
                manager: context.manager,
                world: self.world.clone(),
                ship: context.id,
                model: self.links.model,
            },
        )?;
        self.world.borrow_mut().retired.push(self.index);
        Ok(())
    }
    pub fn dropped(&self, id: EntityId) {
        self.world
            .borrow_mut()
            .events
            .push(json!(["drop", id.raw(), "ship"]));
    }
}
struct Port<'a> {
    manager: &'a mut EntityManager,
    world: Rc<RefCell<World>>,
    ship: EntityId,
    model: EntityId,
}
impl CleanupPort for Port<'_> {
    fn erase(&mut self, id: EntityId) -> Result<()> {
        self.manager.erase(id);
        self.world.borrow_mut().events.push(json!([
            "erase",
            id.raw(),
            self.ship.raw(),
            self.manager.get(self.ship).is_some(),
            self.manager.is_valid(self.ship),
            self.manager.get(self.model).is_some(),
            self.manager.is_valid(self.model)
        ]));
        Ok(())
    }
    fn send(&mut self, service: EntityId, code: i32, owner: EntityId) -> Result<()> {
        self.world.borrow_mut().events.push(json!([
            "send",
            service.raw(),
            code,
            owner.raw(),
            self.manager.is_valid(service),
            self.manager.get(self.model).is_some(),
            self.manager.is_valid(self.model)
        ]));
        self.manager.send_message(
            service,
            &mut Message::new("li", vec![Value::Long(code), Value::Entity(owner)])?,
        )?;
        Ok(())
    }
    fn tracks(&mut self) -> Option<EntityId> {
        let id = self.manager.ids("ShipTracks").first().copied()?;
        self.world
            .borrow_mut()
            .events
            .push(json!(["track_lookup", id.raw()]));
        self.manager.get(id).map(|_| id)
    }
    fn del_ship(&mut self, tracks: EntityId, ship: EntityId) -> Result<()> {
        let mut world = self.world.borrow_mut();
        ensure!(tracks == world.tracks_service, "unexpected tracks provider");
        if let Some(i) = world.tracks.iter().position(|id| *id == ship) {
            world.events.push(json!(["track_drop", ship.raw()]));
            world.tracks.remove(i);
        }
        Ok(())
    }
}
struct Model {
    id: Rc<Cell<EntityId>>,
    kind: &'static str,
    world: Rc<RefCell<World>>,
}
impl Entity for Model {}
impl Drop for Model {
    fn drop(&mut self) {
        self.world
            .borrow_mut()
            .events
            .push(json!(["drop", self.id.get().raw(), self.kind]));
    }
}
struct Tracks;
impl Entity for Tracks {}
pub(super) fn create(
    manager: &mut EntityManager,
    ships: Vec<EntityId>,
    controllers: Vec<Weak<RefCell<Controller>>>,
) -> Result<(Rc<RefCell<World>>, Vec<Binding>)> {
    ensure!(
        ships.len() == 2 && controllers.len() == 2,
        "cleanup fixture requires two owners"
    );
    let world = Rc::new(RefCell::new(World {
        groups: std::array::from_fn(|_| Rc::default()),
        services: [EntityId::INVALID; 6],
        tracks_service: EntityId::INVALID,
        tracks: ships.clone(),
        ships: ships.clone(),
        links: Vec::new(),
        retired: Vec::new(),
        applied: [false; 2],
        events: Vec::new(),
    }));
    let mut services = [EntityId::INVALID; 6];
    for i in 0..6 {
        let groups = world.borrow().groups[i].clone();
        services[i] = manager.insert(
            NAMES[i],
            GroupService {
                code: CODES[i],
                groups,
            },
        )?;
    }
    let tracks_service = manager.insert("ShipTracks", Tracks)?;
    world.borrow_mut().services = services;
    world.borrow_mut().tracks_service = tracks_service;
    let mut bindings = Vec::new();
    for (index, controller) in controllers.into_iter().enumerate() {
        let mut ids = Vec::new();
        for kind in ["model", "blots"] {
            let cell = Rc::new(Cell::new(EntityId::INVALID));
            let id = manager.insert(
                kind,
                Model {
                    id: cell.clone(),
                    kind,
                    world: world.clone(),
                },
            )?;
            cell.set(id);
            ids.push(id);
        }
        let links = Links {
            model: ids[0],
            blots: ids[1],
            sail: services[0],
            rope: services[1],
            flag: services[2],
            vants: [services[3], services[4], services[5]],
        };
        for i in 0..6 {
            world.borrow().groups[i]
                .borrow_mut()
                .owners
                .push((if i == 0 { ships[index] } else { links.model }, false));
        }
        world.borrow_mut().links.push(links);
        bindings.push(Binding {
            links,
            world: world.clone(),
            index,
            controller,
        });
    }
    Ok((world, bindings))
}
impl World {
    pub fn report(&self, manager: &EntityManager) -> Json {
        json!({"source":"retained_ship_destructor_and_group_marks","full_original_services":false,"original_buffer_compaction":false,"geometry":"preview_assets_retained_for_diagnostics",
            "groups":self.groups.iter().enumerate().map(|(i,g)|{let g=g.borrow();json!({"service":NAMES[i],"entity":self.services[i].raw(),"pending":g.pending,"owners":g.owners.iter().map(|(id,d)|json!([id.raw(),d])).collect::<Vec<_>>()})}).collect::<Vec<_>>(),
            "models":self.links.iter().enumerate().map(|(i,l)|json!({"preview_owner":i+1,"ship":self.ships[i].raw(),"model":l.model.raw(),"blots":l.blots.raw(),"model_valid":manager.is_valid(l.model),"model_lookup":manager.get(l.model).is_some(),"blots_valid":manager.is_valid(l.blots),"blots_lookup":manager.get(l.blots).is_some()})).collect::<Vec<_>>(),
            "tracks":self.tracks.iter().map(|id|id.raw()).collect::<Vec<_>>(),"retired_owners":self.retired.iter().map(|i|i+1).collect::<Vec<_>>(),"preview_cleanup_applied":self.applied,"events":self.events})
    }
}
impl Fleet {
    fn cleanup_world(&self) -> Option<Rc<RefCell<World>>> {
        self.native_motion
            .as_ref()
            .and_then(|m| m.rocking.as_ref())
            .and_then(|r| r.native.as_ref())
            .and_then(sea::State::cleanup_world)
    }
    pub(super) fn ship_cleanup_enabled(&self) -> bool {
        self.cleanup_world().is_some()
    }
    pub(super) fn enable_native_ship_cleanup(&mut self) -> Result<()> {
        ensure!(
            self.ship_staged() && self.next_frame == 0,
            "cleanup requires fresh ship stages"
        );
        let controllers = self
            .sessions
            .iter()
            .map(|s| Rc::downgrade(s.controller.as_ref().unwrap()))
            .collect();
        self.native_motion
            .as_mut()
            .unwrap()
            .rocking
            .as_mut()
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .enable_cleanup(controllers)
    }
    pub(super) fn apply_ship_cleanup(&mut self) -> Result<()> {
        let world = self.cleanup_world().unwrap();
        let retired = world.borrow().retired.clone();
        for index in retired {
            if world.borrow().applied[index] {
                continue;
            }
            self.sessions[index].controller.take();
            self.sessions[index]
                .world
                .borrow_mut()
                .release_owner(self.sessions[index].owner);
            self.visible[index] = false;
            if let Some(drive) = &mut self.drive {
                let w = world.borrow();
                let deleted = w.groups[0]
                    .borrow()
                    .owners
                    .iter()
                    .find(|(id, _)| *id == w.ships[index])
                    .is_some_and(|(_, deleted)| *deleted);
                drive.groups[index].deleted = deleted;
            }
            world.borrow_mut().applied[index] = true;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
