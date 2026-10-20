//! Fixed-frame replay adapter around the retained native birth accumulator.
use super::WorldEmitter;
use super::{ParticleStyle, particle_style::MotionProfile};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use storm_formats::particles::Graph;
use storm_sim::particle_emitter::{Birth, BirthPort, Emitter, Kind, ParticleType};
use storm_sim::particle_motion::{Billboard, Curve, MotionPort, Transform};
use storm_sim::particle_transform::EmitterTransform;

pub const FRAME_SECONDS: f64 = 1.0 / 60.0;
#[derive(Clone, Debug)]
struct Config {
    lifetime: f32,
    looping: bool,
    count: usize,
    rate: Graph,
    particle_lifetime: Graph,
    motion: MotionProfile,
    track: [Graph; 3],
    position: [f32; 3],
}
impl Config {
    fn matches(&self, style: &ParticleStyle) -> bool {
        self.lifetime.to_bits() == style.emitter_lifetime.to_bits()
            && self.looping == style.looping
            && self.count == style.count
            && self.rate == style.rate
            && self.particle_lifetime == style.lifetime
            && self.motion == style.motion
            && self.track == style.track
            && self.position
                == [
                    style.emitter_position.x,
                    style.emitter_position.y,
                    -style.emitter_position.z,
                ]
    }
}
#[derive(Clone, Debug)]
pub(super) struct LiveParticle {
    pub serial: u64,
    pub body: Billboard,
    pub random: [f32; 12],
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BirthStats {
    pub created: u64,
    pub living: usize,
    pub clock_bits: u32,
    pub remainder_bits: u32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MotionStats {
    pub living: usize,
    pub moving: usize,
    pub spinning: usize,
    pub position_bounds: Option<[[f32; 3]; 2]>,
    pub angle_bounds: Option<[f32; 2]>,
    pub maximum_speed: f64,
}
impl MotionStats {
    pub(super) fn of(particles: &[LiveParticle]) -> Self {
        let mut result = Self {
            living: particles.len(),
            moving: 0,
            spinning: 0,
            position_bounds: None,
            angle_bounds: None,
            maximum_speed: 0.0,
        };
        for p in particles {
            let b = &p.body;
            result.moving += usize::from(b.velocity.iter().any(|v| *v != 0.0));
            result.spinning += usize::from(b.render_angle != 0.0);
            result.maximum_speed = result.maximum_speed.max(
                b.velocity
                    .into_iter()
                    .map(|v| f64::from(v).powi(2))
                    .sum::<f64>()
                    .sqrt(),
            );
            let position = [
                b.render_position[0],
                b.render_position[1],
                -b.render_position[2],
            ];
            let bounds = result.position_bounds.get_or_insert([position; 2]);
            for (i, v) in position.into_iter().enumerate() {
                bounds[0][i] = bounds[0][i].min(v);
                bounds[1][i] = bounds[1][i].max(v);
            }
            let angles = result.angle_bounds.get_or_insert([b.render_angle; 2]);
            angles[0] = angles[0].min(b.render_angle);
            angles[1] = angles[1].max(b.render_angle);
        }
        result
    }
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
    world: Option<WorldEmitter>,
    transform: EmitterTransform,
}
impl Run {
    fn new(
        style: &ParticleStyle,
        start: f32,
        stop: Option<f32>,
        seed: usize,
        world: Option<WorldEmitter>,
    ) -> Self {
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
                motion: style.motion.clone(),
                track: style.track.clone(),
                position: [
                    style.emitter_position.x,
                    style.emitter_position.y,
                    -style.emitter_position.z,
                ],
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
            world,
            transform: EmitterTransform::default(),
        }
    }
    fn process(&mut self, wall: f64, delta: f64, paused: bool) -> Result<()> {
        if !paused {
            if let Some(world) = &self.world {
                let start = f64::from(f32::from_bits(self.start));
                if self.transform.old.is_none() {
                    self.transform.set(world.at(start + wall)?);
                }
                self.transform.set(world.at(start + wall + delta)?);
            }
            let mut port = Port {
                config: &self.config,
                rng: &mut self.rng,
                created: &mut self.created,
                particles: &mut self.particles,
                transform: self.world.as_ref().map(|_| &mut self.transform),
                error: None,
            };
            self.emitter.execute(delta as f32, &mut port)?;
            if let Some(error) = port.error {
                return Err(error);
            }
        }
        let mut index = 0;
        while index < self.particles.len() {
            let particle = &mut self.particles[index];
            let alive = particle.body.process(
                delta as f32,
                &mut Motion {
                    config: &self.config,
                    random: &particle.random,
                },
            )?;
            if !alive {
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
                return self.process(wall, delta, true);
            }
            if stop < wall + delta {
                self.process(wall, stop - wall, false)?;
                return self.process(stop, wall + delta - stop, true);
            }
        }
        self.process(wall, delta, false)
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
    transform: Option<&'a mut EmitterTransform>,
    error: Option<anyhow::Error>,
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
        // Keep the preceding stream order: direction(3), initial track(3),
        // lifetime/mass/spin/velocity(4), then 12 persistent FRAND factors.
        let direction = std::array::from_fn::<_, 3, _>(|i| {
            let raw = self.next();
            self.config.motion.direction[i].sample_range(
                birth.emitter_time,
                birth.emitter_lifetime,
                raw,
            ) * (std::f32::consts::PI / 180.0)
        });
        let offset = std::array::from_fn::<_, 3, _>(|i| {
            let raw = self.next();
            self.config.track[i].sample_range(0.0, 100.0, raw)
        });
        let raw = self.next();
        let lifetime = self.config.particle_lifetime.sample_range(
            birth.emitter_time,
            birth.emitter_lifetime,
            raw,
        );
        let raw = self.next();
        let mass =
            self.config
                .motion
                .mass
                .sample_range(birth.emitter_time, birth.emitter_lifetime, raw);
        let raw = self.next();
        let spin =
            self.config
                .motion
                .spin
                .sample_range(birth.emitter_time, birth.emitter_lifetime, raw)
                * (std::f32::consts::PI / 180.0);
        let raw = self.next();
        let power = self.config.motion.velocity.sample_range(
            birth.emitter_time,
            birth.emitter_lifetime,
            raw,
        );
        let factors: [f32; 12] = std::array::from_fn(|_| self.next() as f32 / i32::MAX as f32);
        // Native Matrix::Build rotation order rz * rx * ry, taking its Y axis.
        let (sx, cx) = direction[0].sin_cos();
        let (sy, cy) = direction[1].sin_cos();
        let (sz, cz) = direction[2].sin_cos();
        let mut velocity = [-sz * cy + cz * sx * sy, cz * cx, -sz * -sy + cz * sx * cy];
        let mut transform = Transform::default();
        transform.0[3] = self.config.position;
        if let Some(world) = &self.transform {
            match world.birth(birth.matrix_blend, self.config.position) {
                Ok(value) => {
                    transform = value;
                    velocity = transform.normal(velocity);
                }
                Err(error) => {
                    self.error = Some(error);
                    return false;
                }
            }
        }
        // Native AddParticle rotates the direction BEFORE scaling by Velocity.
        velocity = velocity.map(|v| v * power);
        let position = transform.point(offset);
        if ![lifetime, mass, spin]
            .into_iter()
            .chain(position)
            .chain(velocity)
            .all(f32::is_finite)
        {
            return false;
        }
        *self.created += 1;
        self.particles.push(LiveParticle {
            serial: *self.created,
            body: Billboard::new(position, velocity, lifetime, mass, spin, transform),
            random: factors,
        });
        true
    }
    fn finish(&mut self) {
        if let Some(transform) = &mut self.transform {
            transform.finish();
        }
    }
}
struct Motion<'a> {
    config: &'a Config,
    random: &'a [f32; 12],
}
impl MotionPort for Motion<'_> {
    fn sample(&mut self, curve: Curve, age: f32, life: f32) -> f32 {
        let (graph, index) = match curve {
            Curve::Drag => (&self.config.motion.drag, 0),
            Curve::Gravity => (&self.config.motion.gravity, 6),
            Curve::SpinDrag => (&self.config.motion.spin_drag, 1),
            Curve::TrackX => (&self.config.track[0], 9),
            Curve::TrackY => (&self.config.track[1], 10),
            Curve::TrackZ => (&self.config.track[2], 11),
            Curve::Blend => (&self.config.motion.blend, 8),
        };
        graph.sample(age, life, self.random[index])
    }
}
#[derive(Debug, Default)]
pub(super) struct Replay {
    runs: BTreeMap<usize, Run>,
}
impl Replay {
    #[allow(clippy::too_many_arguments)]
    pub fn snapshot(
        &mut self,
        style: &ParticleStyle,
        seconds: f32,
        start: f32,
        stop: Option<f32>,
        seed: usize,
        world: Option<WorldEmitter>,
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
        if let Some(world) = &world
            && seconds >= start
        {
            let through = stop.map_or(seconds, |s| s.min(seconds));
            ensure!(
                world
                    .history
                    .read()?
                    .last()
                    .is_some_and(|s| s.seconds as f32 >= through),
                "pose history must cover the particle birth interval"
            );
        }
        let elapsed = (f64::from(seconds) - f64::from(start)).max(0.0);
        ensure!(elapsed <= 7200.0, "particle replay exceeds 7200 seconds");
        let ticks = (elapsed / FRAME_SECONDS).floor() as u32;
        let reset = self.runs.get(&seed).is_none_or(|r| {
            r.start != start.to_bits()
                || r.stop != stop.map(f32::to_bits)
                || r.ticks > ticks
                || !r.config.matches(style)
                || match (&r.world, &world) {
                    (None, None) => false,
                    (Some(a), Some(b)) => !a.same(b),
                    _ => true,
                }
        });
        if reset {
            self.runs
                .insert(seed, Run::new(style, start, stop, seed, world));
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
            presented.process(f64::from(ticks) * FRAME_SECONDS, remainder, true)?;
        }
        let stats = presented.stats();
        Ok((presented.particles, stats))
    }
}
