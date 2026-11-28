use super::{
    Collector, LocatorEnd, LocatorRope, ModelVertex, PlacedLabel, RiggingStats, reflect_z,
};
use crate::model::{ModelAsset, NativeRigFrame, RigMesh};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use storm_sim::rope::{Geometry, Phase, Pose, Settings};

#[derive(Clone, Debug)]
pub struct NativeRope {
    pub locator: LocatorRope,
    pub rest: Pose,
    pub geometry: Geometry,
}

impl NativeRope {
    pub(super) fn deform(&self, out: &mut [ModelVertex], frame: Option<&NativeRigFrame>) {
        let pose = if let Some(frame) = frame {
            let Some(pose) = frame.ropes.get(&self.locator.number) else {
                for v in out {
                    v.position = [0.0; 3];
                }
                return;
            };
            *pose
        } else {
            self.rest
        };
        for (out, v) in out.iter_mut().zip(self.geometry.vertices(pose)) {
            *out = ModelVertex {
                position: reflect_z(v.position.to_array()),
                normal: reflect_z(v.normal.to_array()),
                uv: v.uv,
                color: [1.0; 4],
            };
        }
    }
}

impl ModelAsset {
    pub fn native_ropes(&self) -> &[NativeRope] {
        self.meshes
            .iter()
            .find_map(|mesh| match &mesh.rigging {
                Some(RigMesh::NativeRopes(ropes)) => Some(ropes.as_slice()),
                _ => None,
            })
            .unwrap_or_default()
    }
}

impl Collector {
    pub(crate) fn build_native_ropes(
        &self,
        settings: Settings,
    ) -> Result<(Vec<NativeRope>, RiggingStats)> {
        ensure!(self.labels.len() <= 50_000, "too many rigging locators");
        let mut stats = RiggingStats::default();
        let mut by_number = BTreeMap::new();
        let mut pairs = Vec::<(i32, [Option<&PlacedLabel>; 2], bool, bool)>::new();
        for label in &self.labels {
            let Some((number, side, stay, sag)) = parse_locator(&label.name) else {
                continue;
            };
            let index = *by_number.entry(number).or_insert_with(|| {
                let index = pairs.len();
                pairs.push((number, [None; 2], stay, sag));
                index
            });
            let (_, ends, thick, _) = &mut pairs[index];
            stats.duplicate_points += usize::from(ends[side].replace(label).is_some());
            *thick = stay;
        }
        ensure!(pairs.len() <= 4096, "too many native rope pairs");
        let mut ropes = Vec::new();
        for (number, ends, stay, sag) in pairs {
            let [Some(begin), Some(end)] = ends else {
                stats.unmatched_ropes += 1;
                continue;
            };
            let rest = Pose {
                ends: [begin, end].map(|label| reflect_z(label.position.to_array())),
                phase: Phase::default(),
            };
            let Some(geometry) = Geometry::new(rest.ends, stay, sag, settings)? else {
                stats.invalid_lengths += 1;
                continue;
            };
            ropes.push(NativeRope {
                locator: LocatorRope {
                    number,
                    ends: [begin, end].map(|label| LocatorEnd {
                        node: label.node.clone(),
                        group: label.group.clone(),
                        point: label.local,
                        frame: label.frame,
                    }),
                },
                rest,
                geometry,
            });
        }
        stats.ropes = ropes.len();
        Ok((ropes, stats))
    }
}

fn parse_locator(name: &str) -> Option<(i32, usize, bool, bool)> {
    let (tail, stay, sag) = if let Some(tail) = name.strip_prefix("rope") {
        (tail, false, true)
    } else if let Some(tail) = name.strip_prefix("fal") {
        (tail, false, false)
    } else {
        (name.strip_prefix("stay")?, true, false)
    };
    let side = match tail.as_bytes().first()? {
        b'b' => 0,
        b'e' => 1,
        _ => return None,
    };
    let number: i32 = tail[1..].parse().ok()?;
    let number = number.checked_add(if sag { 0 } else { 1000 })?;
    (number >= 0).then_some((number, side, stay, sag))
}
