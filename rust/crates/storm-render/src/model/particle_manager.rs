//! Shared project data and independently owned runtime systems.
use super::{
    FireEmission, MAX_PARTICLE_INSTANCES, ParticleStream, ParticleStyle, particle_profile_key,
};
use anyhow::{Context, Result, ensure};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, Mutex},
};
use storm_formats::particles::Profile;

const CACHE_BYTES: usize = 64 * 1024 * 1024;
#[derive(Debug)]
pub struct ParticleDefinition {
    pub key: String,
    pub styles: Vec<ParticleStyle>,
}
#[derive(Debug, Default)]
pub struct ParticleLibrary {
    profiles: BTreeMap<String, Profile>,
    prepared: Mutex<BTreeMap<String, Arc<ParticleDefinition>>>,
    bytes: usize,
}
impl ParticleLibrary {
    /// Cache the project's descriptions, including currently unsupported model types.
    /// Billboard resources are prepared only when a system is requested.
    pub fn from_bytes(profiles: impl IntoIterator<Item = (String, Vec<u8>)>) -> Result<Self> {
        let mut library = Self::default();
        for (name, bytes) in profiles {
            let key = particle_profile_key(&name)?;
            ensure!(
                library.profiles.len() < 9999,
                "particle project exceeds profile budget"
            );
            library.bytes = library
                .bytes
                .checked_add(bytes.len())
                .context("particle cache size overflow")?;
            ensure!(
                library.bytes <= CACHE_BYTES,
                "particle project exceeds 64 MiB source budget"
            );
            let profile =
                Profile::read(&bytes).with_context(|| format!("XPS project profile {key}"))?;
            ensure!(
                library.profiles.insert(key, profile).is_none(),
                "duplicate particle cache key"
            );
        }
        Ok(library)
    }
    pub fn contains(&self, name: &str) -> bool {
        particle_profile_key(name).is_ok_and(|key| self.profiles.contains_key(&key))
    }
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.profiles.keys()
    }
    pub fn source_bytes(&self) -> usize {
        self.bytes
    }
    pub fn prepared_count(&self) -> Result<usize> {
        Ok(self
            .prepared
            .lock()
            .map_err(|_| anyhow::anyhow!("particle library lock poisoned"))?
            .len())
    }
    pub fn definition(&self, name: &str) -> Result<Option<Arc<ParticleDefinition>>> {
        let key = particle_profile_key(name)?;
        let Some(profile) = self.profiles.get(&key) else {
            return Ok(None);
        };
        let mut prepared = self
            .prepared
            .lock()
            .map_err(|_| anyhow::anyhow!("particle library lock poisoned"))?;
        if let Some(definition) = prepared.get(&key) {
            return Ok(Some(definition.clone()));
        }
        // Preserve the existing preview RNG seed for the two standard profiles.
        let source = match key.as_str() {
            "ship_fire.xps" => "Ship_Fire.xps",
            "ship_smoke.xps" => "Ship_Smoke.xps",
            _ => &key,
        };
        let mut styles = Vec::new();
        for emitter in &profile.emitters {
            for particle in &emitter.particles {
                ensure!(
                    particle.kind == 1,
                    "XPS project profile {key} contains unsupported model particles"
                );
                ensure!(
                    styles.len() < 256,
                    "particle profile exceeds description budget"
                );
                let mut style =
                    ParticleStyle::read(source.into(), &emitter.fields, &particle.fields)?;
                style.preview = false;
                styles.push(style);
            }
        }
        let definition = Arc::new(ParticleDefinition {
            key: key.clone(),
            styles,
        });
        prepared.insert(key, definition.clone());
        Ok(Some(definition))
    }
}
#[derive(Debug)]
struct System {
    owner: u64,
    emission: FireEmission,
}
#[derive(Debug)]
pub struct ParticleManager {
    pub library: Arc<ParticleLibrary>,
    systems: Vec<System>,
    styles: BTreeMap<String, Vec<ParticleStyle>>,
    next_id: u64,
    now: f32,
}
impl ParticleManager {
    pub fn new(library: Arc<ParticleLibrary>) -> Self {
        Self {
            library,
            systems: Vec::new(),
            styles: BTreeMap::new(),
            next_id: 0,
            now: 0.0,
        }
    }
    pub fn len(&self) -> usize {
        self.systems.len()
    }
    pub fn is_empty(&self) -> bool {
        self.systems.is_empty()
    }
    pub fn ids(&self) -> Vec<u64> {
        self.systems.iter().map(|s| s.emission.instance).collect()
    }
    pub fn owners(&self) -> BTreeSet<u64> {
        self.systems.iter().map(|s| s.owner).collect()
    }
    pub fn create(
        &mut self,
        owner: u64,
        name: &str,
        point: usize,
        smoke: bool,
    ) -> Result<Option<u64>> {
        ensure!(
            owner != 0 && point < 16384,
            "invalid particle system owner or locator"
        );
        if self.systems.len() >= MAX_PARTICLE_INSTANCES {
            return Ok(None);
        }
        let Some(definition) = self.library.definition(name)? else {
            return Ok(None);
        };
        let id = self
            .next_id
            .checked_add(1)
            .context("particle instance identity exhausted")?;
        self.styles
            .entry(definition.key.clone())
            .or_insert_with(|| definition.styles.clone());
        self.systems.push(System {
            owner,
            emission: FireEmission {
                instance: id,
                point,
                smoke,
                started: self.now,
                stopped: None,
                profile: Some(definition.key.clone()),
            },
        });
        self.next_id = id;
        Ok(Some(id))
    }
    pub fn validate(&self, id: u64) -> bool {
        self.systems.iter().any(|s| s.emission.instance == id)
    }
    pub fn pause(&mut self, id: u64) {
        if let Some(system) = self.systems.iter_mut().find(|s| s.emission.instance == id) {
            // Pause(true) does not advance the emitter clock and is idempotent.
            system.emission.stopped.get_or_insert(self.now);
        }
    }
    pub fn release(&mut self, id: u64) -> bool {
        if let Some(index) = self.systems.iter().position(|s| s.emission.instance == id) {
            self.systems.swap_remove(index);
            true
        } else {
            false
        }
    }
    pub fn release_owner(&mut self, owner: u64) {
        let ids: Vec<_> = self
            .systems
            .iter()
            .filter(|s| s.owner == owner)
            .map(|s| s.emission.instance)
            .collect();
        for id in ids {
            self.release(id);
        }
    }
    pub fn clear(&mut self) {
        self.systems.clear();
        self.styles.clear();
    }
    pub fn emissions(&self, owner: u64) -> Vec<FireEmission> {
        self.systems
            .iter()
            .filter(|s| s.owner == owner)
            .map(|s| s.emission.clone())
            .collect()
    }
    pub fn all_emissions(&self) -> Vec<FireEmission> {
        self.systems.iter().map(|s| s.emission.clone()).collect()
    }
    fn reclaim_replay(&self) -> Result<()> {
        let retained = self.systems.iter().map(|s| s.emission.instance).collect();
        for styles in self.styles.values() {
            for style in styles {
                style.retain_instances(&retained)?;
            }
        }
        Ok(())
    }
    /// PARTICLES::Realize releases systems with neither a running emitter nor live particles.
    /// A paused looping emitter stays alive, as in native IsAlive; explicit release is required.
    pub fn advance(&mut self, now: f32) -> Result<()> {
        ensure!(
            now.is_finite() && now >= self.now,
            "invalid particle manager clock"
        );
        self.reclaim_replay()?;
        let mut dead = Vec::new();
        for system in &self.systems {
            let e = &system.emission;
            let mut alive = false;
            for style in &self.styles[e.profile.as_ref().unwrap()] {
                alive |= style.instance_alive(
                    now,
                    e.started,
                    e.stopped,
                    ParticleStream {
                        locator: e.point,
                        instance: e.instance,
                    },
                )?;
            }
            if !alive {
                dead.push(e.instance);
            }
        }
        self.now = now;
        for id in dead {
            self.release(id);
        }
        self.reclaim_replay()?;
        Ok(())
    }
}
#[cfg(test)]
mod tests;
