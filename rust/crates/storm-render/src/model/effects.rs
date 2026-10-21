//! Deterministic billboard preview attached to original fireplace locators.
use super::{
    ModelVertex, SailWind,
    crew::CrewPatch,
    ship::{FirePoint, ShipAsset, ShipState},
};
use anyhow::{Result, ensure};
use glam::Vec3;
use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct ParticlePatch {
    pub points: Arc<Vec<FirePoint>>,
    pub smoke: bool,
    pub per_point: usize,
    pub lifetime: f32,
    pub style: Option<Arc<super::ParticleStyle>>,
}
#[derive(Clone, Debug)]
pub enum DynamicMesh {
    Crew(CrewPatch),
    Particles(ParticlePatch),
}
impl DynamicMesh {
    pub fn frame_vertex_count(&self, state: &ShipState, seconds: f32) -> Result<usize> {
        match self {
            Self::Crew(_) => Ok(self.vertex_count()),
            Self::Particles(p) => p.frame_vertex_count(state, seconds),
        }
    }
    pub fn vertex_count(&self) -> usize {
        match self {
            Self::Crew(c) => c.source.len(),
            Self::Particles(p) => p.points.len() * p.per_point * 4,
        }
    }
    pub fn particle(&self) -> bool {
        matches!(self, Self::Particles(_))
    }
    pub fn visible(&self, state: &ShipState, seconds: f32) -> bool {
        match self {
            Self::Crew(_) => state.crew_visible,
            Self::Particles(patch) => state.native_fire.as_ref().map_or_else(
                || patch.manual_preview() && state.fire_started.is_some_and(|t| seconds >= t),
                |_| {
                    patch
                        .matching_emissions(state)
                        .any(|e| patch.alive(e, seconds))
                },
            ),
        }
    }
    #[allow(clippy::too_many_arguments)]
    pub fn deform(
        &self,
        out: &mut [ModelVertex],
        seconds: f32,
        wind: SailWind,
        eye: Vec3,
        asset: &ShipAsset,
        state: &ShipState,
    ) -> Result<()> {
        ensure!(
            seconds.is_finite()
                && eye.is_finite()
                && out.len() == self.frame_vertex_count(state, seconds)?,
            "invalid dynamic mesh frame"
        );
        match self {
            Self::Crew(c) => c.deform(out, seconds)?,
            Self::Particles(p) => p.deform(out, seconds, wind, eye, asset, state, None)?,
        }
        ensure!(
            out.iter().all(|v| v
                .position
                .iter()
                .chain(v.normal.iter())
                .chain(v.uv.iter())
                .chain(v.color.iter())
                .all(|f| f.is_finite())),
            "non-finite dynamic vertex"
        );
        Ok(())
    }
}
impl ParticlePatch {
    fn alive(&self, e: &super::FireEmission, seconds: f32) -> bool {
        seconds >= e.started && e.stopped.is_none_or(|stop| seconds < stop + self.lifetime)
    }
    pub fn frame_vertex_count(&self, state: &ShipState, seconds: f32) -> Result<usize> {
        let slots = if state.native_fire.is_some() {
            self.matching_emissions(state)
                .filter(|e| self.alive(e, seconds))
                .count()
        } else {
            self.points.len()
        };
        let count = slots
            .checked_mul(self.per_point)
            .and_then(|v| v.checked_mul(4))
            .ok_or_else(|| anyhow::anyhow!("particle vertex count overflow"))?;
        ensure!(
            count <= super::MAX_PARTICLE_VERTICES,
            "particle frame exceeds vertex budget"
        );
        Ok(count)
    }
    fn matching_emissions<'a>(
        &'a self,
        state: &'a ShipState,
    ) -> impl Iterator<Item = &'a super::FireEmission> {
        let key = self
            .style
            .as_ref()
            .and_then(|s| storm_formats::particles::profile_key(&s.source).ok());
        state.native_fire.iter().flatten().filter(move |e| {
            e.point < self.points.len()
                && match &e.profile {
                    Some(name) => key.as_ref() == Some(name),
                    None => e.smoke == self.smoke,
                }
        })
    }
    pub fn manual_preview(&self) -> bool {
        self.style.as_ref().is_none_or(|style| style.preview)
    }
    pub fn matches(&self, emission: &super::FireEmission) -> bool {
        match &emission.profile {
            Some(key) => self.style.as_ref().is_some_and(|s| {
                storm_formats::particles::profile_key(&s.source).is_ok_and(|source| source == *key)
            }),
            None => emission.smoke == self.smoke,
        }
    }
    pub fn emissions<'a>(
        &'a self,
        state: &'a ShipState,
        locator: usize,
    ) -> impl Iterator<Item = &'a super::FireEmission> {
        self.matching_emissions(state)
            .filter(move |e| e.point == locator)
    }
    fn activations<'a>(
        &'a self,
        state: &'a ShipState,
        locator: usize,
        seconds: f32,
    ) -> Vec<Option<&'a super::FireEmission>> {
        if state.native_fire.is_some() {
            self.emissions(state, locator)
                .filter(|e| {
                    seconds >= e.started
                        && e.stopped.is_none_or(|stop| seconds < stop + self.lifetime)
                })
                .map(Some)
                .collect()
        } else {
            vec![None]
        }
    }
    pub fn indices(&self) -> Vec<u32> {
        Self::quad_indices(self.points.len() * self.per_point)
    }
    pub(super) fn quad_indices(quads: usize) -> Vec<u32> {
        (0..quads)
            .flat_map(|i| {
                let a = (i * 4) as u32;
                [a, a + 1, a + 2, a, a + 2, a + 3]
            })
            .collect()
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn deform(
        &self,
        out: &mut [ModelVertex],
        seconds: f32,
        wind: SailWind,
        eye: Vec3,
        asset: &ShipAsset,
        state: &ShipState,
        history: Option<&Arc<storm_sim::pose_history::PoseHistory>>,
    ) -> Result<()> {
        let expected = self.frame_vertex_count(state, seconds)?;
        ensure!(
            out.len() == expected,
            "particle frame vertex count mismatch"
        );
        if let Some(style) = &self.style {
            let instances = state.native_fire.as_ref().map_or_else(
                || std::collections::BTreeSet::from([0]),
                |_| {
                    self.matching_emissions(state)
                        .filter(|e| self.alive(e, seconds))
                        .map(|e| e.instance)
                        .collect()
                },
            );
            style.retain_instances(&instances)?;
        }
        let air = Vec3::new(wind.direction.sin(), 0.0, wind.direction.cos()) * wind.speed;
        let mut particles = Vec::with_capacity(self.points.len() * self.per_point);
        for (emitter, point) in self.points.iter().enumerate() {
            for emission in self.activations(state, emitter, seconds) {
                let start =
                    emission.map_or_else(|| state.fire_started.unwrap_or(seconds), |e| e.started);
                let elapsed = (seconds - start).max(0.0);
                let transform = asset
                    .transform(&point.node, state, seconds)
                    .unwrap_or_default();
                let source = transform.transform_point3(point.position);
                let samples = if let Some(style) = &self.style {
                    if emission.is_some()
                        || (state.native_fire.is_none()
                            && self.manual_preview()
                            && state.fire_started.is_some())
                    {
                        Some(
                            style
                                .instance_snapshot(
                                    seconds,
                                    start,
                                    emission.and_then(|e| e.stopped),
                                    super::ParticleStream {
                                        locator: emitter,
                                        instance: emission.map_or(0, |e| e.instance),
                                    },
                                    history.map(|history| super::WorldEmitter {
                                        history: history.clone(),
                                        point: point.position.to_array(),
                                    }),
                                )?
                                .0,
                        )
                    } else {
                        Some(vec![None; self.per_point])
                    }
                } else {
                    None
                };
                for i in 0..self.per_point {
                    let phase = i as f32 / self.per_point as f32;
                    let age = (elapsed + phase * self.lifetime).rem_euclid(self.lifetime);
                    let life = age / self.lifetime;
                    let seed = (i + emitter * 31) as f32;
                    let offset = Vec3::new((seed * 2.399).sin(), 0.0, (seed * 3.137).cos())
                        * (if self.smoke { 0.5 } else { 0.3 });
                    let center = source
                        + offset
                        + Vec3::Y * age * (if self.smoke { 1.4 } else { 0.8 })
                        + air * age * if self.smoke { 0.1 } else { 0.018 };
                    let radius = if self.smoke {
                        0.4 + life * 2.6
                    } else {
                        0.18 + life * 0.85
                    };
                    let birth = seconds - age;
                    let native_hidden = state.native_fire.is_some()
                        && emission.is_none_or(|e| {
                            seconds < e.started
                                || birth < e.started
                                || e.stopped.is_some_and(|stop| birth > stop)
                        });
                    let alpha = if native_hidden || asset.hidden(&point.node, state, seconds) {
                        0.0
                    } else {
                        (if self.smoke { 0.42 } else { 0.8 })
                            * (1.0 - life)
                            * ((life * 8.0).min(1.0))
                    };
                    let (center, radius, color, uv, angle) = if let Some(samples) = &samples {
                        match samples[i].as_ref() {
                            Some(sample) => {
                                let mut color = sample.color;
                                if (state.native_fire.is_some()
                                    && emission.is_none_or(|e| seconds < e.started))
                                    || (history.is_none()
                                        && asset.hidden(&point.node, state, seconds))
                                {
                                    color[3] = 0.0;
                                }
                                (
                                    if history.is_some() {
                                        sample.offset
                                    } else {
                                        source + sample.offset
                                    },
                                    sample.radius,
                                    color,
                                    sample.uv,
                                    sample.angle,
                                )
                            }
                            None => (source, 0.0, [0.0; 4], [0.0; 4], 0.0),
                        }
                    } else {
                        (
                            center,
                            radius,
                            if self.smoke {
                                [0.23, 0.24, 0.25, alpha]
                            } else {
                                [1.0, 0.9, 0.8, alpha]
                            },
                            [0.0, 0.0, 1.0, 1.0],
                            0.0,
                        )
                    };
                    particles.push((center, radius, color, uv, angle));
                }
            }
        }
        particles.sort_by(|a, b| {
            b.0.distance_squared(eye)
                .total_cmp(&a.0.distance_squared(eye))
        });
        for ((center, radius, color, uv, angle), quad) in
            particles.into_iter().zip(out.chunks_exact_mut(4))
        {
            let normal = (eye - center).normalize_or_zero();
            let mut right = Vec3::Y.cross(normal).normalize_or_zero();
            if right.length_squared() < 0.5 {
                right = Vec3::X;
            }
            let up = normal.cross(right).normalize_or_zero();
            let (sin, cos) = angle.sin_cos();
            for ((x, y, u, v), vertex) in [
                (-1.0, -1.0, 0.0, 1.0),
                (1.0, -1.0, 1.0, 1.0),
                (1.0, 1.0, 1.0, 0.0),
                (-1.0, 1.0, 0.0, 0.0),
            ]
            .into_iter()
            .zip(quad)
            {
                *vertex = ModelVertex {
                    position: (center
                        + (right * (x * cos - y * sin) + up * (x * sin + y * cos)) * radius)
                        .to_array(),
                    normal: normal.to_array(),
                    uv: [uv[0] + (uv[2] - uv[0]) * u, uv[1] + (uv[3] - uv[1]) * v],
                    color,
                };
            }
        }
        ensure!(
            out.iter().all(|v| v
                .position
                .iter()
                .chain(&v.normal)
                .chain(&v.uv)
                .chain(&v.color)
                .all(|f| f.is_finite())),
            "non-finite particle vertex"
        );
        Ok(())
    }
}
