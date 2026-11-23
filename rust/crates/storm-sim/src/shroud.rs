//! VANT_BASE::SetAdd, SetVertexes and SetIndex in native model coordinates.
use anyhow::{Result, ensure};
use glam::Vec3;

const EDGES: usize = 5;
const RING: usize = EDGES + 1;
const HEADER_INDICES: [u32; 33] = [
    0, 2, 1, 3, 4, 5, 4, 6, 5, 7, 8, 9, 10, 11, 12, 7, 10, 11, 7, 11, 8, 7, 9, 10, 10, 9, 12, 9,
    11, 12, 8, 11, 9,
];
const HEADER_VERTICES: usize = 13;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    Vant,
    VantL,
    VantZ,
}

impl Kind {
    pub const ALL: [Self; 3] = [Self::Vant, Self::VantL, Self::VantZ];

    pub fn locator_prefix(self) -> &'static str {
        match self {
            Self::Vant => "vant",
            Self::VantL => "vanx",
            Self::VantZ => "vanz",
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub width: f32,
    pub ropes: usize,
    pub horizontal_u: [f32; 2],
    pub triangle_u: [f32; 2],
    pub triangle_v: [f32; 2],
    pub beam_v: [f32; 2],
    pub vertical_u: [f32; 2],
    pub upper_width: f32,
    pub upper_height: f32,
    pub vertical_repeat: f32,
    pub horizontal_repeat: f32,
    pub beam_height: f32,
    pub beam_width: f32,
    pub max_distance: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            width: 0.1,
            ropes: 5,
            horizontal_u: [0.5, 1.0],
            triangle_u: [0.0, 0.5],
            triangle_v: [0.0, 1.0],
            beam_v: [0.6, 1.0],
            vertical_u: [0.0, 0.1],
            upper_width: 1.0,
            upper_height: 1.0,
            vertical_repeat: 1.0,
            horizontal_repeat: 1.0,
            beam_height: 0.1,
            beam_width: 1.2,
            max_distance: 10_000.0,
        }
    }
}

impl Settings {
    pub fn validate(self) -> Result<()> {
        ensure!(
            (2..=1024).contains(&self.ropes)
                && self.width > 0.0
                && self.upper_width >= 0.0
                && self.upper_height >= 0.0
                && self.vertical_repeat > 0.0
                && self.horizontal_repeat > 0.0
                && self.beam_height >= 0.0
                && self.beam_width >= 0.0
                && self.max_distance >= 0.0
                && [
                    self.width,
                    self.upper_width,
                    self.upper_height,
                    self.vertical_repeat,
                    self.horizontal_repeat,
                    self.beam_height,
                    self.beam_width,
                    self.max_distance,
                ]
                .into_iter()
                .chain(self.horizontal_u)
                .chain(self.triangle_u)
                .chain(self.triangle_v)
                .chain(self.beam_v)
                .chain(self.vertical_u)
                .all(f32::is_finite),
            "invalid shroud settings"
        );
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vertex {
    pub position: Vec3,
    pub uv: [f32; 2],
}

#[derive(Clone, Debug)]
pub struct Geometry {
    settings: Settings,
    anchors: [Vec3; 3],
    offsets: [Vec3; EDGES],
}

impl Geometry {
    pub fn new(
        anchors: [[f32; 3]; 3],
        locator_positions: [[f32; 3]; 3],
        settings: Settings,
    ) -> Result<Option<Self>> {
        settings.validate()?;
        let [top, left, right] = anchors.map(Vec3::from_array);
        let direction = top - (left + right) * 0.5;
        ensure!(
            [top, left, right, direction]
                .into_iter()
                .all(Vec3::is_finite),
            "invalid shroud anchors"
        );
        if direction.length_squared() <= f32::EPSILON
            || (right - left).length_squared() <= f32::EPSILON
        {
            return Ok(None);
        }
        let [near_top, near_left, near_right] = locator_positions.map(Vec3::from_array);
        let look = near_top - (near_left + near_right) * 0.5;
        ensure!(
            look.is_finite() && look.length_squared().is_finite(),
            "invalid shroud locator basis"
        );
        // SetAdd builds the rope ring in locator coordinates before SetVertexes
        // moves the three endpoints into model coordinates.
        let (side, up) = if look.length_squared() == 0.0 {
            (Vec3::X, Vec3::Y)
        } else {
            let forward = look * look.length_squared().sqrt().recip();
            let up = Vec3::Z - forward * forward.z;
            let up = if up.length_squared() == 0.0 {
                Vec3::Y
            } else {
                up * up.length_squared().sqrt().recip()
            };
            let side = up.cross(forward);
            (side * side.length_squared().sqrt().recip(), up)
        };
        let offsets = std::array::from_fn(|index| {
            let angle = index as f32 / EDGES as f32 * 2.0 * std::f32::consts::PI;
            side * (settings.width * 0.5 * angle.cos()) + up * (settings.width * 0.5 * angle.sin())
        });
        Ok(Some(Self {
            settings,
            anchors: [top, left, right],
            offsets,
        }))
    }

    pub fn vertex_count(&self) -> usize {
        HEADER_VERTICES + 2 * self.settings.ropes * RING
    }

    pub fn index_count(&self) -> usize {
        HEADER_INDICES.len() + self.settings.ropes * EDGES * 6
    }

    pub fn visible_from(&self, distance_squared: f32, perspective: f32) -> bool {
        distance_squared * (perspective * 0.5).tan() < self.settings.max_distance
    }

    pub fn indices(&self) -> impl Iterator<Item = u32> + '_ {
        let lower = (HEADER_VERTICES + RING * self.settings.ropes) as u32;
        HEADER_INDICES
            .into_iter()
            .chain((0..self.settings.ropes).flat_map(move |rope| {
                let offset = (RING * rope) as u32;
                (0..EDGES as u32).flat_map(move |edge| {
                    let top = HEADER_VERTICES as u32 + offset + edge;
                    let bottom = lower + offset + edge;
                    [top, bottom + 1, bottom, top, top + 1, bottom + 1]
                })
            }))
    }

    pub fn vertices(&self) -> impl Iterator<Item = Vertex> + '_ {
        let [top, left, right] = self.anchors;
        let s = self.settings;
        let horizontal = normalize(right - left);
        let direction = (left + right) * 0.5 - top;
        let vertical = normalize(direction);
        let height = direction.length();
        let half_width = horizontal * (s.upper_width * 0.5);
        let triangle_depth = vertical * s.upper_height * (1.0 - s.beam_height);
        let triangle_left = top - half_width + triangle_depth;
        let triangle_right = top + half_width + triangle_depth;
        let beam_left = top - half_width + vertical * s.upper_height;
        let beam_right = beam_left + horizontal * s.upper_width;
        let cap_a = self.offsets[0] * s.beam_width;
        let cap_b = self.offsets[EDGES / 2] * s.beam_width;
        let [tri_l, tri_r] = s.triangle_u;
        let [tri_top, tri_bottom] = s.triangle_v;
        let [hor_l, hor_r] = s.horizontal_u;
        let [beam_top, beam_bottom] = s.beam_v;
        let [rope_l, rope_r] = s.vertical_u;
        let ladder_repeat = (height / s.horizontal_repeat + 0.5).trunc();
        let vertex = |position, u, v| Vertex {
            position,
            uv: [u, v],
        };
        let head = [
            vertex(top, (tri_l + tri_r) * 0.5, tri_top),
            vertex(triangle_left, tri_l, tri_bottom),
            vertex(triangle_right, tri_r, tri_bottom),
            vertex(triangle_left, hor_l, 0.0),
            vertex(triangle_right, hor_r, 0.0),
            vertex(left, hor_l, ladder_repeat),
            vertex(right, hor_r, ladder_repeat),
            vertex(
                beam_left - vertical * s.upper_height * s.beam_height,
                tri_l,
                beam_top,
            ),
            vertex(beam_left + cap_a, tri_l, beam_bottom),
            vertex(beam_left + cap_b, tri_l, beam_bottom),
            vertex(
                beam_right - vertical * s.upper_height * s.beam_height,
                tri_r,
                beam_top,
            ),
            vertex(beam_right + cap_a, tri_r, beam_bottom),
            vertex(beam_right + cap_b, tri_r, beam_bottom),
        ];
        let divisor = (s.ropes - 1) as f32;
        let upper_step = horizontal * (s.upper_width / divisor);
        let lower_step = (right - left) / divisor;
        let increment_u = (rope_r - rope_l) / EDGES as f32;
        let rings = [
            (beam_left, upper_step, 0.0),
            (left, lower_step, height / s.vertical_repeat),
        ];
        head.into_iter()
            .chain(rings.into_iter().flat_map(move |(mut center, step, v)| {
                (0..s.ropes).flat_map(move |_| {
                    let current = center;
                    center += step;
                    (0..RING).map(move |edge| {
                        vertex(
                            current + self.offsets[edge % EDGES],
                            rope_l + increment_u * edge as f32,
                            v,
                        )
                    })
                })
            }))
    }
}

