//! Deterministic preview cloth. Locator conventions follow rigging/src/sail.cpp;
//! deformation is new kinematic code, not the original rigging/ship simulation.
use super::ModelVertex;
use anyhow::{Result, bail, ensure};
use bytemuck::Zeroable;
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;
use storm_formats::geometry::Label;
use storm_sim::SailTier;

const SEGMENTS: u32 = 16;
const QUAD_COLUMNS: u32 = 12;

#[derive(Clone, Copy, Debug)]
pub struct SailWind {
    /// Preview wind speed in m/s, in the inclusive range 0..=40.
    pub speed: f32,
    /// Direction the wind travels towards, clockwise from +Z towards +X (radians).
    pub direction: f32,
}

impl Default for SailWind {
    fn default() -> Self {
        Self {
            speed: 8.0,
            direction: std::f32::consts::FRAC_PI_4,
        }
    }
}

impl SailWind {
    pub fn validate(self) -> Result<()> {
        ensure!(
            self.speed.is_finite()
                && (0.0..=40.0).contains(&self.speed)
                && self.direction.is_finite(),
            "invalid preview wind (speed must be 0..=40 m/s)"
        );
        Ok(())
    }

    fn vector(self) -> Vec3 {
        let angle = self.direction.rem_euclid(std::f32::consts::TAU);
        Vec3::new(angle.sin(), 0.0, angle.cos())
    }
}

/// Points in locator order (sail1, sail2, sail3, sail4), around the perimeter.
#[derive(Clone, Copy, Debug)]
pub enum SailShape {
    Triangle([Vec3; 3]),
    Quad([Vec3; 4]),
}

#[derive(Clone, Debug)]
pub struct SailPatch {
    shape: SailShape,
    normal: Vec3,
    scale: f32,
    pub tier: SailTier,
    pub weight: f64,
    pub origin: Option<SailOrigin>,
    pub damage_settings: super::sail_damage::SailDamageSettings,
    drive_maximum: f32,
    drive_axes: [[f32; 3]; 3],
}

#[derive(Clone, Debug)]
pub struct SailOrigin {
    pub group: String,
    pub node: String,
    pub parent: String,
}

impl SailPatch {
    pub fn new(shape: SailShape) -> Result<Self> {
        let (points, du, dv) = match &shape {
            SailShape::Triangle(p) => (&p[..], p[1] - p[0], p[2] - p[0]),
            SailShape::Quad(p) => (
                &p[..],
                (p[1] - p[0] + p[2] - p[3]) * 0.5,
                (p[3] - p[0] + p[2] - p[1]) * 0.5,
            ),
        };
        ensure!(
            points.iter().all(|p| p.is_finite()),
            "non-finite sail locator"
        );
        let normal = du.cross(dv).normalize_or_zero();
        let scale = du.length().min(dv.length());
        ensure!(
            normal.is_finite()
                && normal.length_squared() > 0.5
                && scale.is_finite()
                && scale >= 0.1,
            "degenerate sail locators"
        );
        let weight = match shape {
            SailShape::Quad(_) => du.length() * dv.length(),
            SailShape::Triangle(p) => (p[2] - p[0]).length() * (p[1] - p[2]).length() * 0.5,
        };
        ensure!(weight.is_finite() && weight > 0.0, "invalid sail weight");
        let points = match shape {
            SailShape::Triangle(p) => [p[0], p[1], p[2], Vec3::ZERO],
            // Native hard points swap the third/fourth locator corners.
            SailShape::Quad(p) => [p[0], p[1], p[3], p[2]],
        };
        let drive_maximum = storm_sim::sail_drive::capacity(
            points.map(|p| p.to_array()),
            matches!(shape, SailShape::Triangle(_)),
        )?;
        Ok(Self {
            shape,
            normal,
            scale,
            tier: SailTier::Normal,
            weight: f64::from(weight),
            origin: None,
            damage_settings: Default::default(),
            drive_maximum,
            drive_axes: [Vec3::X.to_array(), Vec3::Y.to_array(), Vec3::Z.to_array()],
        })
    }

