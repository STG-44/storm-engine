//! The original five-sided ROPE sections, texture coordinates and sag.
use anyhow::{Result, ensure};
use glam::Vec3;

const SIDES: usize = 5;

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub segment_length: f32,
    pub width: f32,
    pub stay_width: f32,
    pub end_length: f32,
    pub texture_length: f32,
    pub wave: f32,
    pub depth_speed: f32,
    pub rotation_speed: f32,
    pub minimum_depth: f32,
    pub depth_variation: f32,
    pub rotation_angle: f32,
    pub max_distance: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            segment_length: 2.0,
            width: 0.025,
            stay_width: 0.12,
            end_length: 0.05,
            texture_length: 0.5,
            wave: 0.1,
            depth_speed: 0.15,
            rotation_speed: 0.08,
            minimum_depth: 1.0,
            depth_variation: 0.7,
            rotation_angle: 0.1,
            max_distance: 5000.0,
        }
    }
}
impl Settings {
    pub fn validate(self) -> Result<()> {
        ensure!(
            (0.01..=200.0).contains(&self.segment_length)
                && self.width > 0.0
                && self.stay_width > 0.0
                && self.texture_length > 0.0
                && self.end_length >= 0.0
                && self.max_distance >= 0.0
                && [
                    self.width,
                    self.stay_width,
                    self.texture_length,
                    self.end_length,
                    self.wave,
                    self.depth_speed,
                    self.rotation_speed,
                    self.minimum_depth,
                    self.depth_variation,
                    self.rotation_angle,
                    self.max_distance,
                ]
                .into_iter()
                .all(f32::is_finite),
            "invalid [ROPES] settings"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Phase {
    depth: f32,
    rotation: f32,
}
impl Phase {
    pub fn advance(&mut self, settings: Settings, delta_ms: u32) {
        let elapsed = delta_ms as f32 * 0.02;
        advance_angle(&mut self.depth, settings.depth_speed * elapsed);
        advance_angle(&mut self.rotation, settings.rotation_speed * elapsed);
    }
}

fn advance_angle(angle: &mut f32, step: f32) {
    *angle += step;
    if *angle > std::f32::consts::TAU {
        *angle = 0.0;
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Pose {
    pub ends: [[f32; 3]; 2],
    pub phase: Phase,
}
impl Pose {
    pub fn is_finite(self) -> bool {
        self.ends
            .into_iter()
            .flatten()
            .chain([self.phase.depth, self.phase.rotation])
            .all(f32::is_finite)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Vertex {
    pub position: Vec3,
    pub normal: Vec3,
    pub uv: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct Geometry {
    settings: Settings,
    segments: usize,
    inset: Vec3,
    offsets: [Vec3; SIDES],
    length: f32,
    wave: f32,
}
impl Geometry {
    pub fn new(
        ends: [[f32; 3]; 2],
        stay: bool,
        sag: bool,
        settings: Settings,
    ) -> Result<Option<Self>> {
        settings.validate()?;
        let [begin, end] = ends.map(Vec3::from_array);
        let delta = end - begin;
        let length = delta.length();
        if !(0.1..=200.0).contains(&length) {
            return Ok(None);
        }
        let segments = (length / settings.segment_length) as usize + 1;
        let radius = if stay {
            settings.stay_width
        } else {
            settings.width
        } * 0.5;
        let horizontal = (delta.x * delta.x + delta.z * delta.z).sqrt();
        let offsets = std::array::from_fn(|i| {
            let angle = i as f32 / SIDES as f32 * 2.0 * std::f32::consts::PI;
            let (s, c) = (angle.sin(), angle.cos());
            if horizontal < 0.0001 {
                return radius * Vec3::new(s, 0.0, c);
            }
            let (hc, hs) = (delta.z / horizontal, delta.x / horizontal);
            let (vc, vs) = (delta.z / length, delta.y / length);
            radius * Vec3::new(s * hc + c * vs * hs, c * vc, c * vs * hc - s * hs)
        });
        let scale = if segments <= 2 { 0.2 } else { 1.0 };
        Ok(Some(Self {
            settings,
            segments,
            inset: delta.normalize() * settings.end_length,
            offsets,
            length,
            wave: if sag { -settings.wave * scale } else { 0.0 },
        }))
    }

    pub fn vertex_count(&self) -> usize {
        (self.segments + 1) * SIDES + 2
    }

    pub fn index_count(&self) -> usize {
        (self.segments + 1) * SIDES * 6
    }

    pub fn visible_from(&self, distance_squared: f32, perspective: f32) -> bool {
        distance_squared * (perspective * 0.5).tan() < self.settings.max_distance
    }

    pub fn indices(&self) -> impl Iterator<Item = u32> + '_ {
        let sides = SIDES as u32;
        let last_ring = self.segments as u32 * sides + 1;
        let start = (0..sides).flat_map(move |i| [0, 1 + i, 1 + (i + 1) % sides]);
        let middle = (0..self.segments as u32).flat_map(move |row| {
            (0..sides).flat_map(move |i| {
                let a = 1 + row * sides + i;
                let b = 1 + row * sides + (i + 1) % sides;
                [a, a + sides, b, b, a + sides, b + sides]
            })
        });
        // SetIndex uses the second vertex of the final ring as the cap's hub.
        let end = (0..sides)
            .flat_map(move |i| [last_ring + i, last_ring + 1, last_ring + (i + 1) % sides]);
        start.chain(middle).chain(end)
    }

    pub fn vertices(&self, pose: Pose) -> impl Iterator<Item = Vertex> + '_ {
        let [begin, end] = pose.ends.map(Vec3::from_array);
        let s = self.settings;
        let depth = self.wave * (s.minimum_depth + s.depth_variation * pose.phase.depth.sin());
        let sag = Vec3::new(
            depth * (s.rotation_angle * pose.phase.rotation.sin()).sin(),
            depth,
            0.0,
        );
        let step = (end - (begin + self.inset) - self.inset) / self.segments as f32;
        let initial_v = s.end_length / s.texture_length;
        let step_v = (self.length / s.texture_length - initial_v) / self.segments as f32;
        let curve = sag * (4.0 / (self.segments * self.segments) as f32);
        let mut center = begin + self.inset;
        let mut v = initial_v;
        let rings = (0..=self.segments).flat_map(move |row| {
            let p = center + curve * (row * (self.segments - row)) as f32;
            let t = v;
            center += step;
            v += step_v;
            self.offsets
                .iter()
                .enumerate()
                .map(move |(side, offset)| Vertex {
                    position: p + offset,
                    normal: offset.normalize_or_zero(),
                    uv: [
                        if side <= 2 {
                            side as f32 / 2.0
                        } else {
                            1.0 - (side - 2) as f32 / 3.0
                        },
                        t,
                    ],
                })
        });
        std::iter::once(Vertex {
            position: begin,
            normal: Vec3::Y,
            uv: [0.0, 0.0],
        })
        .chain(rings)
        .chain(std::iter::once(Vertex {
            position: end,
            normal: Vec3::Y,
            uv: [1.0, self.length],
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_sections_keep_endpoints_uvs_and_cross_section_during_sag() -> Result<()> {
        let settings = Settings::default();
        let ends = [[0.0, 0.0, 0.0], [0.0, 0.0, 4.0]];
        let rope = Geometry::new(ends, false, true, settings)?.unwrap();
        let stay = Geometry::new(ends, true, false, settings)?.unwrap();
        assert_eq!(rope.vertex_count(), 22);
        assert_eq!(rope.index_count(), 120);
        let indices = rope.indices().collect::<Vec<_>>();
        assert_eq!(indices.len(), rope.index_count());
        assert_eq!(&indices[..6], &[0, 1, 2, 0, 2, 3]);
        assert!(indices.iter().all(|&i| (i as usize) < rope.vertex_count()));
        let mut phase = Phase::default();
        let initial = rope.vertices(Pose { ends, phase }).collect::<Vec<_>>();
        phase.advance(settings, 250);
        let moved = rope.vertices(Pose { ends, phase }).collect::<Vec<_>>();
        let straight = stay.vertices(Pose { ends, phase }).collect::<Vec<_>>();
        assert_eq!(initial[0].position, moved[0].position);
        assert_eq!(initial[21].position, moved[21].position);
        assert_eq!(moved[21].position, Vec3::Z * 4.0);
        assert!(moved[6].position.y < initial[6].position.y);
        assert!(straight[6].position.y > initial[6].position.y);
        assert_eq!(moved[1].uv, [0.0, 0.1]);
        assert_eq!(moved[21].uv, [1.0, 4.0]);
        assert!(rope.visible_from(100.0, 1.0));
        assert!(!rope.visible_from(10_000.0, 1.0));
        Ok(())
    }
}
