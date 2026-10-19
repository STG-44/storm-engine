//! XPS appearance on deterministic preview slots; not the native emitter/physics processor.
use anyhow::{Context, Result, ensure};
use glam::Vec3;
use storm_formats::particles::{ColorGraph, Fields, Graph, Value};

pub const MAX_PARTICLE_LIFETIME: f32 = 60.0;
#[derive(Clone, Debug)]
pub struct ParticleStyle {
    pub source: String,
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
}
pub struct StyleSample {
    pub birth: f32,
    pub offset: Vec3,
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
            source,
        };
        Ok(style)
    }
    pub fn sample(&self, seconds: f32, start: f32, index: usize) -> Option<StyleSample> {
        if !seconds.is_finite()
            || !start.is_finite()
            || !self.emitter_lifetime.is_finite()
            || self.emitter_lifetime <= 0.0
            || self.frames.is_empty()
            || index >= self.count
        {
            return None;
        }
        let elapsed = seconds - start;
        if elapsed < 0.0 {
            return None;
        }
        let emitter_time = if self.looping {
            elapsed.rem_euclid(self.emitter_lifetime)
        } else {
            elapsed.min(self.emitter_lifetime)
        };
        // Diagnostic scheduling at the mean rate; original 32-step accumulation/RNG is pending.
        let rate = self
            .rate
            .sample(emitter_time, self.emitter_lifetime, 0.5)
            .clamp(0.0, 2048.0);
        if !rate.is_finite() || rate <= 0.0 {
            return None;
        }
        let period = self.count as f32 / rate;
        let age = (elapsed - (index + 1) as f32 / rate).rem_euclid(period);
        let birth = seconds - age;
        let birth_elapsed = birth - start;
        if birth_elapsed < 0.0 || (!self.looping && birth_elapsed > self.emitter_lifetime) {
            return None;
        }
        let birth_time = if self.looping {
            birth_elapsed.rem_euclid(self.emitter_lifetime)
        } else {
            birth_elapsed
        };
        let random = |salt: u32| {
            ((index as u32)
                .wrapping_mul(1664525)
                .wrapping_add(salt.wrapping_mul(1013904223))
                % 1000) as f32
                / 1000.0
        };
        let lifetime = self
            .lifetime
            .sample(birth_time, self.emitter_lifetime, random(1));
        if !lifetime.is_finite() || lifetime <= 0.0 || age > lifetime {
            return None;
        }
        let radius = self.size.sample(age, lifetime, random(2));
        if radius <= 0.000001 {
            return None;
        }
        let mut color = self.color.sample(age, lifetime, random(3));
        color[3] =
            (1.0 - self.transparency.sample(age, lifetime, random(4)) * 0.01).clamp(0.0, 1.0);
        let frame = self.animation.sample(age, lifetime, random(5)).max(0.0) as u32 as usize
            % self.frames.len();
        let track = Vec3::new(
            self.track[0].sample(age, lifetime, random(6)),
            self.track[1].sample(age, lifetime, random(7)),
            -self.track[2].sample(age, lifetime, random(8)),
        );
        let sample = StyleSample {
            birth,
            offset: self.emitter_position + track,
            radius,
            color,
            uv: self.frames[frame],
        };
        if !sample.offset.is_finite()
            || !sample.radius.is_finite()
            || !sample.color.iter().chain(&sample.uv).all(|v| v.is_finite())
        {
            return None;
        }
        Some(sample)
    }
}
#[cfg(test)]
mod tests;
