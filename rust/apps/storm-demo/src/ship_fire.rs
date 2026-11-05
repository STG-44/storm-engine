//! Preview particle service. Native sound requests are recorded; no sound driver is bound.
use anyhow::{Result, ensure};
use serde_json::{Value as Json, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    rc::{Rc, Weak},
};
use storm_core::{Entity, EntityId, Message, Value};
use storm_render::model::{FireEmission, FirePoint, ModelPose, ShipState};
use storm_sim::fire::{FireEffects, ParticleId, ParticleKind, ShipFire};

pub type Controller = ShipFire<FirePoint, PreviewEffects>;
pub type Controllers = Rc<RefCell<BTreeMap<u64, Weak<RefCell<Controller>>>>>;
pub struct EffectsWorld {
    manager: Option<storm_render::model::ParticleManager>,
    now: f32,
    retention: f32,
    next_id: u64,
    particles: BTreeMap<ParticleId, FireEmission>,
    profiles: Option<BTreeSet<String>>,
    total: u64,
    recent: VecDeque<Json>,
}
impl Default for EffectsWorld {
    fn default() -> Self {
        Self {
            manager: None,
            now: 0.0,
            retention: 7.0,
            next_id: 0,
            particles: Default::default(),
            profiles: None,
            total: 0,
            recent: Default::default(),
        }
    }
}
impl EffectsWorld {
    pub fn set_library(
        &mut self,
        library: Option<std::sync::Arc<storm_render::model::ParticleLibrary>>,
    ) -> Result<()> {
        ensure!(
            self.particles.is_empty() && self.manager.is_none(),
            "particle manager already initialized"
        );
        if let Some(library) = library {
            let mut manager = storm_render::model::ParticleManager::new(library);
            manager.advance(self.now)?;
            self.manager = Some(manager);
        }
        Ok(())
    }
    pub fn release_owner(&mut self, owner: EntityId) {
        if let Some(manager) = &mut self.manager {
            manager.release_owner(owner.raw());
        }
    }
    pub fn emissions_for(&self, owner: EntityId) -> Vec<FireEmission> {
        self.manager
            .as_ref()
            .map_or_else(|| self.emissions(), |m| m.emissions(owner.raw()))
    }
    pub fn set_profiles(&mut self, profiles: Option<BTreeSet<String>>) {
        self.profiles = profiles;
    }
    pub fn set_retention(&mut self, seconds: f32) -> Result<()> {
        ensure!(
            seconds.is_finite()
                && (0.0..=storm_render::model::MAX_PARTICLE_LIFETIME).contains(&seconds),
            "invalid preview particle retention"
        );
        self.retention = seconds.max(7.0);
        Ok(())
    }
    fn record(&mut self, entity: EntityId, mut event: Json) {
        event["entity"] = json!(entity.raw());
        self.total += 1;
        if self.recent.len() == 256 {
            self.recent.pop_front();
        }
        self.recent.push_back(event);
    }
    pub fn set_clock(&mut self, now: f32) -> Result<()> {
        ensure!(
            now.is_finite() && now >= self.now,
            "invalid fire service clock"
        );
        if let Some(manager) = &mut self.manager {
            manager.advance(now)?;
        }
        self.now = now;
        self.particles
            .retain(|_, e| e.stopped.is_none_or(|t| now < t + self.retention));
        Ok(())
    }
    pub fn emissions(&self) -> Vec<FireEmission> {
        self.manager.as_ref().map_or_else(
            || self.particles.values().cloned().collect(),
            |m| m.all_emissions(),
        )
    }
    pub fn report(&self) -> Json {
        let mut report = json!({"total":self.total,"recent":self.recent,"sound_playback":false});
        if let Some(manager) = &self.manager {
            report["manager"] = json!({"cached":manager.library.names().count(),"prepared":manager.library.prepared_count().ok(),"systems":manager.len(),"owners":manager.owners(),"ids":manager.ids()});
        }
        report
    }
}
pub struct PreviewEffects {
    pub world: Rc<RefCell<EffectsWorld>>,
    pub entity: EntityId,
}
fn bits(position: [f32; 3]) -> [u32; 3] {
    position.map(f32::to_bits)
}
impl FireEffects for PreviewEffects {
    fn has_particles(&self) -> bool {
        true
    }
    fn create_particle(
        &mut self,
        kind: ParticleKind,
        name: &str,
        locator: usize,
        position: [f32; 3],
    ) -> Option<ParticleId> {
        let mut world = self.world.borrow_mut();
        if let Some(manager) = &mut world.manager {
            let result = manager.create(
                self.entity.raw(),
                name,
                locator,
                kind == ParticleKind::Smoke,
            );
            let (id, error) = match result {
                Ok(Some(id)) => (id, None),
                Ok(None) => (0, Some("profile_missing_or_instance_budget".to_owned())),
                Err(error) => (0, Some(error.to_string())),
            };
            let mut event = json!({"op":"create_particle","id":id,"smoke":kind==ParticleKind::Smoke,"profile":name,"locator":locator,"position_bits":bits(position)});
            if let Some(error) = error {
                event["delivered"] = json!(false);
                event["reason"] = json!(error);
            }
            world.record(self.entity, event);
            return (id != 0).then_some(ParticleId(id));
        }
        let profile = if let Some(cache) = &world.profiles {
            match storm_render::model::particle_profile_key(name) {
                Ok(key) if cache.contains(&key) => Some(key),
                _ => {
                    world.record(self.entity, json!({"op":"create_particle","id":0,"smoke":kind==ParticleKind::Smoke,"profile":name,"locator":locator,"position_bits":bits(position),"delivered":false,"reason":"profile_not_cached"}));
                    return None;
                }
            }
        } else {
            None
        };
        if world.particles.len() == storm_render::model::MAX_PARTICLE_INSTANCES {
            world.record(self.entity, json!({"op":"create_particle","id":0,"smoke":kind==ParticleKind::Smoke,"profile":name,"locator":locator,"position_bits":bits(position),"delivered":false,"reason":"instance_budget"}));
            return None;
        }
        world.next_id += 1;
        let id = ParticleId(world.next_id);
        let now = world.now;
        world.particles.insert(
            id,
            FireEmission {
                instance: id.0,
                point: locator,
                smoke: kind == ParticleKind::Smoke,
                started: now,
                stopped: None,
                profile,
            },
        );
        world.record(self.entity,json!({"op":"create_particle","id":id.0,"smoke":kind==ParticleKind::Smoke,"profile":name,"locator":locator,"position_bits":bits(position)}));
        Some(id)
    }
    fn validate_particle(&mut self, id: ParticleId) -> bool {
        let mut world = self.world.borrow_mut();
        let valid = world
            .manager
            .as_ref()
            .map_or_else(|| world.particles.contains_key(&id), |m| m.validate(id.0));
        world.record(
            self.entity,
            json!({"op":"validate_particle","id":id.0,"valid":valid}),
        );
        valid
    }
    fn pause_particle(&mut self, id: ParticleId) {
        let mut world = self.world.borrow_mut();
        let now = world.now;
        if let Some(manager) = &mut world.manager {
            manager.pause(id.0);
        }
        if let Some(e) = world.particles.get_mut(&id) {
            e.stopped = Some(now);
        }
        world.record(self.entity, json!({"op":"pause_particle","id":id.0}));
    }
    fn move_particle(&mut self, id: ParticleId, position: [f32; 3]) {
        self.world.borrow_mut().record(
            self.entity,
            json!({"op":"move_particle","id":id.0,"position_bits":bits(position)}),
        );
    }
    fn play_sound(&mut self, name: &str, position: [f32; 3]) -> i32 {
        self.world.borrow_mut().record(self.entity,json!({"op":"play_sound","profile":name,"id":0,"delivered":false,"position_bits":bits(position)}));
        0
    }
    fn stop_sound(&mut self, id: i32) {
        self.world.borrow_mut().record(
            self.entity,
            json!({"op":"stop_sound","id":id,"delivered":false}),
        );
    }
    fn move_sound(&mut self, id: i32, position: [f32; 3]) {
        self.world.borrow_mut().record(
            self.entity,
            json!({"op":"move_sound","id":id,"delivered":false,"position_bits":bits(position)}),
        );
    }
}
pub fn update(controller: &mut Controller, delta: f32, pose: ModelPose) -> Result<()> {
    controller.set_transform(pose.matrix()?)?;
    controller.execute(delta);
    Ok(())
}
pub struct Session {
    pub owner: EntityId,
    pub controller: Option<Rc<RefCell<Controller>>>,
    pub world: Rc<RefCell<EffectsWorld>>,
}
impl Session {
    pub fn toggle(&mut self) -> Result<bool> {
        let Some(controller) = &self.controller else {
            return Ok(false);
        };
        let mut fire = controller.borrow_mut();
        if fire.queries.fire_places().is_empty() {
            return Ok(false);
        }
        if fire
            .queries
            .fire_places()
            .iter()
            .any(|s| s.state.is_active())
        {
            fire.stop_all();
        } else {
            for index in 0..fire.queries.fire_places().len() {
                let names = fire.queries.fire_places()[index].state.names().clone();
                let profile = |index: usize, fallback: &str| {
                    if names[index].is_empty() {
                        fallback.to_owned()
                    } else {
                        names[index].clone()
                    }
                };
                let mut message = Message::new(
                    "llsssfl",
                    vec![
                        Value::Long(storm_sim::fire::MSG_SHIP_ACTIVATE_FIRE_PLACE),
                        Value::Long(index as i32),
                        Value::String(profile(
                            0,
                            if self.world.borrow().profiles.is_some() {
                                "Ship_Smoke"
                            } else {
                                "preview-smoke"
                            },
                        )),
                        Value::String(profile(
                            1,
                            if self.world.borrow().profiles.is_some() {
                                "Ship_Fire"
                            } else {
                                "preview-fire"
                            },
                        )),
                        Value::String(profile(2, "preview-sound")),
                        Value::Float(60.0),
                        Value::Long(-1),
                    ],
                )?;
                fire.process_message(&mut message)?;
            }
        }
        Ok(true)
    }
    pub fn reset(&mut self) {
        if let Some(controller) = &self.controller {
            controller.borrow_mut().stop_all();
        }
        let mut world = self.world.borrow_mut();
        if world.manager.is_some() {
            world.release_owner(self.owner);
        } else {
            world.particles.clear();
        }
    }
    pub fn advance(&mut self, now: f32, delta: f32, pose: ModelPose) -> Result<()> {
        self.world.borrow_mut().set_clock(now)?;
        if let Some(controller) = &self.controller {
            update(&mut controller.borrow_mut(), delta, pose)?;
        }
        Ok(())
    }
    pub fn apply(&self, state: &mut ShipState) {
        state.native_fire = Some(self.world.borrow().emissions_for(self.owner));
    }
    pub fn report(&self) -> Json {
        let states=self.controller.as_ref().map_or_else(Vec::new,|controller|controller.borrow().queries.fire_places().iter().map(|slot|
            json!({"active":slot.state.is_active(),"remaining_bits":slot.state.remaining().map(f32::to_bits),"owner":slot.state.owner(),"sound":slot.state.sound(),"profiles":slot.state.names()})).collect());
        let emissions = self.world.borrow().emissions_for(self.owner).iter().map(|e| {
            let mut report = json!({"instance":e.instance,"locator":e.point,"smoke":e.smoke,"started_bits":e.started.to_bits(),"stopped_bits":e.stopped.map(f32::to_bits)});
            if let Some(profile) = &e.profile { report["profile"] = json!(profile); }
            report
        }).collect::<Vec<_>>();
        json!({"places":states,"effects":self.world.borrow().report(),"emissions":emissions})
    }
}

#[cfg(test)]
mod tests;
