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
                || state.fire_started.is_some_and(|t| seconds >= t),
                |emissions| {
                    emissions.iter().any(|e| {
                        e.smoke == patch.smoke
                            && seconds >= e.started
                            && e.stopped.is_none_or(|stop| seconds < stop + patch.lifetime)
                    })
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
            seconds.is_finite() && eye.is_finite() && out.len() == self.vertex_count(),
            "invalid dynamic mesh frame"
        );
        match self {
            Self::Crew(c) => c.deform(out, seconds)?,
            Self::Particles(p) => p.deform(out, seconds, wind, eye, asset, state)?,
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
    pub fn indices(&self) -> Vec<u32> {
        (0..self.points.len() * self.per_point)
            .flat_map(|i| {
                let a = (i * 4) as u32;
                [a, a + 1, a + 2, a, a + 2, a + 3]
            })
            .collect()
    }
    fn deform(
        &self,
        out: &mut [ModelVertex],
        seconds: f32,
        wind: SailWind,
        eye: Vec3,
        asset: &ShipAsset,
        state: &ShipState,
    ) -> Result<()> {
        let air = Vec3::new(wind.direction.sin(), 0.0, wind.direction.cos()) * wind.speed;
        let mut particles = Vec::with_capacity(self.points.len() * self.per_point);
        for (emitter, point) in self.points.iter().enumerate() {
            let emission = state.native_fire.as_ref().and_then(|emissions| {
                emissions
                    .iter()
                    .filter(|e| e.point == emitter && e.smoke == self.smoke)
                    .max_by(|a, b| a.started.total_cmp(&b.started))
            });
            let start =
                emission.map_or_else(|| state.fire_started.unwrap_or(seconds), |e| e.started);
            let elapsed = (seconds - start).max(0.0);
            let transform = asset
                .transform(&point.node, state, seconds)
                .unwrap_or_default();
            let source = transform.transform_point3(point.position);
            let samples = if let Some(style) = &self.style {
                if emission.is_some()
                    || (state.native_fire.is_none() && state.fire_started.is_some())
                {
                    Some(style.samples(
                        seconds,
                        start,
                        emission.and_then(|e| e.stopped),
                        emitter,
                    )?)
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
                    (if self.smoke { 0.42 } else { 0.8 }) * (1.0 - life) * ((life * 8.0).min(1.0))
                };
                let (center, radius, color, uv, angle) = if let Some(samples) = &samples {
                    match samples[i].as_ref() {
                        Some(sample) => {
                            let mut color = sample.color;
                            if (state.native_fire.is_some()
                                && emission.is_none_or(|e| seconds < e.started))
                                || asset.hidden(&point.node, state, seconds)
                            {
                                color[3] = 0.0;
                            }
                            (
                                source + sample.offset,
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
        Ok(())
    }
}
