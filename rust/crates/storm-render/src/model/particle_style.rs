//! XPS appearance with retained billboard birth/motion and fixed-frame replay.
use super::particle_replay::{BirthStats, LiveParticle, MotionStats, Replay};
use anyhow::{Context, Result, ensure};
use glam::Vec3;
use std::sync::Mutex;
use storm_formats::particles::{ColorGraph, Fields, Graph, Value};

pub const MAX_PARTICLE_LIFETIME: f32 = 60.0;
#[derive(Clone, Debug, PartialEq)]
pub struct MotionProfile {
    pub direction: [Graph; 3],
    pub mass: Graph,
    pub spin: Graph,
    pub velocity: Graph,
    pub drag: Graph,
    pub spin_drag: Graph,
    pub gravity: Graph,
    pub blend: Graph,
}
#[derive(Debug)]
pub struct ParticleStyle {
    replay: Mutex<Replay>,
    pub source: String,
    /// Include this profile in the manual fire preview, without native messages.
    pub preview: bool,
    pub emitter_position: Vec3,
    pub emitter_lifetime: f32,
    pub looping: bool,
    pub count: usize,
    pub lifetime: Graph,
    pub rate: Graph,
    pub size: Graph,
    pub transparency: Graph,
    pub color: ColorGraph,
    pub frames: Vec<[f32; 4]>,
    pub animation: Graph,
    pub track: [Graph; 3],
    pub motion: MotionProfile,
}
#[derive(Clone, Debug)]
pub struct StyleSample {
    pub serial: u64,
    pub birth: f32,
    pub offset: Vec3,
    pub angle: f32,
    pub radius: f32,
    pub color: [f32; 4],
    pub uv: [f32; 4],
}
impl ParticleStyle {
    pub fn read(source: String, emitter: &Fields, particle: &Fields) -> Result<Self> {
        let graph = |name| {
            particle
                .graph(name)
                .cloned()
                .with_context(|| format!("XPS {source}: missing graph {name}"))
        };
        let emitter_graph = |name| {
            emitter
                .graph(name)
                .cloned()
                .with_context(|| format!("XPS {source}: missing graph {name}"))
        };
        let emitter_lifetime = emitter
            .float("Life time")
            .context("XPS emitter lifetime missing")?;
        let count = particle
            .float("Max Particles count")
            .context("XPS particle count missing")?;
        ensure!(
            emitter_lifetime > 0.0 && emitter_lifetime.is_finite(),
            "invalid XPS emitter lifetime"
        );
        ensure!(
            count.is_finite() && (1.0..=2048.0).contains(&count),
            "XPS preview particle count exceeds 1..=2048"
        );
        let color = match particle.get("Color") {
            Some(Value::Color(c)) => c.clone(),
            _ => anyhow::bail!("XPS color graph missing"),
        };
        let frames = match particle.get("Texture") {
            Some(Value::Uv(v)) if !v.is_empty() => v.clone(),
            _ => anyhow::bail!("XPS UV frames missing"),
        };
        let position = match emitter.get("Position") {
            Some(Value::Position(v)) => Vec3::new(v[0], v[1], -v[2]),
            _ => anyhow::bail!("XPS emitter position missing"),
        };
        let lifetime = graph("Life time")?;
        ensure!(
            lifetime.maximum() > 0.0 && lifetime.maximum() <= MAX_PARTICLE_LIFETIME,
            "XPS preview particle lifetime exceeds 0..=60 seconds"
        );
        let style = Self {
            replay: Mutex::new(Replay::default()),
            emitter_position: position,
            emitter_lifetime,
            looping: matches!(emitter.get("Looping"), Some(Value::Bool(true))),
            count: count as usize,
            lifetime,
            rate: graph("Emission rate")?,
            size: graph("Size")?,
            transparency: graph("Transparency")?,
            color,
            frames,
            animation: graph("Animation frame")?,
            track: [graph("Track X")?, graph("Track Y")?, graph("Track Z")?],
            motion: MotionProfile {
                direction: [
                    emitter_graph("Physics Dir X")?,
                    emitter_graph("Physics Dir Y")?,
                    emitter_graph("Physics Dir Z")?,
                ],
                mass: graph("Mass")?,
                spin: graph("Spin")?,
                velocity: graph("Velocity")?,
                drag: graph("Friction")?,
                spin_drag: graph("Spin Friction")?,
                gravity: graph("ForceField_gravity")?,
                blend: graph("Physic blend")?,
            },
            source,
            preview: true,
        };
        Ok(style)
    }
    fn appearance(&self, seconds: f32, start: f32, particle: &LiveParticle) -> Option<StyleSample> {
        let age = particle.body.age;
        let lifetime = particle.body.lifetime;
        let random = particle.random;
        if lifetime <= 0.0 || self.frames.is_empty() {
            return None;
        }
        let radius = self.size.sample(age, lifetime, random[2]);
        if radius <= 0.000001 {
            return None;
        }
        let mut color = self.color.sample(age, lifetime, random[3]);
        color[3] =
            (1.0 - self.transparency.sample(age, lifetime, random[4]) * 0.01).clamp(0.0, 1.0);
        let frame = self.animation.sample(age, lifetime, random[5]).max(0.0) as u32 as usize
            % self.frames.len();
        let position = particle.body.render_position;
        let sample = StyleSample {
            serial: particle.serial,
            birth: (seconds - age).max(start),
            offset: Vec3::new(position[0], position[1], -position[2]),
            angle: particle.body.render_angle,
            radius,
            color,
            uv: self.frames[frame],
        };
        if !sample.offset.is_finite()
            || !sample.radius.is_finite()
            || !sample.angle.is_finite()
            || !sample.color.iter().chain(&sample.uv).all(|v| v.is_finite())
        {
            return None;
        }
        Some(sample)
    }
    pub fn samples(
        &self,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        locator: usize,
    ) -> Result<Vec<Option<StyleSample>>> {
        let (particles, _) = self
            .replay
            .lock()
            .map_err(|_| anyhow::anyhow!("particle replay lock poisoned"))?
            .snapshot(self, seconds, start, stop, locator.into(), None)?;
        if seconds < start {
            return Ok(vec![None; self.count]);
        }
        let mut result = particles
            .iter()
            .map(|p| self.appearance(seconds, start, p))
            .collect::<Vec<_>>();
        result.resize(self.count, None);
        Ok(result)
    }
    pub fn birth_stats(
        &self,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        locator: usize,
    ) -> Result<BirthStats> {
        Ok(self
            .replay
            .lock()
            .map_err(|_| anyhow::anyhow!("particle replay lock poisoned"))?
            .snapshot(self, seconds, start, stop, locator.into(), None)?
            .1)
    }
    pub fn motion_stats(
        &self,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        locator: usize,
    ) -> Result<MotionStats> {
        let (particles, _) = self
            .replay
            .lock()
            .map_err(|_| anyhow::anyhow!("particle replay lock poisoned"))?
            .snapshot(self, seconds, start, stop, locator.into(), None)?;
        Ok(MotionStats::of(&particles))
    }
    pub fn world_samples(
        &self,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        locator: usize,
        world: super::WorldEmitter,
    ) -> Result<(Vec<Option<StyleSample>>, BirthStats, MotionStats)> {
        self.instance_snapshot(seconds, start, stop, locator.into(), Some(world))
    }
    pub fn instance_alive(
        &self,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        stream: super::ParticleStream,
    ) -> Result<bool> {
        self.replay
            .lock()
            .map_err(|_| anyhow::anyhow!("particle replay lock poisoned"))?
            .alive(self, seconds, start, stop, stream)
    }
    pub fn retain_instances(&self, instances: &std::collections::BTreeSet<u64>) -> Result<()> {
        self.replay
            .lock()
            .map_err(|_| anyhow::anyhow!("particle replay lock poisoned"))?
            .retain_instances(instances);
        Ok(())
    }
    pub fn instance_snapshot(
        &self,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        stream: super::ParticleStream,
        world: Option<super::WorldEmitter>,
    ) -> Result<(Vec<Option<StyleSample>>, BirthStats, MotionStats)> {
        let (particles, births) = self
            .replay
            .lock()
            .map_err(|_| anyhow::anyhow!("particle replay lock poisoned"))?
            .snapshot(self, seconds, start, stop, stream, world)?;
        let motion = MotionStats::of(&particles);
        let mut samples = if seconds < start {
            vec![]
        } else {
            particles
                .iter()
                .map(|p| self.appearance(seconds, start, p))
                .collect::<Vec<_>>()
        };
        samples.resize(self.count, None);
        Ok((samples, births, motion))
    }
    pub fn sample(&self, seconds: f32, start: f32, index: usize) -> Option<StyleSample> {
        if index >= self.count || self.frames.is_empty() {
            return None;
        }
        self.samples(seconds, start, None, 0)
            .ok()?
            .get(index)?
            .clone()
    }
}
impl Clone for ParticleStyle {
    fn clone(&self) -> Self {
        Self {
            replay: Mutex::new(Replay::default()),
            source: self.source.clone(),
            preview: self.preview,
            emitter_position: self.emitter_position,
            emitter_lifetime: self.emitter_lifetime,
            looping: self.looping,
            count: self.count,
            lifetime: self.lifetime.clone(),
            rate: self.rate.clone(),
            size: self.size.clone(),
            transparency: self.transparency.clone(),
            color: self.color.clone(),
            frames: self.frames.clone(),
            animation: self.animation.clone(),
            track: self.track.clone(),
            motion: self.motion.clone(),
        }
    }
}
#[cfg(test)]
mod tests;
