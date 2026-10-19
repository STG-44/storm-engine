//! Fixed-frame replay adapter around the retained native birth accumulator.
use super::ParticleStyle;
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use storm_formats::particles::Graph;
use storm_sim::particle_emitter::{Birth, BirthPort, Emitter, Kind, ParticleType};

pub const FRAME_SECONDS: f64 = 1.0 / 60.0;
#[derive(Clone, Debug)]
struct Config {
    lifetime: f32,
    looping: bool,
    count: usize,
    rate: Graph,
    particle_lifetime: Graph,
}
impl Config {
    fn matches(&self, style: &ParticleStyle) -> bool {
        self.lifetime.to_bits() == style.emitter_lifetime.to_bits()
            && self.looping == style.looping
            && self.count == style.count
            && self.rate == style.rate
            && self.particle_lifetime == style.lifetime
    }
}
#[derive(Clone, Debug)]
pub(super) struct LiveParticle {
    pub serial: u64,
    pub age: f32,
    pub lifetime: f32,
    pub random: [f32; 7],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BirthStats {
    pub created: u64,
    pub living: usize,
    pub clock_bits: u32,
    pub remainder_bits: u32,
}
#[derive(Clone, Debug)]
struct Run {
    config: Config,
    start: u32,
    stop: Option<u32>,
    ticks: u32,
    emitter: Emitter,
    rng: u32,
    created: u64,
    particles: Vec<LiveParticle>,
}
impl Run {
    fn new(style: &ParticleStyle, start: f32, stop: Option<f32>, seed: usize) -> Self {
        // This explicit portable stream is an embedding policy, not libc rand.
        let rng = style.source.bytes().fold(2166136261u32, |a, b| {
            (a ^ u32::from(b)).wrapping_mul(16777619)
        }) ^ (seed as u32).wrapping_mul(0x9e3779b9);
        Self {
            config: Config {
                lifetime: style.emitter_lifetime,
                looping: style.looping,
                count: style.count,
                rate: style.rate.clone(),
                particle_lifetime: style.lifetime.clone(),
            },
            start: start.to_bits(),
            stop: stop.map(f32::to_bits),
            ticks: 0,
            emitter: Emitter::new(
                style.emitter_lifetime,
                style.looping,
                vec![ParticleType::new(Kind::Billboard, style.count as u32)],
            ),
            rng,
            created: 0,
            particles: vec![],
        }
    }
    fn process(&mut self, delta: f32, paused: bool) -> Result<()> {
        if !paused {
            let mut port = Port {
                config: &self.config,
                rng: &mut self.rng,
                created: &mut self.created,
                particles: &mut self.particles,
            };
            self.emitter.execute(delta, &mut port)?;
        }
        let mut index = 0;
        while index < self.particles.len() {
            self.particles[index].age += delta;
            if self.particles[index].age > self.particles[index].lifetime {
                self.particles.swap_remove(index);
                self.emitter.release(0)?;
            } else {
                index += 1;
            }
        }
        Ok(())
    }
    fn frame(&mut self, wall: f64, delta: f64) -> Result<()> {
        let stop = self
            .stop
            .map(|s| f64::from(f32::from_bits(s)) - f64::from(f32::from_bits(self.start)));
        if let Some(stop) = stop {
            if stop <= wall {
                return self.process(delta as f32, true);
            }
            if stop < wall + delta {
                self.process((stop - wall) as f32, false)?;
                return self.process((wall + delta - stop) as f32, true);
            }
        }
        self.process(delta as f32, false)
    }
    fn stats(&self) -> BirthStats {
        BirthStats {
            created: self.created,
            living: self.particles.len(),
            clock_bits: self.emitter.elapsed.to_bits(),
            remainder_bits: self.emitter.types[0].remainder.to_bits(),
        }
    }
}
struct Port<'a> {
    config: &'a Config,
    rng: &'a mut u32,
    created: &'a mut u64,
    particles: &'a mut Vec<LiveParticle>,
}
impl Port<'_> {
    fn next(&mut self) -> u32 {
        *self.rng = self.rng.wrapping_mul(1664525).wrapping_add(1013904223);
        *self.rng >> 1
    }
}
impl BirthPort for Port<'_> {
    fn rate(&mut self, _: usize, time: f32, life: f32) -> f32 {
        let raw = self.next();
        self.config.rate.sample_range(time, life, raw)
    }
    fn birth(&mut self, birth: Birth) -> bool {
        // Consume direction(3), initial track(3), lifetime/mass/spin/velocity(4),
        // then 12 FRAND factors in the retained billboard initialization order.
        for _ in 0..6 {
            self.next();
        }
        let raw = self.next();
        let lifetime = self.config.particle_lifetime.sample_range(
            birth.emitter_time,
            birth.emitter_lifetime,
            raw,
        );
        for _ in 0..3 {
            self.next();
        }
        let factors: [f32; 12] = std::array::from_fn(|_| self.next() as f32 / i32::MAX as f32);
        if !lifetime.is_finite() {
            return false;
        }
        *self.created += 1;
        self.particles.push(LiveParticle {
            serial: *self.created,
            age: 0.0,
            lifetime,
            random: [
                factors[2],
                factors[3],
                factors[4],
                factors[5],
                factors[9],
                factors[10],
                factors[11],
            ],
        });
        true
    }
}
#[derive(Debug, Default)]
pub(super) struct Replay {
    runs: BTreeMap<usize, Run>,
}
impl Replay {
    pub fn snapshot(
        &mut self,
        style: &ParticleStyle,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        seed: usize,
    ) -> Result<(Vec<LiveParticle>, BirthStats)> {
        ensure!(
            seconds.is_finite()
                && start.is_finite()
                && style.emitter_lifetime.is_finite()
                && style.emitter_lifetime > 0.0
                && (1..=2048).contains(&style.count),
            "invalid particle replay input"
        );
        ensure!(
            stop.is_none_or(|s| s.is_finite() && s >= start),
            "invalid particle emission stop time"
        );
        ensure!(seed < 16_384, "particle replay locator budget exceeded");
        let elapsed = (f64::from(seconds) - f64::from(start)).max(0.0);
        ensure!(elapsed <= 7200.0, "particle replay exceeds 7200 seconds");
        let ticks = (elapsed / FRAME_SECONDS).floor() as u32;
        let reset = self.runs.get(&seed).is_none_or(|r| {
            r.start != start.to_bits()
                || r.stop != stop.map(f32::to_bits)
                || r.ticks > ticks
                || !r.config.matches(style)
        });
        if reset {
            self.runs.insert(seed, Run::new(style, start, stop, seed));
        }
        let run = self.runs.get_mut(&seed).unwrap();
        while run.ticks < ticks {
            let wall = f64::from(run.ticks) * FRAME_SECONDS;
            if run.particles.is_empty()
                && stop.is_some_and(|s| f64::from(s) - f64::from(start) <= wall)
            {
                run.ticks = ticks;
                break;
            }
            run.frame(wall, FRAME_SECONDS)?;
            run.ticks += 1;
        }
        // Only complete fixed frames create births. Fractional presentation ages a
        // copy of existing particles, preventing redraws from inventing births.
        let mut presented = run.clone();
        let remainder = (elapsed - f64::from(ticks) * FRAME_SECONDS).max(0.0);
        if remainder > 0.0 {
            presented.process(remainder as f32, true)?;
        }
        let stats = presented.stats();
        Ok((presented.particles, stats))
    }
}
