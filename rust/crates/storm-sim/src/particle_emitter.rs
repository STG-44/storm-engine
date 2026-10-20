//! Retained BaseEmitter birth accumulation, with graph/matrix/processor ports.
use anyhow::{Result, ensure};

pub const SUBSTEPS: usize = 32;
const MAX_ATTEMPTS: usize = 65_536;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Billboard,
    Model,
}
#[derive(Clone, Debug)]
pub struct ParticleType {
    pub kind: Kind,
    pub enabled: bool,
    pub remainder: f32,
    pub active: u32,
    pub maximum: u32,
}
impl ParticleType {
    pub fn new(kind: Kind, maximum: u32) -> Self {
        Self {
            kind,
            enabled: true,
            remainder: 0.0,
            active: 0,
            maximum,
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Birth {
    pub particle_type: usize,
    pub kind: Kind,
    pub emitter_time: f32,
    pub emitter_lifetime: f32,
    pub substep: usize,
    pub matrix_blend: f32,
}
pub trait BirthPort {
    fn rate(&mut self, particle_type: usize, time: f32, lifetime: f32) -> f32;
    /// False models processor allocation failure, which undoes the active increment.
    fn birth(&mut self, birth: Birth) -> bool;
    /// Native BornParticles commits old=new only after all substeps finish.
    fn finish(&mut self) {}
}
#[derive(Clone, Debug)]
pub struct Emitter {
    pub elapsed: f32,
    pub lifetime: f32,
    pub looping: bool,
    pub stopped: bool,
    pub enabled: bool,
    pub attached: bool,
    pub types: Vec<ParticleType>,
}
impl Emitter {
    pub fn new(lifetime: f32, looping: bool, types: Vec<ParticleType>) -> Self {
        Self {
            elapsed: 0.0,
            lifetime,
            looping,
            stopped: false,
            enabled: true,
            attached: false,
            types,
        }
    }
    fn increase_time(&mut self, delta: f32) {
        self.elapsed += delta;
        if self.elapsed > self.lifetime {
            if !self.looping {
                self.stopped = true;
            }
            self.elapsed -= self.lifetime;
        }
    }
    fn born(&mut self, delta: f32, port: &mut impl BirthPort) -> Result<()> {
        let saved = self.elapsed;
        if !self.enabled {
            return Ok(());
        }
        let mut matrix_blend = 0.0f32;
        let mut attempts = 0usize;
        for substep in 0..SUBSTEPS {
            let blend = matrix_blend;
            matrix_blend += 1.0 / SUBSTEPS as f32;
            let step = delta / SUBSTEPS as f32;
            self.increase_time(step);
            if self.stopped {
                self.elapsed = saved;
                return Ok(());
            }
            for (index, particle) in self.types.iter_mut().enumerate() {
                if !particle.enabled {
                    continue;
                }
                let rate = port.rate(index, self.elapsed, self.lifetime);
                ensure!(rate.is_finite(), "non-finite native particle emission rate");
                particle.remainder += rate * step;
                ensure!(
                    particle.remainder.is_finite(),
                    "native emission remainder overflow"
                );
                while particle.remainder >= 1.0 {
                    ensure!(
                        attempts < MAX_ATTEMPTS,
                        "native particle birth attempt budget exceeded"
                    );
                    attempts += 1;
                    particle.remainder -= 1.0;
                    if particle.active < particle.maximum {
                        particle.active += 1;
                        if !port.birth(Birth {
                            particle_type: index,
                            kind: particle.kind,
                            emitter_time: self.elapsed,
                            emitter_lifetime: self.lifetime,
                            substep,
                            matrix_blend: blend,
                        }) {
                            particle.active -= 1;
                        }
                    }
                }
            }
        }
        // Matrix history is handled by the embedding. Substep time is temporary.
        self.elapsed = saved;
        port.finish();
        Ok(())
    }
    pub fn execute(&mut self, delta: f32, port: &mut impl BirthPort) -> Result<()> {
        ensure!(
            delta.is_finite() && self.elapsed.is_finite() && self.lifetime.is_finite(),
            "invalid native emitter time"
        );
        ensure!(
            self.types.len() <= 256 && self.types.iter().all(|t| t.remainder.is_finite()),
            "invalid native emitter types"
        );
        if !self.stopped && !self.attached {
            self.born(delta, port)?;
        }
        self.increase_time(delta);
        ensure!(self.elapsed.is_finite(), "native emitter clock overflow");
        Ok(())
    }
    pub fn release(&mut self, particle_type: usize) -> Result<()> {
        let kind = self
            .types
            .get_mut(particle_type)
            .ok_or_else(|| anyhow::anyhow!("invalid particle type release"))?;
        ensure!(kind.active > 0, "native particle active-count underflow");
        kind.active -= 1;
        Ok(())
    }
}
#[derive(Clone, Debug, Default)]
pub struct System {
    pub paused: bool,
    pub emitters: Vec<Emitter>,
}
impl System {
    pub fn execute(&mut self, delta: f32, port: &mut impl BirthPort) -> Result<usize> {
        if !self.paused {
            for emitter in &mut self.emitters {
                emitter.execute(delta, port)?;
            }
        }
        Ok(self.emitters.len())
    }
}
#[cfg(test)]
mod tests;
