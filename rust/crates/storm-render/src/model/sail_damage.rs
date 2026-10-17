//! Persistent original 12/10-sector damage and swept cloth intersections.
//! The cutout shape is new; the original parus_hole texture/particle/combat code
//! is not reproduced. Shots penetrate cloth; hull/cannon battle logic is separate.
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
}

impl SailPatch {
    pub fn section_count(&self) -> u32 {
        if self.is_triangle() { 10 } else { 12 }
    }
    /// Original SSailTrace 3x4 / TSailTrace four triangular rows.
    pub fn damage_section(&self, uv: [f32; 2]) -> u32 {
        let p = Vec2::from_array(uv);
        if self.is_triangle() {
            let row = (p.x * 4.0).floor().clamp(0.0, 3.0) as u32;
            let col = ((p.x + p.y - 1.0) * 4.0).floor().clamp(0.0, row as f32) as u32;
            row * (row + 1) / 2 + col
        } else {
            (p.x * 3.0).floor().clamp(0.0, 2.0) as u32 * 4
                + (p.y * 4.0).floor().clamp(0.0, 3.0) as u32
        }
    }
    pub fn intact_fraction(&self, mask: u16) -> f64 {
        1.0 - f64::from((mask & ((1 << self.section_count()) - 1)).count_ones())
            / f64::from(self.section_count())
    }
    /// Must agree with sail_cutout in model.wgsl. In fabric UVs, never world space.
    pub fn cutout(&self, uv: [f32; 2], mask: u16) -> bool {
        let section = self.damage_section(uv);
        if mask & (1 << section) == 0 {
            return false;
        }
        let (x, y) = if self.is_triangle() {
            (uv[0] * 4.0, (uv[0] + uv[1] - 1.0) * 4.0)
        } else {
            (uv[0] * 3.0, uv[1] * 4.0)
        };
        let x = x - x.floor();
        let y = y - y.floor();
        let dx = (x - 0.5) / 0.42;
        let dy = (y - 0.5) / 0.42;
        let edge = 1.0 + 0.12 * (x * 43.0 + y * 31.0).sin() * (y * 37.0 - x * 29.0).sin();
        dx * dx + dy * dy < edge
    }
    pub(super) fn trace(
        &self,
        vertices: &[ModelVertex],
        from: Vec3,
        to: Vec3,
        mask: u16,
    ) -> Option<(f32, [f32; 2])> {
        let delta = to - from;
        self.indices()
            .chunks_exact(3)
            .filter_map(|tri| {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| &vertices[i as usize]);
                let va = Vec3::from_array(a.position);
                let ab = Vec3::from_array(b.position) - va;
                let ac = Vec3::from_array(c.position) - va;
                let p = delta.cross(ac);
                let det = ab.dot(p);
                if det.abs() < 1e-7 {
                    return None;
                }
                let s = from - va;
                let u = s.dot(p) / det;
                let q = s.cross(ab);
                let v = delta.dot(q) / det;
                let t = ac.dot(q) / det;
                if u < -1e-6 || v < -1e-6 || u + v > 1.0 + 1e-6 || !(0.0..=1.0).contains(&t) {
                    return None;
                }
                let uv = (Vec2::from_array(a.uv) * (1.0 - u - v)
                    + Vec2::from_array(b.uv) * u
                    + Vec2::from_array(c.uv) * v)
                    .to_array();
                (!self.cutout(uv, mask)).then_some((t, uv))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }
}

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
            let mut vertices = patch.vertices(local_wind, seconds)?;
            patch.deform_with_setting(
                &mut vertices,
                local_wind,
                seconds,
                rig.deployed(patch.tier) as f32,
            )?;
            let from_local = base.inverse().transform_point3(from);
            let to_local = base.inverse().transform_point3(to);
            let mask = state.sail_holes.get(&i).copied().unwrap_or(0);
            if let Some((fraction, uv)) = patch.trace(&vertices, from_local, to_local, mask) {
                let section = patch.damage_section(uv);
                let bit = 1 << section;
                let new_damage = mask & bit == 0;
                hits.push(SailImpact {
                    mesh: i,
                    section,
                    fraction,
                    uv,
                    position: from.lerp(to, fraction).to_array(),
                    new_damage,
                });
            }
        }
        hits.sort_by(|a, b| a.fraction.total_cmp(&b.fraction));
        for h in &hits {
            *state.sail_holes.entry(h.mesh).or_default() |= 1 << h.section;
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
        let mut shots = Vec::new();
        for mesh in &self.meshes {
            let Some(patch) = &mesh.sail else {
                continue;
            };
            let mut vertices = patch.vertices(wind, seconds)?;
            patch.deform_with_setting(
                &mut vertices,
                wind,
                seconds,
                rig.deployed(patch.tier) as f32,
            )?;
            let mut found = std::collections::BTreeSet::new();
            for tri in patch.indices().chunks_exact(3) {
                let [a, b, c] = [tri[0], tri[1], tri[2]].map(|i| vertices[i as usize]);
                let uv = [
                    (a.uv[0] + b.uv[0] + c.uv[0]) / 3.0,
                    (a.uv[1] + b.uv[1] + c.uv[1]) / 3.0,
                ];
                let section = patch.damage_section(uv);
                if section >= sections || !found.insert(section) {
                    continue;
                }
                let [a, b, c] = [a, b, c].map(|v| Vec3::from_array(v.position));
                let point = (a + b + c) / 3.0;
                let normal = (b - a).cross(c - a).normalize_or_zero();
                shots.push((point - normal * 0.2, point + normal * 0.2));
            }
        }
        let mut hits = Vec::new();
        for (a, b) in shots {
            hits.extend(self.trace_sail_shot(
                state,
                a,
                b,
                wind,
                seconds,
                rig,
                ModelPose::default(),
            )?);
        }
        Ok(hits)
    }
}
#[cfg(test)]
mod tests;
