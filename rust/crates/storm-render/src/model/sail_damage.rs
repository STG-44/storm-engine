//! Original sail hole sectors, coarse collision and parus_hole alpha material.
//! Ported from retained GPL SAILONE::FillIndex/SetTexGrid/CheckSailSquar.
use super::*;
use glam::Vec2;

#[derive(Clone, Copy, Debug)]
pub struct SailImpact {
    pub mesh: usize,
    pub section: u32,
    pub fraction: f32,
    pub uv: [f32; 2],
    pub position: [f32; 3],
    pub new_damage: bool,
    pub hole_mask: u16,
}

impl SailPatch {
    pub fn section_count(&self) -> u32 {
        if self.is_triangle() { 10 } else { 12 }
    }
    pub fn intact_fraction(&self, mask: u16) -> f64 {
        let count = (mask & ((1 << self.section_count()) - 1)).count_ones() as i32;
        f64::from(original_sail_speed(
            count,
            self.section_count() as i32,
            self.weight as f32,
            self.damage_settings.speed_depend,
        )) / self.weight
    }
    /// Literal CheckSailSquar/SSailTrace/TSailTrace behavior: alpha holes do
    /// not suppress collision, and a segment may damage several coarse cells.
    pub(super) fn trace_original(
        &self,
        vertices: &[ModelVertex],
        from: Vec3,
        to: Vec3,
        mut mask: u16,
    ) -> Option<(f32, [f32; 2], u32, u16)> {
        let cells = self.trace_cells();
        let points = cells
            .iter()
            .flat_map(|(_, a, b)| {
                a.iter()
                    .copied()
                    .chain(b.iter().flat_map(|b| b.iter().copied()))
            })
            .map(|i| Vec3::from_array(vertices[i as usize].position))
            .collect::<Vec<_>>();
        let min = points
            .iter()
            .fold(Vec3::splat(f32::INFINITY), |a, b| a.min(*b));
        let max = points
            .iter()
            .fold(Vec3::splat(f32::NEG_INFINITY), |a, b| a.max(*b));
        let center = (min + max) * 0.5;
        let size = max - center;
        if !original_box_trace(from, to, center - size, center + size) {
            return None;
        }
        let mut nearest: Option<(f32, [f32; 2], u32)> = None;
        for (section, a, b) in cells {
            let test = |tri: [u32; 3]| {
                let v = tri.map(|i| vertices[i as usize]);
                let p = v.map(|v| Vec3::from_array(v.position));
                let (t, damage) = original_triangle_trace(p[0], p[1], p[2], from, to);
                // UV is diagnostic only; original collision returns t and the cell.
                let hit = from.lerp(to, t);
                let ab = p[1] - p[0];
                let ac = p[2] - p[0];
                let d = hit - p[0];
                let denominator = ab.length_squared() * ac.length_squared() - ab.dot(ac).powi(2);
                let (u, w) = if denominator.abs() > 1e-12 {
                    (
                        (d.dot(ab) * ac.length_squared() - d.dot(ac) * ab.dot(ac)) / denominator,
                        (d.dot(ac) * ab.length_squared() - d.dot(ab) * ab.dot(ac)) / denominator,
                    )
                } else {
                    (0.0, 0.0)
                };
                let uv = (Vec2::from_array(v[0].uv) * (1.0 - u - w)
                    + Vec2::from_array(v[1].uv) * u
                    + Vec2::from_array(v[2].uv) * w)
                    .to_array();
                (t, damage, uv)
            };
            let mut hit = test(a);
            if hit.0 > 1.0
                && let Some(b) = b
            {
                hit = test(b);
            }
            if hit.0 <= 1.0 {
                if hit.1 {
                    mask |= 1 << section;
                }
                if nearest.as_ref().is_none_or(|n| hit.0 < n.0) {
                    nearest = Some((hit.0, hit.2, section));
                }
            }
        }
        nearest.map(|(t, uv, section)| (t, uv, section, mask))
    }
}

