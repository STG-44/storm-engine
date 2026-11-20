//! Paired-locator service adapter for the retained rope arithmetic.
use super::*;
use storm_render::model::LocatorRope;
use storm_sim::{
    sail_geometry::{Parameters, Sphere},
    sail_rope::{self, End, Limits, Links, Rope, Turning},
};
#[derive(Clone, Copy)]
struct Reference {
    rope: usize,
    side: usize,
}
struct Entry {
    links: Links,
    refs: [Option<Reference>; 4],
    parameters: Parameters,
    sphere: Sphere,
    turns: u32,
    rebuilds: u32,
    held: u32,
    queries: u64,
}
pub(super) struct Ropes {
    locators: Vec<LocatorRope>,
    entries: Vec<Entry>,
    duplicates: usize,
}
impl Ropes {
    pub fn new(asset: &ModelAsset, meshes: &[usize], sails: &mut [SailMotion]) -> Result<Self> {
        let mut seen = std::collections::BTreeSet::new();
        let mut duplicates = 0;
        let locators = asset
            .locator_ropes()
            .into_iter()
            .filter(|r| {
                let first = seen.insert(r.number);
                if !first {
                    duplicates += 1;
                }
                first
            })
            .collect();
        let entries = meshes
            .iter()
            .map(|&mesh| {
                let input = asset.meshes[mesh]
                    .sail
                    .as_ref()
                    .unwrap()
                    .native_wave_input(
                        asset.ship_state.sail_holes.get(&mesh).copied().unwrap_or(0),
                    )?;
                Ok(Entry {
                    links: Links::default(),
                    refs: [None; 4],
                    parameters: input.parameters,
                    sphere: input.sphere,
                    turns: 0,
                    rebuilds: 0,
                    held: 0,
                    queries: 0,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let mut out = Self {
            locators,
            entries,
            duplicates,
        };
        let nodes = BTreeMap::new();
        for i in 0..out.locators.len() {
            for side in 0..2 {
                let end = out.locators[i].ends[side].clone();
                let number = if side == 0 {
                    out.locators[i].number
                } else {
                    -out.locators[i].number
                };
                for (j, &mesh) in meshes.iter().enumerate() {
                    let patch = asset.meshes[mesh].sail.as_ref().unwrap();
                    if !patch
                        .origin
                        .as_ref()
                        .is_some_and(|o| o.node == end.node && o.group == end.group)
                    {
                        continue;
                    }
                    let turning = end.node.starts_with("rey_");
                    let slot = patch.rope_slot(end.point);
                    let opposite = if turning
                        && slot != 0
                        && out.entries[j].links.records.iter().any(Option::is_none)
                    {
                        let r = out.registry(&nodes, sails, None)?;
                        let mut point = [0.0; 3];
                        ensure!(
                            sail_rope::endpoint(&r, number, 1, Some(&mut point))?,
                            "missing paired native rope"
                        );
                        out.entries[j].queries += 1;
                        Some(point)
                    } else {
                        None
                    };
                    let tie = sail_rope::register(
                        &mut sails[j],
                        &mut out.entries[j].links,
                        turning,
                        end.point,
                        number,
                        opposite,
                    )?;
                    out.entries[j].refs[tie] = Some(Reference { rope: i, side });
                }
            }
        }
        let r = out.registry(&nodes, sails, None)?;
        for (j, &mesh) in meshes.iter().enumerate() {
            let patch = asset.meshes[mesh].sail.as_ref().unwrap();
            let Some(origin) = &patch.origin else {
                continue;
            };
            let e = &mut out.entries[j];
            if sails[j].kind == storm_sim::sail_drive::Kind::Triangle
                && origin.node.starts_with("rey_")
                && let Some(record) = e.links.records[0]
            {
                let mut point = [0.0; 3];
                ensure!(
                    sail_rope::endpoint(&r, record.number, 1, Some(&mut point))?,
                    "missing limit rope"
                );
                e.queries += 1;
                sail_rope::set_limits(
                    &mut sails[j],
                    &e.links,
                    true,
                    Some(Limits {
                        endpoint: point,
                        sphere_radius: e.sphere.radius,
                        parent_width: asset
                            .ship
                            .node_widths
                            .get(&origin.parent)
                            .copied()
                            .unwrap_or(0.0),
                        deep_h: e.parameters.deep_h,
                        deep_z: e.parameters.deep_z,
                        triangle_dependence: 0.5,
                        max_angle: 0.6,
                    }),
                )?;
            }
        }
        Ok(out)
    }
    fn registry(
        &self,
        nodes: &BTreeMap<String, [f32; 16]>,
        sails: &[SailMotion],
        current: Option<(usize, &SailMotion)>,
    ) -> Result<Vec<Rope>> {
        let mut ropes = self
            .locators
            .iter()
            .map(|r| {
                let [begin, end] = r.ends.each_ref().map(|e| {
                    let matrix = nodes.get(&e.node).map_or(e.frame, |m| {
                        std::array::from_fn(|i| {
                            let row = i % 4;
                            let col = i / 4 * 4;
                            m[row] * e.frame[col]
                                + m[row + 4] * e.frame[col + 1]
                                + m[row + 8] * e.frame[col + 2]
                                + m[row + 12] * e.frame[col + 3]
                        })
                    });
                    End {
                        point: e.point,
                        matrix,
                    }
                });
                Rope {
                    number: r.number,
                    model: 1,
                    begin,
                    end,
                }
            })
            .collect::<Vec<_>>();
        for (i, e) in self.entries.iter().enumerate() {
            let motion = current
                .filter(|(j, _)| *j == i)
                .map_or(&sails[i], |(_, m)| m);
            for (slot, reference) in e.refs.iter().enumerate() {
                if let Some(reference) = reference
                    && let Some(point) = motion.ties[slot]
                {
                    let rope = &mut ropes[reference.rope];
                    if reference.side == 0 {
                        rope.begin.point = point;
                    } else {
                        rope.end.point = point;
                    }
                }
            }
        }
        Ok(ropes)
    }
    pub fn geometry(&mut self, index: usize, p: Parameters, s: Sphere) -> Result<()> {
        let e = self
            .entries
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("unknown rope geometry"))?;
        e.parameters = p;
        e.sphere = s;
        Ok(())
    }
    pub fn input(&self, index: usize) -> (Parameters, Sphere) {
        let e = &self.entries[index];
        (e.parameters, e.sphere)
    }
    pub fn turn(
        &mut self,
        asset: &ModelAsset,
        sails: &mut [SailMotion],
        owners: &[Option<String>],
        yards: &mut BTreeMap<String, Yard>,
        outputs: &[SailOutput],
        delta_ms: u32,
    ) -> Result<()> {
        for j in 0..sails.len() {
            let Some(owner) = owners[j].as_ref().filter(|n| n.starts_with("rey_")) else {
                continue;
            };
            let mut yard = yards[owner].clone();
            let prior = sails.to_vec();
            let mut queries = 0;
            let result = sail_rope::turn(
                &mut sails[j],
                &self.entries[j].links,
                Turning {
                    wind_x: outputs[j].wind[0],
                    step: delta_ms as f32 * TURN_RATE,
                    discrete: TURN_DISCRETE,
                    geometry: self.entries[j].parameters,
                },
                &mut yard,
                Some(
                    |n: &Yard, current: &SailMotion, number, point: &mut [f32; 3]| {
                        let mut posed = yards.clone();
                        posed.insert(owner.clone(), n.clone());
                        let nodes = node_frames(&posed, &asset.ship)?;
                        let r = self.registry(&nodes, &prior, Some((j, current)))?;
                        queries += 1;
                        ensure!(
                            sail_rope::endpoint(&r, number, 1, Some(point))?,
                            "missing live rope endpoint"
                        );
                        Ok(())
                    },
                ),
            );
            let e = &mut self.entries[j];
            e.queries += queries;
            match result {
                Ok(out) => {
                    if out.moved {
                        e.turns += 1;
                    }
                    if let Some(s) = out.rebuilt {
                        e.sphere = s;
                        e.rebuilds += 1;
                    }
                    yards.insert(owner.clone(), yard);
                }
                Err(error) if error.is::<sail_rope::NativeNonFinite>() => e.held += 1,
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }
    pub fn report(&self, sails: &[SailMotion]) -> Json {
        json!({"provider":"paired_rope_and_fal_locators_static_frames_yard_adapter","native_registration_order":false,"native_matrix_accumulation":false,"duplicate_numbers_skipped":self.duplicates,"ropes":self.locators.len(),"sails":self.entries.iter().zip(sails).map(|(e,m)|json!({"limits_bits":[m.minimum_angle.to_bits(),m.maximum_angle.to_bits()],"hard_point_bits":m.hard_points.map(|p|p.map(f32::to_bits)),"geometry_bits":m.geometry.map(|p|p.map(f32::to_bits)),"transition":m.transition.map(|t|json!({"up":t.up,"delta_bits":t.delta.to_bits(),"geometry_bits":t.geometry.map(|p|p.map(f32::to_bits))})),"tie_bits":m.ties.map(|p|p.map(|p|p.map(f32::to_bits))),"sphere_center_bits":e.sphere.center.map(f32::to_bits),"sphere_radius_bits":e.sphere.radius.to_bits(),"records":e.links.records.map(|r|r.map(|r|json!({"number":r.number,"tie":r.tie,"r1_bits":r.r1.to_bits(),"r2_bits":r.r2.to_bits(),"b_bits":r.b.map(f32::to_bits)}))),"turns":e.turns,"rebuilds":e.rebuilds,"held_turns":e.held,"endpoint_queries":e.queries})).collect::<Vec<_>>()})
    }
}
