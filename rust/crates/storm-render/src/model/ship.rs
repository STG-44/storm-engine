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
    pub point: usize,
    pub smoke: bool,
    pub started: f32,
    pub stopped: Option<f32>,
}
#[derive(Clone, Debug, Default)]
pub struct ShipAsset {
    pub deck_center: Option<Vec3>,
    pub nodes: Vec<ShipNode>,
    pub fires: Vec<FirePoint>,
    pub fire_tail_seconds: f32,
    pub crew_count: usize,
    pub waypoint_counts: Option<[usize; 2]>,
    pub waypoint_missing_links: usize,
    pub crew_source: Option<String>,
}
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
                emissions.len() <= asset.fires.len() * 2,
                "too many native fire emissions"
            );
            ensure!(
                emissions.iter().all(|e| e.point < asset.fires.len()
                    && e.started.is_finite()
                    && e.stopped.is_none_or(|t| t.is_finite() && t >= e.started)),
                "invalid native fire emission"
            );
        }
        Ok(())
    }
}
impl ShipAsset {
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