/// GetSailSpeed's original float arithmetic and clamping.
fn original_sail_speed(holes: i32, mut maximum: i32, speed: f32, dependence: f32) -> f32 {
    if maximum <= 0 {
        maximum = 1;
    }
    if holes <= 0 {
        return speed;
    }
    let current = speed * (1.0 - holes as f32 / maximum as f32 * dependence);
    if current < 0.0 { 0.0 } else { current }
}

/// Original coarse bounding-box admission test, including its asymmetric Y test.
fn original_box_trace(src: Vec3, dst: Vec3, min: Vec3, max: Vec3) -> bool {
    let inside = |v: Vec3| v.cmpge(min).all() && v.cmple(max).all();
    if inside(src) || inside(dst) {
        return true;
    }
    for axis in [0, 2, 1] {
        let below = src[axis] < min[axis];
        let above = axis != 1 && src[axis] > max[axis];
        if below || above {
            let edge = if below { min[axis] } else { max[axis] };
            if (below && dst[axis] < edge) || (above && dst[axis] > edge) {
                return false;
            }
            let numerator = (dst - src) * (edge - src[axis]);
            // CVECTOR / float uses a double reciprocal, then casts each component.
            let reciprocal = 1.0 / f64::from(dst[axis] - src[axis]);
            let v = src
                + Vec3::new(
                    (f64::from(numerator.x) * reciprocal) as f32,
                    (f64::from(numerator.y) * reciprocal) as f32,
                    (f64::from(numerator.z) * reciprocal) as f32,
                );
            if (0..3)
                .filter(|i| *i != axis)
                .all(|i| v[i] >= min[i] && v[i] <= max[i])
            {
                return true;
            }
        }
    }
    false
}
/// Direct translation of SAILONE::CheckSailSquar's three projected edge tests.
fn original_triangle_trace(a: Vec3, b: Vec3, c: Vec3, src: Vec3, dst: Vec3) -> (f32, bool) {
    let d1 = b - a;
    let d2 = c - a;
    let normal = Vec3::new(
        d1.y * d2.z - d1.z * d2.y,
        d1.z * d2.x - d1.x * d2.z,
        d1.x * d2.y - d1.y * d2.x,
    );
    let plane = normal.dot(a);
    let sd = normal.dot(src) - plane;
    let dd = normal.dot(dst) - plane;
    if sd == 0.0 {
        return (0.0, false);
    }
    if dd == 0.0 {
        return (1.0, false);
    }
    if (sd < 0.0) == (dd < 0.0) {
        return (2.0, false);
    }
    let t = sd / (sd - dd);
    let point = src + (dst - src) * t;
    for (u, v) in [(0, 1), (2, 1), (0, 2)] {
        for (a, b, opposite) in [(a, b, c), (b, c, a), (c, a, b)] {
            let edge = b - a;
            let ka = -edge[v];
            let kb = edge[u];
            let kd = a[v] * edge[u] - a[u] * edge[v];
            let d1 = ka * point[u] + kb * point[v];
            let d2 = ka * opposite[u] + kb * opposite[v];
            if d1 != kd && d2 != kd && ((d1 > kd) ^ (d2 > kd)) {
                return (2.0, false);
            }
        }
    }
    (t, true)
}