fn normalize(v: Vec3) -> Vec3 {
    let square = v.length_squared() as f64;
    if square == 0.0 {
        return Vec3::ZERO;
    }
    let inverse = 1.0 / libm::sqrt(square);
    Vec3::from_array(
        v.to_array()
            .map(|component| (f64::from(component) * inverse) as f32),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ladders_crossbeams_and_seamed_rope_rings_share_native_anchors() -> Result<()> {
        for count in [2, 3, 5] {
            let settings = Settings {
                ropes: count,
                ..Default::default()
            };
            let anchors = [[0.0, 10.0, 0.0], [-2.0, 0.0, 0.0], [2.0, 0.0, 0.0]];
            let geometry = Geometry::new(anchors, anchors, settings)?.unwrap();
            let vertices = geometry.vertices().collect::<Vec<_>>();
            let indices = geometry.indices().collect::<Vec<_>>();
            assert_eq!(vertices.len(), 13 + 12 * count);
            assert_eq!(indices.len(), 33 + 30 * count);
            assert_eq!(&indices[..9], &[0, 2, 1, 3, 4, 5, 4, 6, 5]);
            assert!(
                indices
                    .iter()
                    .all(|&index| (index as usize) < vertices.len())
            );
            assert_eq!(vertices[0].position, Vec3::Y * 10.0);
            assert_eq!(vertices[1].position, Vec3::new(-0.5, 9.1, 0.0));
            assert_eq!(vertices[5].position, -Vec3::X * 2.0);
            assert_eq!(vertices[5].uv, [0.5, 10.0]);
            assert_eq!(vertices[13].position, vertices[18].position);
            assert_eq!(vertices[13].uv, [0.0, 0.0]);
            assert!((vertices[18].uv[0] - 0.1).abs() < 1e-6);
            assert_eq!(vertices[13 + 6 * count].uv[1], 10.0);
            assert!(geometry.visible_from(100.0, 1.0));
            assert!(!geometry.visible_from(100_000.0, 1.0));
        }
        Ok(())
    }
}
