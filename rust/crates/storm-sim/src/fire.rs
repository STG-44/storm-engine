//! Retained FirePlace lifecycle; particle and sound services are explicit ports.
use crate::ship_queries::ShipQueries;
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use storm_core::{AttributeStore, Entity, Message, Value};

pub const MSG_SHIP_ACTIVATE_FIRE_PLACE: i32 = 50304;
pub const MSG_SHIP_SAFE_DELETE: i32 = 50315;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParticleKind {
    Smoke,
    Fire,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct ParticleId(pub u64);

pub trait FireEffects {
    fn has_particles(&self) -> bool;
    fn create_particle(
        &mut self,
        kind: ParticleKind,
        name: &str,
        locator: usize,
        position: [f32; 3],
    ) -> Option<ParticleId>;
    fn validate_particle(&mut self, id: ParticleId) -> bool;
    fn pause_particle(&mut self, id: ParticleId);
    fn move_particle(&mut self, id: ParticleId, position: [f32; 3]);
    fn play_sound(&mut self, name: &str, position: [f32; 3]) -> i32;
    fn stop_sound(&mut self, id: i32);
    fn move_sound(&mut self, id: i32, position: [f32; 3]);
}

#[derive(Debug)]
pub struct FirePlace {
    active: bool,
    remaining: Option<f32>, // Native constructor leaves fRunTime uninitialized.
    smoke: Option<ParticleId>,
    fire: Option<ParticleId>,
    sound: i32,
    owner: i32,
    names: [String; 3],
}
impl Default for FirePlace {
    fn default() -> Self {
        Self {
            active: false,
            remaining: None,
            smoke: None,
            fire: None,
            sound: 0,
            owner: -1,
            names: Default::default(),
        }
    }
}
impl FirePlace {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn remaining(&self) -> Option<f32> {
        self.remaining
    }
    pub fn owner(&self) -> i32 {
        self.owner
    }
    pub fn sound(&self) -> i32 {
        self.sound
    }
    pub fn names(&self) -> &[String; 3] {
        &self.names
    }
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        smoke: &str,
        fire: &str,
        owner: i32,
        sound: &str,
        duration: f32,
        locator: usize,
        position: [f32; 3],
        effects: &mut impl FireEffects,
    ) {
        if self.active || !effects.has_particles() {
            return;
        }
        self.smoke = effects.create_particle(ParticleKind::Smoke, smoke, locator, position);
        self.fire = effects.create_particle(ParticleKind::Fire, fire, locator, position);
        self.remaining = Some(duration);
        self.sound = effects.play_sound(sound, position);
        self.active = true;
        self.names = [smoke.into(), fire.into(), sound.into()];
        self.owner = owner;
    }
    pub fn stop(&mut self, effects: &mut impl FireEffects) {
        if !self.active {
            return;
        }
        if effects.has_particles() {
            for id in [self.smoke, self.fire].into_iter().flatten() {
                if effects.validate_particle(id) {
                    effects.pause_particle(id);
                }
            }
        }
        self.smoke = None;
        self.fire = None;
        if self.sound != 0 {
            effects.stop_sound(self.sound);
        }
        self.sound = 0;
        self.owner = -1;
        self.active = false;
    }
    pub fn execute(&mut self, delta: f32, position: [f32; 3], effects: &mut impl FireEffects) {
        if !self.active {
            return;
        }
        let remaining = self.remaining.expect("active FirePlace has a Run time") - delta;
        self.remaining = Some(remaining);
        if remaining < 0.0 {
            self.stop(effects);
            return;
        }
        for id in [self.smoke, self.fire].into_iter().flatten() {
            effects.move_particle(id, position);
        }
        effects.move_sound(self.sound, position);
    }
}

pub trait FireLocator {
    fn fire_position(&self) -> [f32; 3];
}
pub struct FireSlot<F> {
    pub locator: F,
    pub state: FirePlace,
}

pub struct ShipFire<F: FireLocator, B: FireEffects> {
    pub queries: ShipQueries<FireSlot<F>>,
    pub effects: B,
    transform: Mat4,
}
impl<F: FireLocator, B: FireEffects> ShipFire<F, B> {
    pub fn new(locators: Vec<F>, effects: B) -> Self {
        Self {
            queries: ShipQueries::new(
                locators
                    .into_iter()
                    .map(|locator| FireSlot {
                        locator,
                        state: FirePlace::new(),
                    })
                    .collect(),
            ),
            effects,
            transform: Mat4::IDENTITY,
        }
    }
    pub fn set_transform(&mut self, transform: Mat4) -> Result<()> {
        ensure!(transform.is_finite(), "invalid fire emitter transform");
        self.transform = transform;
        Ok(())
    }
    pub fn execute(&mut self, delta: f32) {
        for slot in self.queries.fire_places_mut() {
            let position = self
                .transform
                .transform_point3(Vec3::from_array(slot.locator.fire_position()))
                .to_array();
            slot.state.execute(delta, position, &mut self.effects);
        }
    }
    pub fn stop_all(&mut self) {
        // Match the retained libc++ oracle's vector-clear destruction order.
        for slot in self.queries.fire_places_mut().iter_mut().rev() {
            slot.state.stop(&mut self.effects);
        }
    }
    pub fn clear(&mut self) {
        self.stop_all();
        self.queries.fire_places_mut().clear();
    }
    fn fire_message(&mut self, message: &mut Message) -> Result<Option<u64>> {
        match message.current_value() {
            Some(Value::Long(MSG_SHIP_ACTIVATE_FIRE_PLACE)) => {
                message.long()?;
                let index = message.long()? as u32;
                let smoke = message.string()?.to_owned();
                let fire = message.string()?.to_owned();
                let sound = message.string()?.to_owned();
                let duration = message.float()?;
                let owner = message.long()?;
                ensure!(
                    index != u32::MAX && (index as usize) < self.queries.fire_places().len(),
                    "invalid SHIP fireplace index"
                );
                let slot = &mut self.queries.fire_places_mut()[index as usize];
                let position = self
                    .transform
                    .transform_point3(Vec3::from_array(slot.locator.fire_position()))
                    .to_array();
                slot.state.run(
                    &smoke,
                    &fire,
                    owner,
                    &sound,
                    duration,
                    index as usize,
                    position,
                    &mut self.effects,
                );
                Ok(Some(0))
            }
            Some(Value::Long(MSG_SHIP_SAFE_DELETE)) => {
                message.long()?;
                self.clear();
                Ok(Some(0))
            }
            _ => Ok(None),
        }
    }
}
impl<F: FireLocator + 'static, B: FireEffects> Entity for ShipFire<F, B> {
    fn process_message(&mut self, message: &mut Message) -> Result<u64> {
        if let Some(result) = self.fire_message(message)? {
            return Ok(result);
        }
        self.queries.process_message(message)
    }
    fn process_message_with_attributes(
        &mut self,
        message: &mut Message,
        store: &mut AttributeStore,
    ) -> Result<u64> {
        if let Some(result) = self.fire_message(message)? {
            return Ok(result);
        }
        self.queries.process_message_with_attributes(message, store)
    }
}
impl<F: FireLocator, B: FireEffects> Drop for ShipFire<F, B> {
    fn drop(&mut self) {
        self.clear();
    }
}

#[cfg(test)]
mod tests;