#[derive(Clone, Copy, Debug)]
pub struct SailDamageSettings {
    pub speed_depend: f32,
    pub triangle_flex: f32,
    pub quad_flex: f32,
}
impl Default for SailDamageSettings {
    fn default() -> Self {
        Self {
            speed_depend: 1.0,
            triangle_flex: 0.01,
            quad_flex: 0.01,
        }
    }
}
impl SailDamageSettings {
    pub fn from_ini(text: &str) -> Result<Self> {
        let mut result = Self::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_ascii_lowercase();
                continue;
            }
            if section != "sails" {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let field = match key.trim().to_ascii_lowercase().as_str() {
                    "fholedepend" => &mut result.speed_depend,
                    "ftholeflexdepend" => &mut result.triangle_flex,
                    "fsholeflexdepend" => &mut result.quad_flex,
                    _ => continue,
                };
                *field = value.trim().parse()?;
            }
        }
        ensure!(
            [result.speed_depend, result.triangle_flex, result.quad_flex]
                .iter()
                .all(|v| v.is_finite()),
            "invalid original sail damage dependence"
        );
        result.triangle_flex = result.triangle_flex.min(0.1);
        result.quad_flex = result.quad_flex.min(1.0 / 12.0);
        Ok(result)
    }
}
pub struct SailDamageMaterial {
    pub texture: storm_formats::texture::Texture,
    pub source: Option<String>,
}
impl SailDamageMaterial {
    /// Synthetic alpha markers for redistribution/CI, never substituted for game artwork.
    pub fn fixture() -> Self {
        use storm_formats::{
            TxHeader,
            texture::{Mip, Texture},
        };
        let mut rgba = vec![255u8; 64 * 64 * 4];
        for (x, y) in [
            (38, 5),
            (52, 11),
            (46, 20),
            (58, 26),
            (37, 42),
            (48, 49),
            (56, 57),
        ] {
            for dy in 0..2 {
                for dx in 0..2 {
                    rgba[((y + dy) * 64 + x + dx) * 4 + 3] = 0;
                }
            }
        }
        Self {
            texture: Texture {
                header: TxHeader {
                    flags: 0,
                    width: 64,
                    height: 64,
                    mips: 1,
                    format: 21,
                    mip0_bytes: 64 * 64 * 4,
                },
                mips: vec![Mip {
                    width: 64,
                    height: 64,
                    rgba,
                }],
            },
            source: None,
        }
    }
}
mod topology;

