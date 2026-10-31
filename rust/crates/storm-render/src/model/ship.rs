//! Presentation state: original node ownership and explicit preview damage.
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct ShipNode {
    pub name: String,
    pub parent: String,
    pub pivot: Vec3,
}
#[derive(Clone, Debug)]
pub struct FirePoint {
    pub node: String,
    pub name: String,
    pub position: Vec3,
}
impl storm_sim::fire::FireLocator for FirePoint {
    fn fire_position(&self) -> [f32; 3] {
        self.position.to_array()
    }
}
#[derive(Clone, Debug)]
pub struct FireEmission {
    /// Stable service instance ID. Zero is reserved for a caller's legacy stream.
    pub instance: u64,
    pub point: usize,
    pub smoke: bool,
    pub started: f32,
    pub stopped: Option<f32>,
    /// Canonical XPS cache key; None is the generated/legacy diagnostic service.
    pub profile: Option<String>,
}
#[derive(Clone, Debug, Default)]
pub struct ParticleProjectInfo {
    pub source: String,
    pub texture: String,
    pub color_key: String,
    pub normal_key: String,
    pub normal_loaded: bool,
    pub declared: Vec<String>,
    pub available: Vec<String>,
    pub missing: Vec<String>,
}
#[derive(Clone, Debug, Default)]
pub struct ShipAsset {
    /// Static model-space node frames; commercial geometry stays external.
    pub node_frames: BTreeMap<String, [f32; 16]>,
    pub deck_center: Option<Vec3>,
    pub nodes: Vec<ShipNode>,
    pub fires: Vec<FirePoint>,
    pub fire_tail_seconds: f32,
    /// None uses generated/legacy particles; Some requires an exact cached XPS profile.
    pub particle_profiles: Option<std::collections::BTreeSet<String>>,
    pub particle_project: Option<ParticleProjectInfo>,
    pub particle_library: Option<std::sync::Arc<super::ParticleLibrary>>,
    pub particle_texture: Option<usize>,
    pub crew_count: usize,
    pub waypoint_counts: Option<[usize; 2]>,
    pub waypoint_missing_links: usize,
    pub crew_source: Option<String>,
}
pub const MAX_PARTICLE_INSTANCES: usize = 4096;
pub const MAX_PARTICLE_VERTICES: usize = 1_048_576;
pub fn part_number(name: &str, prefix: &str) -> Option<u32> {
    let digits = name
        .strip_prefix(prefix)?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}
