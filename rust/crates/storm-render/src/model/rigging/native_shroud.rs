use super::{Collector, ModelVertex, PlacedLabel, RiggingStats, reflect_z, shroud_locator};
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use storm_sim::shroud::{Geometry, Kind, Settings};

#[derive(Clone, Debug)]
pub struct NativeShroud {
    pub number: u32,
    pub mast_node: String,
    pub geometry: Geometry,
}

impl NativeShroud {
    pub(super) fn write_vertices(&self, vertices: &mut [ModelVertex]) {
        for (vertex, source) in vertices.iter_mut().zip(self.geometry.vertices()) {
            *vertex = ModelVertex {
                position: reflect_z(source.position.to_array()),
                normal: [0.0; 3],
                uv: source.uv,
                color: [1.0; 4],
            };
        }
    }
}

impl Collector {
    pub(in crate::model) fn build_native_shrouds(
        &self,
        kind: Kind,
        settings: Settings,
    ) -> Result<(Vec<NativeShroud>, RiggingStats)> {
        ensure!(self.labels.len() <= 50_000, "too many rigging locators");
        let mut stats = RiggingStats::default();
        let mut groups = BTreeMap::<u32, [Option<&PlacedLabel>; 3]>::new();
        for label in &self.labels {
            if let Some((number, side)) = shroud_locator(&label.name, kind.locator_prefix()) {
                let group = groups.entry(number).or_insert([None; 3]);
                stats.duplicate_points += usize::from(group[side].replace(label).is_some());
            }
        }
        ensure!(groups.len() <= 4096, "too many native shrouds");
        let mut shrouds = Vec::new();
        for (number, anchors) in groups {
            let [Some(upper), Some(left), Some(right)] = anchors else {
                stats.incomplete_shrouds += 1;
                continue;
            };
            let labels = [upper, left, right];
            let anchors = labels.map(|label| reflect_z(label.position.to_array()));
            let Some(geometry) = Geometry::new(anchors, labels.map(|label| label.local), settings)?
            else {
                stats.invalid_lengths += 1;
                continue;
            };
            shrouds.push(NativeShroud {
                number,
                mast_node: upper.node.clone(),
                geometry,
            });
        }
        stats.shrouds = shrouds.len();
        Ok((shrouds, stats))
    }
}