pub(super) fn validate_holes<'a>(
    state: &ShipState,
    sails: impl Iterator<Item = (usize, &'a SailPatch)>,
) -> Result<()> {
    let counts = sails
        .map(|(i, p)| (i, p.section_count()))
        .collect::<std::collections::BTreeMap<_, _>>();
    ensure!(
        state
            .sail_holes
            .iter()
            .all(|(i, mask)| counts.get(i).is_some_and(|n| u32::from(*mask) < (1 << n))),
        "unknown sail or invalid hole mask"
    );
    Ok(())
}
impl ModelAsset {
    /// SAILONE::SetAllHole restores only the original 10/12 low mask bits.
    pub fn set_sail_holes(&self, state: &mut ShipState, mesh: usize, data: u32) -> Result<()> {
        let patch = self
            .meshes
            .get(mesh)
            .and_then(|m| m.sail.as_ref())
            .ok_or_else(|| anyhow::anyhow!("unknown sail"))?;
        state
            .sail_holes
            .insert(mesh, (data & ((1 << patch.section_count()) - 1)) as u16);
        Ok(())
    }
    /// A cannonball's swept segment in world coordinates. Penetrates every sail
    /// along the segment, at most one hit per sail, sorted from entry to exit.
    #[allow(clippy::too_many_arguments)]
    pub fn trace_sail_shot(
        &self,
        state: &mut ShipState,
        from: Vec3,
        to: Vec3,
        wind: SailWind,
        seconds: f32,
        rig: storm_sim::RigState,
        pose: ModelPose,
    ) -> Result<Vec<SailImpact>> {
        state.validate(&self.ship)?;
        validate_holes(
            state,
            self.meshes
                .iter()
                .enumerate()
                .filter_map(|(i, m)| m.sail.as_ref().map(|p| (i, p))),
        )?;
        wind.validate()?;
        rig.validate()?;
        ensure!(
            seconds.is_finite() && from.is_finite() && to.is_finite() && (to - from).is_finite(),
            "invalid shot segment or time"
        );
        let base = pose.matrix()?;
        let direction = Vec3::new(wind.direction.sin(), 0.0, wind.direction.cos()) * wind.speed;
        let local = base.inverse().transform_vector3(direction);
        let local_wind = SailWind {
            speed: local.x.hypot(local.z).min(40.0),
            direction: local.x.atan2(local.z),
        };
        let mut hits = Vec::new();
        for (i, mesh) in self.meshes.iter().enumerate() {
            let Some(patch) = &mesh.sail else {
                continue;
            };
            // Bundled sails are not cannon targets (SAILONE::Trace bRolling).
            if rig.deployed(patch.tier) <= 0.0 {
                continue;
            }
            if mesh.ship_node.as_deref().is_some_and(|n| {
                self.ship.hidden(n, state, seconds)
                    || self
                        .ship
                        .fallen_mast(n, state)
                        .is_some_and(|(_, t, _)| seconds >= t)
            }) {
                continue;
            }
            let mask = state.sail_holes.get(&i).copied().unwrap_or(0);
            let mut vertices = patch.vertices(local_wind, seconds)?;
            patch.deform_with_damage(
                &mut vertices,
                local_wind,
                seconds,
                rig.deployed(patch.tier) as f32,
                mask,
            )?;
            let from_local = base.inverse().transform_point3(from);
            let to_local = base.inverse().transform_point3(to);
            if let Some((fraction, uv, section, hole_mask)) =
                patch.trace_original(&vertices, from_local, to_local, mask)
            {
                hits.push(SailImpact {
                    mesh: i,
                    section,
                    fraction,
                    uv,
                    position: from.lerp(to, fraction).to_array(),
                    new_damage: hole_mask != mask,
                    hole_mask,
                });
            }
        }
        hits.sort_by(|a, b| a.fraction.total_cmp(&b.fraction));
        for h in &hits {
            state.sail_holes.insert(h.mesh, h.hole_mask);
        }
        Ok(hits)
    }
    /// Reproducible fixture volley. Each segment crosses the actual deformed
    /// triangle centroids; all hits still go through trace_sail_shot.
    pub fn sail_volley(
        &self,
        state: &mut ShipState,
        sections: u32,
        wind: SailWind,
        seconds: f32,
        rig: storm_sim::RigState,
    ) -> Result<Vec<SailImpact>> {
        ensure!(
            (1..=12).contains(&sections),
            "volley sections must be 1..=12"
        );
        let mut hits = Vec::new();
        for (index, mesh) in self.meshes.iter().enumerate() {
            let Some(patch) = &mesh.sail else {
                continue;
            };
            let order: &[u32] = if patch.is_triangle() {
                &[4, 8, 1, 6, 0, 9, 3, 7, 2, 5]
            } else {
                &[5, 9, 2, 7, 0, 10, 4, 8, 3, 6, 11, 1]
            };
            let cells = patch.trace_cells();
            let mut vertices = patch.vertices(wind, seconds)?;
            for &section in order.iter().take(sections as usize) {
                // Only the test helper retries: the original trace itself is unchanged.
                for (attempt, weights) in [
                    [0.2, 0.3, 0.5],
                    [0.45, 0.2, 0.35],
                    [0.15, 0.7, 0.15],
                    [0.3, 0.4, 0.3],
                ]
                .into_iter()
                .enumerate()
                {
                    let mask = state.sail_holes.get(&index).copied().unwrap_or(0);
                    if mask & (1 << section) != 0 {
                        break;
                    }
                    patch.deform_with_damage(
                        &mut vertices,
                        wind,
                        seconds,
                        rig.deployed(patch.tier) as f32,
                        mask,
                    )?;
                    let (_, first, second) = cells[section as usize];
                    let tri = if attempt >= 2 {
                        second.unwrap_or(first)
                    } else {
                        first
                    };
                    let [a, b, c] = tri.map(|i| Vec3::from_array(vertices[i as usize].position));
                    let point = a * weights[0] + b * weights[1] + c * weights[2];
                    let normal = (b - a).cross(c - a).normalize_or_zero();
                    let extent = 0.2 * (attempt + 1) as f32;
                    for reverse in [false, true] {
                        let sign = if reverse { -1.0 } else { 1.0 };
                        hits.extend(self.trace_sail_shot(
                            state,
                            point - normal * extent * sign,
                            point + normal * extent * sign,
                            wind,
                            seconds,
                            rig,
                            ModelPose::default(),
                        )?);
                        if state
                            .sail_holes
                            .get(&index)
                            .is_some_and(|mask| mask & (1 << section) != 0)
                        {
                            break;
                        }
                    }
                }
            }
        }
        Ok(hits)
    }
}
#[cfg(test)]
mod tests;