/// SHIP::MastFall's mast/topmast numeric grouping.
pub fn mast_falls_with(broken: u32, candidate: u32) -> bool {
    let (broken, candidate) = (u64::from(broken), u64::from(candidate));
    candidate == broken
        || if broken < 100 {
            candidate > broken * 100 && candidate < (broken + 1) * 100
        } else {
            candidate > broken && candidate < (broken / 100 + 1) * 100
        }
}
#[derive(Clone, Debug)]
pub struct ShipState {
    pub native_rig: Option<std::sync::Arc<NativeRigFrame>>,
    /// Draw-mesh index -> original 10/12-sector bitmask.
    pub sail_holes: BTreeMap<usize, u16>,
    pub broken_masts: BTreeMap<u32, f32>,
    pub broken_hulls: BTreeMap<String, f32>,
    pub fire_started: Option<f32>,
    /// None keeps the manual preview; Some selects native particle emissions.
    pub native_fire: Option<Vec<FireEmission>>,
    pub particle_history: Option<std::sync::Arc<storm_sim::pose_history::PoseHistory>>,
    pub crew_visible: bool,
}
impl Default for ShipState {
    fn default() -> Self {
        Self {
            native_rig: None,
            sail_holes: BTreeMap::new(),
            broken_masts: BTreeMap::new(),
            broken_hulls: BTreeMap::new(),
            fire_started: None,
            native_fire: None,
            particle_history: None,
            crew_visible: true,
        }
    }
}
impl ShipState {
    pub fn validate(&self, asset: &ShipAsset) -> Result<()> {
        if let Some(frame) = &self.native_rig {
            ensure!(
                frame.sails.len() <= 4096 && frame.nodes.len() <= 256 && frame.origins.len() <= 256,
                "rig frame budget exceeded"
            );
            ensure!(
                frame
                    .sails
                    .values()
                    .chain(frame.origins.values().flat_map(|v| v.values()))
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v)),
                "invalid native rig deployment"
            );
            ensure!(
                frame.origins.values().map(|v| v.len()).sum::<usize>() <= 4096,
                "rig origin budget exceeded"
            );
            ensure!(
                frame
                    .nodes
                    .iter()
                    .all(|(name, m)| asset.nodes.iter().any(|n| &n.name == name)
                        && m.iter().all(|v| v.is_finite())
                        && m[3] == 0.0
                        && m[7] == 0.0
                        && m[11] == 0.0
                        && m[15] == 1.0),
                "invalid rig node transform"
            );
        }
        ensure!(
            self.broken_masts.len() <= 64 && self.broken_hulls.len() <= 256,
            "too many damaged parts"
        );
        ensure!(
            self.broken_masts.iter().all(|(id, t)| t.is_finite()
                && asset
                    .nodes
                    .iter()
                    .any(|n| part_number(&n.name, "mast") == Some(*id))),
            "unknown mast or invalid damage time"
        );
        ensure!(
            self.broken_hulls.iter().all(|(name, t)| t.is_finite()
                && part_number(name, "shatter").is_some()
                && asset.nodes.iter().any(|n| &n.name == name)),
            "unknown hull piece or invalid damage time"
        );
        ensure!(
            self.fire_started.is_none_or(f32::is_finite),
            "invalid fire time"
        );
        if let Some(emissions) = &self.native_fire {
            ensure!(
                emissions.len() <= MAX_PARTICLE_INSTANCES,
                "too many native fire emissions"
            );
            let mut instances = std::collections::BTreeSet::new();
            ensure!(
                emissions.iter().all(|e| e.point < asset.fires.len()
                    && (e.instance == 0 || instances.insert(e.instance))
                    && e.started.is_finite()
                    && e.stopped.is_none_or(|t| t.is_finite() && t >= e.started)
                    && e.profile
                        .as_ref()
                        .is_none_or(|name| super::particle_profile_key(name)
                            .is_ok_and(|key| key == *name)
                            && (asset
                                .particle_profiles
                                .as_ref()
                                .is_some_and(|cache| cache.contains(name))
                                || asset
                                    .particle_library
                                    .as_ref()
                                    .is_some_and(|library| library.contains(name))))),
                "invalid native fire emission"
            );
        }
        Ok(())
    }
}
impl ShipAsset {
    /// Viewer matrix provider. Scalar sine/cosine are supplied by the caller.
    /// This is not native CMatrix accumulation or rope-constrained turning.
    pub fn yard_delta(&self, name: &str, sine: f32, cosine: f32) -> Result<[f32; 16]> {
        ensure!(
            sine.is_finite() && cosine.is_finite(),
            "invalid yard rotation"
        );
        let node = self
            .nodes
            .iter()
            .find(|n| n.name == name)
            .ok_or_else(|| anyhow::anyhow!("unknown yard node"))?;
        let frame = |name: &str| {
            self.node_frames
                .get(name)
                .map(Mat4::from_cols_array)
                .or_else(|| {
                    self.nodes
                        .iter()
                        .find(|n| n.name == name)
                        .map(|n| Mat4::from_translation(n.pivot))
                })
                .unwrap_or(Mat4::IDENTITY)
        };
        let base = frame(name);
        let parent = frame(&node.parent);
        let inverse = parent.inverse();
        let local = inverse.transform_point3(base.w_axis.truncate());
        let rotation = Mat4::from_cols_array(&[
            cosine, 0.0, -sine, 0.0, 0.0, 1.0, 0.0, 0.0, sine, 0.0, cosine, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        let result = parent
            * Mat4::from_translation(local)
            * rotation
            * Mat4::from_translation(-local)
            * inverse;
        ensure!(result.is_finite(), "yard transform overflow");
        Ok(result.to_cols_array())
    }
    pub fn posed_nodes(&self, deltas: &BTreeMap<String, [f32; 16]>) -> BTreeMap<String, [f32; 16]> {
        let mut result = BTreeMap::new();
        for node in &self.nodes {
            let mut composed = None;
            for ancestor in self.ancestors(&node.name) {
                if let Some(delta) = deltas.get(&ancestor.name) {
                    let matrix = Mat4::from_cols_array(delta);
                    composed = Some(composed.map_or(matrix, |old| matrix * old));
                }
            }
            if let Some(matrix) = composed {
                result.insert(node.name.clone(), matrix.to_cols_array());
            }
        }
        result
    }
    pub fn posed_axes(axes: [[f32; 3]; 3], transform: [f32; 16]) -> [[f32; 3]; 3] {
        let m = Mat4::from_cols_array(&transform);
        axes.map(|v| m.transform_vector3(Vec3::from_array(v)).to_array())
    }
    pub fn masts(&self) -> Vec<u32> {
        self.nodes
            .iter()
            .filter_map(|n| part_number(&n.name, "mast"))
            .collect()
    }
    pub fn hulls(&self) -> Vec<String> {
        self.nodes
            .iter()
            .filter(|n| part_number(&n.name, "shatter").is_some())
            .map(|n| n.name.clone())
            .collect()
    }
    fn ancestors<'a>(&'a self, node: &'a str) -> Vec<&'a ShipNode> {
        let mut result = Vec::new();
        let mut current = node;
        for _ in 0..16 {
            let Some(n) = self.nodes.iter().find(|n| n.name == current) else {
                break;
            };
            result.push(n);
            if n.parent.is_empty() {
                break;
            }
            current = &n.parent;
        }
        result
    }
    pub fn fallen_mast(&self, node: &str, state: &ShipState) -> Option<(u32, f32, Vec3)> {
        for ancestor in self.ancestors(node) {
            if let Some(candidate) = part_number(&ancestor.name, "mast") {
                for (&broken, &time) in &state.broken_masts {
                    if mast_falls_with(broken, candidate) {
                        let pivot = self
                            .nodes
                            .iter()
                            .find(|n| part_number(&n.name, "mast") == Some(broken))
                            .map_or(ancestor.pivot, |n| n.pivot);
                        return Some((broken, time, pivot));
                    }
                }
            }
        }
        None
    }
    pub fn hidden(&self, node: &str, state: &ShipState, seconds: f32) -> bool {
        self.ancestors(node).iter().any(|n| {
            state
                .broken_hulls
                .get(&n.name)
                .is_some_and(|t| seconds >= *t)
        })
    }
    pub fn transform(&self, node: &str, state: &ShipState, seconds: f32) -> Option<Mat4> {
        let fall = self.fall_transform(node, state, seconds);
        match state.native_rig.as_ref().and_then(|r| r.nodes.get(node)) {
            Some(m) => {
                let motion = Mat4::from_cols_array(m);
                Some(fall.map_or(motion, |f| f * motion))
            }
            None => fall,
        }
    }
    pub(super) fn fall_transform(
        &self,
        node: &str,
        state: &ShipState,
        seconds: f32,
    ) -> Option<Mat4> {
        let (_, start, pivot) = self.fallen_mast(node, state)?;
        let age = (seconds - start).max(0.0);
        let angle = (age * age * 0.22).min(1.65);
        let sink = (age - 2.5).max(0.0).powi(2) * 1.4;
        Some(
            Mat4::from_translation(pivot - Vec3::Y * sink)
                * Mat4::from_rotation_z(-angle)
                * Mat4::from_translation(-pivot),
        )
    }
}

#[derive(Clone, Debug, Default)]
pub struct NativeRigFrame {
    pub sails: BTreeMap<usize, f32>,
    pub origins: BTreeMap<String, BTreeMap<String, f32>>,
    /// Model-space deltas including inherited yard motion for each affected node.
    pub nodes: BTreeMap<String, [f32; 16]>,
}
impl NativeRigFrame {
    pub fn deployment(&self, patch: &super::SailPatch) -> Option<f32> {
        let origin = patch.origin.as_ref()?;
        self.origins.get(&origin.node)?.get(&origin.group).copied()
    }
    pub fn point(&self, node: Option<&str>, point: Vec3) -> Vec3 {
        node.and_then(|n| self.nodes.get(n))
            .map_or(point, |m| Mat4::from_cols_array(m).transform_point3(point))
    }
}

#[cfg(test)]
mod motion_tests {
    use super::*;
    #[test]
    fn yard_rotation_uses_parent_axes_and_inherits_without_rotating_the_pivot() {
        let parent = Mat4::from_translation(Vec3::new(3.0, 4.0, 5.0)) * Mat4::from_rotation_x(0.5);
        let child = parent * Mat4::from_translation(Vec3::new(0.0, 3.0, 2.0));
        let asset = ShipAsset {
            nodes: vec![
                ShipNode {
                    name: "mast".into(),
                    parent: String::new(),
                    pivot: parent.w_axis.truncate(),
                },
                ShipNode {
                    name: "rey_a".into(),
                    parent: "mast".into(),
                    pivot: child.w_axis.truncate(),
                },
                ShipNode {
                    name: "tip".into(),
                    parent: "rey_a".into(),
                    pivot: child.transform_point3(Vec3::X),
                },
            ],
            node_frames: BTreeMap::from([
                ("mast".into(), parent.to_cols_array()),
                ("rey_a".into(), child.to_cols_array()),
            ]),
            ..Default::default()
        };
        let delta = asset.yard_delta("rey_a", 1.0, 0.0).unwrap();
        let m = Mat4::from_cols_array(&delta);
        let pivot = child.w_axis.truncate();
        assert!(m.transform_point3(pivot).distance(pivot) < 0.00001);
        assert!(
            m.transform_point3(child.transform_point3(Vec3::X))
                .distance(child.transform_point3(-Vec3::Z))
                < 0.00001
        );
        let inherited = asset.posed_nodes(&BTreeMap::from([("rey_a".into(), delta)]));
        assert_eq!(inherited["rey_a"], inherited["tip"]);
        assert!(!inherited.contains_key("mast"));
        let mut state = ShipState {
            native_rig: Some(std::sync::Arc::new(NativeRigFrame {
                nodes: inherited,
                ..Default::default()
            })),
            ..Default::default()
        };
        state.validate(&asset).unwrap();
        assert_eq!(
            state.native_rig.as_ref().unwrap().nodes["tip"],
            state.native_rig.as_ref().unwrap().nodes["rey_a"]
        );
        std::sync::Arc::make_mut(state.native_rig.as_mut().unwrap())
            .sails
            .insert(0, 1.01);
        assert!(state.validate(&asset).is_err());
    }
}