    pub fn is_triangle(&self) -> bool {
        matches!(self.shape, SailShape::Triangle(_))
    }
    /// Native scalar drive input. Static node axes and roll geometry are ports;
    /// this does not execute the original turning/rolling cloth processor.
    pub fn native_drive(&self, mask: u16) -> storm_sim::sail_drive::Sail {
        use storm_sim::sail_drive::{Kind, Sail};
        let kind = if self.is_triangle() {
            Kind::Triangle
        } else if self
            .origin
            .as_ref()
            .is_some_and(|o| matches!(o.group.as_bytes().get(4), Some(b'f' | b'n')))
        {
            Kind::Trapezoid
        } else if self
            .origin
            .as_ref()
            .is_some_and(|o| o.group.as_bytes().get(4) == Some(&b'x'))
        {
            Kind::Special
        } else {
            Kind::Square
        };
        Sail {
            kind,
            maximum: self.drive_maximum,
            axes: self.drive_axes,
            holes: (mask & ((1 << self.section_count()) - 1)).count_ones() as i32,
            hole_maximum: self.section_count() as i32,
            hole_dependence: self.damage_settings.speed_depend,
            rolling: false,
            rolling_delta: None,
            roll_type: if self.tier == SailTier::Normal { 1 } else { 2 },
        }
    }
    /// Rope ties follow the closest original hard point and preserve their rest offset.
    pub(super) fn gather_anchor(&self, point: Vec3, deployed: f32) -> Vec3 {
        let corners = match self.shape {
            SailShape::Triangle(p) => [
                (p[0], 0.0, 0.0),
                (p[1], 1.0, 0.0),
                (p[2], 0.0, 1.0),
                (p[2], 0.0, 1.0),
            ],
            SailShape::Quad(p) => [
                (p[0], 0.0, 0.0),
                (p[1], 1.0, 0.0),
                (p[2], 1.0, 1.0),
                (p[3], 0.0, 1.0),
            ],
        };
        let &(corner, u, v) = corners
            .iter()
            .min_by(|a, b| {
                a.0.distance_squared(point)
                    .total_cmp(&b.0.distance_squared(point))
            })
            .expect("sail corners");
        Vec3::from_array(self.vertex(u, v, 0.0, 0.0, 0.0, deployed).position) + point - corner
    }

    pub fn vertex_count(&self) -> usize {
        let n = SEGMENTS as usize + 1;
        if self.is_triangle() {
            n * (n + 1) / 2
        } else {
            n * (QUAD_COLUMNS as usize + 1)
        }
    }

    /// Generate topology once; a triangular grid has no duplicated/degenerate tip.
    pub fn indices(&self) -> Vec<u32> {
        self.original_indices(0).indices
    }

    pub fn vertices(&self, wind: SailWind, seconds: f32) -> Result<Vec<ModelVertex>> {
        let mut vertices = vec![ModelVertex::zeroed(); self.vertex_count()];
        self.deform(&mut vertices, wind, seconds)?;
        Ok(vertices)
    }

    /// Reuses the CPU/GPU allocation; every point on the boundary stays pinned.
    pub fn deform(&self, vertices: &mut [ModelVertex], wind: SailWind, seconds: f32) -> Result<()> {
        self.deform_with_setting(vertices, wind, seconds, 1.0)
    }

    /// Gather free edges towards sail1--sail2; a narrow folded band remains at zero.
    pub fn deform_with_setting(
        &self,
        vertices: &mut [ModelVertex],
        wind: SailWind,
        seconds: f32,
        deployed: f32,
    ) -> Result<()> {
        self.deform_frame(vertices, wind, seconds, deployed, 1.0, 1.0)
    }
    pub fn deform_with_damage(
        &self,
        vertices: &mut [ModelVertex],
        wind: SailWind,
        seconds: f32,
        deployed: f32,
        mask: u16,
    ) -> Result<()> {
        let count = (mask & ((1 << self.section_count()) - 1)).count_ones() as f32;
        let flex = 1.0
            - count
                * if self.is_triangle() {
                    self.damage_settings.triangle_flex
                } else {
                    self.damage_settings.quad_flex
                };
        self.deform_frame(
            vertices,
            wind,
            seconds,
            deployed,
            flex,
            if self.is_triangle() { flex } else { 1.0 },
        )
    }
    #[allow(clippy::too_many_arguments)]
    fn deform_frame(
        &self,
        vertices: &mut [ModelVertex],
        wind: SailWind,
        seconds: f32,
        deployed: f32,
        flex: f32,
        flutter_scale: f32,
    ) -> Result<()> {
        ensure!(
            deployed.is_finite() && (0.0..=1.0).contains(&deployed),
            "invalid sail deployment"
        );
        wind.validate()?;
        ensure!(seconds.is_finite(), "invalid sail time");
        ensure!(
            vertices.len() == self.vertex_count(),
            "invalid sail vertex count"
        );
        let pressure = wind.speed * wind.speed / (64.0 + wind.speed * wind.speed)
            * self.normal.dot(wind.vector())
            * flex;
        let flutter = 0.025 * wind.speed / (8.0 + wind.speed) * flutter_scale;
        // Reduce in f64 before converting to f32, including very large finite times.
        let phase = (f64::from(seconds) * (1.8 + f64::from(wind.speed) * 0.12))
            .rem_euclid(std::f64::consts::TAU) as f32;
        if self.is_triangle() {
            for row in 0..=SEGMENTS {
                for col in 0..=row {
                    vertices[(row * (row + 1) / 2 + col) as usize] = self.vertex(
                        (row - col) as f32 / SEGMENTS as f32,
                        col as f32 / SEGMENTS as f32,
                        pressure,
                        flutter,
                        phase,
                        deployed,
                    );
                }
            }
        } else {
            for col in 0..=QUAD_COLUMNS {
                for row in 0..=SEGMENTS {
                    vertices[(col * (SEGMENTS + 1) + row) as usize] = self.vertex(
                        col as f32 * (1.0 / QUAD_COLUMNS as f32),
                        row as f32 / SEGMENTS as f32,
                        pressure,
                        flutter,
                        phase,
                        deployed,
                    );
                }
            }
        }
        ensure!(
            vertices
                .iter()
                .all(|v| Vec3::from_array(v.position).is_finite()
                    && Vec3::from_array(v.normal).is_finite()),
            "sail deformation overflow"
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn vertex(
        &self,
        u: f32,
        v: f32,
        pressure: f32,
        flutter: f32,
        phase: f32,
        deployed: f32,
    ) -> ModelVertex {
        // Keep a small nonzero band for bundled cloth, avoiding collapsed triangles.
        // The band is visual cloth only; it contributes no drive at zero deployment.
        let span = 0.025 + 0.975 * deployed;
        let shape = if deployed == 1.0 {
            self.shape
        } else {
            match self.shape {
                SailShape::Quad(p) => {
                    SailShape::Quad([p[0], p[1], p[1].lerp(p[2], span), p[0].lerp(p[3], span)])
                }
                SailShape::Triangle(p) => {
                    let edge = p[1] - p[0];
                    let along = ((p[2] - p[0]).dot(edge) / edge.length_squared()).clamp(0.0, 1.0);
                    let foot = p[0] + edge * along;
                    SailShape::Triangle([p[0], p[1], foot.lerp(p[2], span)])
                }
            }
        };
        let (base, du, dv, envelope, eu, ev, uv) = match shape {
            SailShape::Triangle(p) => (
                p[0] + (p[1] - p[0]) * u + (p[2] - p[0]) * v,
                p[1] - p[0],
                p[2] - p[0],
                27.0 * u * v * (1.0 - u - v).max(0.0),
                27.0 * v * (1.0 - 2.0 * u - v),
                27.0 * u * (1.0 - u - 2.0 * v),
                // SAILONE::SetTexGrid: tip=(0,1), sail2=(1,0), sail3=(1,1).
                [u + v, 1.0 - u],
            ),
            SailShape::Quad(p) => (
                p[0].lerp(p[1], u).lerp(p[3].lerp(p[2], u), v),
                (p[1] - p[0]).lerp(p[2] - p[3], v),
                (p[3] - p[0]).lerp(p[2] - p[1], u),
                16.0 * u * (1.0 - u) * v * (1.0 - v),
                16.0 * (1.0 - 2.0 * u) * v * (1.0 - v),
                16.0 * u * (1.0 - u) * (1.0 - 2.0 * v),
                [u, v],
            ),
        };
        let wave_phase = phase + u * 7.0 + v * 11.0;
        let billow = 0.28 * pressure + flutter * wave_phase.sin();
        let wave_derivative = flutter * wave_phase.cos();
        let fold_phase = v * (6.0 * std::f32::consts::PI);
        let fold_scale = self.scale * 0.025 * (1.0 - deployed);
        let fold = fold_scale * v * fold_phase.sin();
        let fold_derivative =
            fold_scale * (fold_phase.sin() + v * 6.0 * std::f32::consts::PI * fold_phase.cos());
        let height = self.scale * deployed * envelope * billow + fold;
        let normal = (du
            + self.normal
                * self.scale
                * deployed
                * (eu * billow + envelope * wave_derivative * 7.0))
            .cross(
                dv + self.normal
                    * (self.scale * deployed * (ev * billow + envelope * wave_derivative * 11.0)
                        + fold_derivative),
            )
            .normalize_or_zero();
        ModelVertex {
            position: (base + self.normal * height).to_array(),
            normal: normal.to_array(),
            uv,
            color: [1.0, 0.97, 0.88, 1.0],
        }
    }
}

pub(super) fn from_labels(
    labels: &[Label],
    world: Mat4,
    node: &str,
    parent: &str,
) -> Result<Vec<SailPatch>> {
    let mut groups = BTreeMap::<&str, [Option<Vec3>; 4]>::new();
    for label in labels {
        if !label.group.starts_with("sail") {
            continue;
        }
        let Some(index) = ["sail1", "sail2", "sail3", "sail4"]
            .iter()
            .position(|&s| s == label.name)
        else {
            continue;
        }; // Rope and sail0 limit locators are not cloth corners.
        let corners = groups.entry(&label.group).or_insert([None; 4]);
        ensure!(
            corners[index].is_none(),
            "duplicate sail corner: {}/{}",
            label.group,
            label.name
        );
        corners[index] = Some(Mat4::from_cols_array(&label.matrix).w_axis.truncate());
    }
    groups
        .into_iter()
        .map(|(group, corners)| {
            let shape = match group.as_bytes().get(4) {
                Some(b't' | b'v') => {
                    let [Some(a), Some(b), Some(c), None] = corners else {
                        bail!("incomplete triangular sail: {group}");
                    };
                    SailShape::Triangle([a, b, c])
                }
                Some(b's' | b'g' | b'f' | b'd' | b'x' | b'n') => {
                    let [Some(a), Some(b), Some(c), Some(d)] = corners else {
                        bail!("incomplete four-corner sail: {group}");
                    };
                    SailShape::Quad([a, b, c, d])
                }
                _ => bail!("unsupported sail locator group: {group}"),
            };
            // C++ maxSpeed uses local locator dimensions, before node transforms.
            let local = SailPatch::new(shape)?;
            let weight = local.weight;
            let world_shape = match shape {
                SailShape::Triangle(p) => SailShape::Triangle(p.map(|p| world.transform_point3(p))),
                SailShape::Quad(p) => SailShape::Quad(p.map(|p| world.transform_point3(p))),
            };
            let mut patch = SailPatch::new(world_shape)?;
            patch.weight = weight;
            patch.drive_maximum = local.drive_maximum;
            patch.drive_axes = [
                world.x_axis.truncate().to_array(),
                world.y_axis.truncate().to_array(),
                world.z_axis.truncate().to_array(),
            ];
            patch.tier = SailTier::from_original(group, node, parent);
            patch.origin = Some(SailOrigin {
                group: group.into(),
                node: node.into(),
                parent: parent.into(),
            });
            Ok(patch)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shapes() -> [SailShape; 2] {
        [
            SailShape::Quad([
                Vec3::ZERO,
                Vec3::X * 4.0,
                Vec3::new(4.0, 6.0, 0.0),
                Vec3::Y * 6.0,
            ]),
            SailShape::Triangle([Vec3::ZERO, Vec3::X * 4.0, Vec3::Y * 6.0]),
        ]
    }

    #[test]
    fn furling_reduces_both_surfaces_but_preserves_the_attachment_edge_and_uvs() {
        let calm = SailWind {
            speed: 0.0,
            direction: 0.0,
        };
        for shape in shapes() {
            let patch = SailPatch::new(shape).unwrap();
            let full = patch.vertices(calm, 0.0).unwrap();
            let indices = patch.indices();
            let mut vertices = full.clone();
            for deployed in [1.0, 0.9, 0.5, 0.1, 0.0] {
                patch
                    .deform_with_setting(&mut vertices, calm, 2.0, deployed)
                    .unwrap();
                let mut area = 0.0;
                for tri in indices.chunks_exact(3) {
                    let [a, b, c] = [tri[0], tri[1], tri[2]]
                        .map(|i| Vec3::from_array(vertices[i as usize].position));
                    let projected = (b - a).cross(c - a).dot(patch.normal);
                    assert!(
                        projected > 1e-6,
                        "furling must not collapse or reverse triangles"
                    );
                    area += projected * 0.5;
                }
                let full_area = if patch.is_triangle() { 12.0 } else { 24.0 };
                assert!((area - full_area * (0.025 + 0.975 * deployed)).abs() < 1e-4);
                for (v, original) in vertices.iter().zip(&full) {
                    assert_eq!(v.uv, original.uv);
                    assert!((Vec3::from_array(v.normal).length() - 1.0).abs() < 1e-5);
                }
                for i in 0..=if patch.is_triangle() {
                    SEGMENTS
                } else {
                    QUAD_COLUMNS
                } as usize
                {
                    let index = if patch.is_triangle() {
                        i * (i + 1) / 2
                    } else {
                        i * (SEGMENTS as usize + 1)
                    };
                    assert!(
                        Vec3::from_array(vertices[index].position)
                            .distance(Vec3::from_array(full[index].position))
                            < 1e-6
                    );
                }
            }
            let furled = vertices.clone();
            patch
                .deform_with_setting(
                    &mut vertices,
                    SailWind {
                        speed: 40.0,
                        direction: 1.0,
                    },
                    17.0,
                    0.0,
                )
                .unwrap();
            for (a, b) in vertices.iter().zip(&furled) {
                assert_eq!(a.position, b.position, "bundled cloth must not flutter");
                assert_eq!(a.normal, b.normal);
            }
            patch
                .deform_with_setting(&mut vertices, calm, 0.0, 1.0)
                .unwrap();
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&vertices),
                bytemuck::cast_slice::<_, u8>(&full)
            );
            for bad in [-0.1, 1.1, f32::NAN] {
                assert!(
                    patch
                        .deform_with_setting(&mut vertices, calm, 0.0, bad)
                        .is_err()
                );
            }
        }
    }

    #[test]
    fn grids_cover_each_surface_without_degenerate_or_reversed_triangles() {
        for shape in shapes() {
            let patch = SailPatch::new(shape).unwrap();
            let vertices = patch
                .vertices(
                    SailWind {
                        speed: 0.0,
                        direction: 0.0,
                    },
                    0.0,
                )
                .unwrap();
            let mut area = 0.0;
            for triangle in patch.indices().chunks_exact(3) {
                let p = triangle
                    .iter()
                    .map(|&i| Vec3::from_array(vertices[i as usize].position))
                    .collect::<Vec<_>>();
                let cross = (p[1] - p[0]).cross(p[2] - p[0]);
                assert!(cross.dot(Vec3::Z) > 1e-5, "degenerate or reversed triangle");
                area += cross.length() * 0.5;
            }
            let expected = if patch.is_triangle() { 12.0 } else { 24.0 };
            assert!((area - expected).abs() < 0.001, "surface area {area}");
            assert_eq!(vertices.len(), if patch.is_triangle() { 153 } else { 221 });
        }
    }

    #[test]
    fn wind_is_repeatable_reversible_and_keeps_all_boundary_points_pinned() {
        for shape in shapes() {
            let patch = SailPatch::new(shape).unwrap();
            let calm = SailWind {
                speed: 0.0,
                direction: 0.0,
            };
            let rest = patch.vertices(calm, 0.0).unwrap();
            let still = patch.vertices(calm, 9.0).unwrap();
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&rest),
                bytemuck::cast_slice::<_, u8>(&still)
            );
            let wind = SailWind {
                speed: 16.0,
                direction: 0.0,
            };
            let first = patch.vertices(wind, 0.0).unwrap();
            let later = patch.vertices(wind, 2.0).unwrap();
            let repeat = patch.vertices(wind, 2.0).unwrap();
            assert_eq!(
                bytemuck::cast_slice::<_, u8>(&later),
                bytemuck::cast_slice::<_, u8>(&repeat)
            );
            assert_ne!(
                bytemuck::cast_slice::<_, u8>(&first),
                bytemuck::cast_slice::<_, u8>(&later)
            );
            let reverse = patch
                .vertices(
                    SailWind {
                        direction: std::f32::consts::PI,
                        ..wind
                    },
                    0.0,
                )
                .unwrap();
            let mut moved = false;
            for (i, base) in rest.iter().enumerate() {
                let p = Vec3::from_array(base.position);
                let boundary = if patch.is_triangle() {
                    p.x.abs() < 1e-5
                        || p.y.abs() < 1e-5
                        || (p.x / 4.0 + p.y / 6.0 - 1.0).abs() < 1e-5
                } else {
                    p.x.abs() < 1e-5
                        || p.y.abs() < 1e-5
                        || (p.x - 4.0).abs() < 1e-5
                        || (p.y - 6.0).abs() < 1e-5
                };
                if boundary {
                    assert!(
                        Vec3::from_array(later[i].position).distance(p) < 1e-5,
                        "anchor moved"
                    );
                } else {
                    assert!(first[i].position[2] > 0.0 && reverse[i].position[2] < 0.0);
                    moved |= (later[i].position[2] - first[i].position[2]).abs() > 1e-4;
                }
                assert!((Vec3::from_array(later[i].normal).length() - 1.0).abs() < 1e-4);
            }
            assert!(moved, "wind must move the cloth interior");
            assert!(patch.vertices(wind, f32::MAX).is_ok());
            assert!(patch.vertices(wind, f32::NAN).is_err());
        }
        for speed in [-1.0, 41.0, f32::NAN, f32::INFINITY] {
            assert!(
                SailWind {
                    speed,
                    direction: 0.0
                }
                .validate()
                .is_err()
            );
        }
        assert!(SailPatch::new(SailShape::Triangle([Vec3::ZERO; 3])).is_err());
    }

    #[test]
    fn native_sail_capacity_uses_local_hard_points_and_keeps_node_axes() {
        let points = [
            Vec3::ZERO,
            Vec3::X * 3.0,
            Vec3::new(2.0, 4.0, 0.0),
            Vec3::Y * 4.0,
        ];
        let labels = points
            .iter()
            .enumerate()
            .map(|(i, p)| Label {
                group: "sailn1".into(),
                name: format!("sail{}", i + 1),
                matrix: Mat4::from_translation(*p).to_cols_array(),
            })
            .collect::<Vec<_>>();
        let transform = Mat4::from_scale_rotation_translation(
            Vec3::new(2.0, 3.0, 1.0),
            glam::Quat::from_rotation_y(0.5),
            Vec3::new(4.0, 0.0, 2.0),
        );
        let local =
            from_labels(&labels, Mat4::IDENTITY, "rey_b2", "mast2").unwrap()[0].native_drive(0);
        let world = from_labels(&labels, transform, "rey_b2", "mast2").unwrap()[0].native_drive(3);
        assert_eq!(local.maximum, world.maximum);
        assert_eq!(world.kind, storm_sim::sail_drive::Kind::Trapezoid);
        assert_eq!(world.roll_type, 2);
        assert_eq!(world.holes, 2);
        assert_eq!(world.axes[0], transform.x_axis.truncate().to_array());
    }
    #[test]
    fn original_locator_types_corner_order_uvs_and_parent_transform_are_preserved() {
        let mut labels = Vec::new();
        for (kind, count) in [
            ('t', 3),
            ('v', 3),
            ('s', 4),
            ('g', 4),
            ('f', 4),
            ('d', 4),
            ('x', 4),
            ('n', 4),
        ] {
            let positions = [Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Y, Vec3::Y];
            for (i, &p) in positions[..count].iter().enumerate() {
                labels.push(Label {
                    group: format!("sail{kind}2"),
                    name: format!("sail{}", i + 1),
                    matrix: Mat4::from_translation(p).to_cols_array(),
                });
            }
        }
        let transform =
            Mat4::from_translation(Vec3::new(3.0, 5.0, -4.0)) * Mat4::from_rotation_y(0.8);
        let patches = from_labels(&labels, transform, "rey_b2", "mast2").unwrap();
        assert_eq!(patches.len(), 8);
        assert_eq!(patches.iter().filter(|p| p.is_triangle()).count(), 2);
        let tri = patches.iter().find(|p| p.is_triangle()).unwrap();
        let vertices = tri.vertices(SailWind::default(), 3.0).unwrap();
        for (index, point, uv) in [
            (0, Vec3::ZERO, [0.0, 1.0]),
            (136, Vec3::X, [1.0, 0.0]),
            (152, Vec3::X + Vec3::Y, [1.0, 1.0]),
        ] {
            assert!(
                Vec3::from_array(vertices[index].position)
                    .distance(transform.transform_point3(point))
                    < 1e-5
            );
            assert_eq!(vertices[index].uv, uv);
        }
        labels.push(Label {
            group: "sailt2".into(),
            name: "sail1".into(),
            matrix: Mat4::IDENTITY.to_cols_array(),
        });
        assert!(from_labels(&labels, transform, "rey_b2", "mast2").is_err());
        labels.pop();
        labels.remove(0);
        assert!(from_labels(&labels, transform, "rey_b2", "mast2").is_err());
    }
}
