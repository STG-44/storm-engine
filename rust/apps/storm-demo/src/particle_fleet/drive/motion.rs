//! Node/cloth adapters for retained sail timing and optional rope constraints.
mod ropes;
use super::*;
use std::collections::BTreeMap;
use storm_render::model::{NativeRigFrame, ShipAsset};
use storm_sim::sail_motion::{NodePort, SailMotion};

const ROLL_RATE: f32 = 0.0003;
const TURN_RATE: f32 = 0.002;
const TURN_DISCRETE: f32 = 0.01;

#[derive(Clone, Default)]
struct Yard {
    position: [f32; 3],
    local: f32,
    global: f32,
}
impl NodePort for Yard {
    fn position(&mut self) -> [f32; 3] {
        self.position
    }
    fn set_position(&mut self, value: [f32; 3]) {
        self.position = value;
    }
    fn rotate_y(&mut self, angle: f32) {
        self.local += angle;
    }
    fn update_global(&mut self) {
        self.global = self.local;
    }
}
pub(super) struct Motion {
    sails: Vec<SailMotion>,
    free: Vec<bool>,
    owners: Vec<Option<String>>,
    yards: BTreeMap<String, Yard>,
    ropes: Option<ropes::Ropes>,
}
impl Motion {
    pub fn set_free(&mut self, index: usize) -> Result<()> {
        *self
            .free
            .get_mut(index)
            .ok_or_else(|| anyhow::anyhow!("unknown native free sail"))? = true;
        Ok(())
    }
    pub fn sync_zero_cloth_geometry(&mut self) -> Result<()> {
        // A newly issued unroll request has delta zero but still holds the
        // previous coefficients. Apply the native zero-ms step before drawing.
        for (sail, free) in self.sails.iter_mut().zip(&self.free) {
            if !free && sail.transition.is_some_and(|t| t.delta == 0.0) {
                sail.advance(0, ROLL_RATE)?;
            }
        }
        Ok(())
    }
    pub fn cloth_state(&self, index: usize) -> Result<SailMotion> {
        self.sails
            .get(index)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("unknown native cloth sail"))
    }
    pub fn rebuild_cloth(
        &mut self,
        index: usize,
        p: storm_sim::sail_geometry::Parameters,
    ) -> Result<()> {
        let sphere = storm_sim::sail_geometry::rebuild(
            self.sails
                .get_mut(index)
                .ok_or_else(|| anyhow::anyhow!("unknown native cloth sail"))?,
            p,
        )?;
        if let Some(ropes) = &mut self.ropes {
            ropes.geometry(index, p, sphere)?;
        }
        Ok(())
    }
    pub fn new(asset: &ModelAsset, meshes: &[usize], group: &Group) -> Result<Self> {
        let mut yards = BTreeMap::new();
        let mut owners = Vec::new();
        let sails = meshes
            .iter()
            .zip(&group.sails)
            .map(|(&mesh, input)| {
                let patch = asset.meshes[mesh].sail.as_ref().unwrap();
                let owner = patch.origin.as_ref().map(|o| o.node.clone());
                if let Some(name) = &owner {
                    yards.entry(name.clone()).or_insert_with(|| Yard {
                        position: asset
                            .ship
                            .nodes
                            .iter()
                            .find(|n| &n.name == name)
                            .map_or([0.0; 3], |n| n.pivot.to_array()),
                        ..Default::default()
                    });
                }
                owners.push(owner);
                let (mut motion, _sphere) = patch.native_geometry(
                    asset.ship_state.sail_holes.get(&mesh).copied().unwrap_or(0),
                )?;
                motion.rolling = input.rolling;
                Ok(motion)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            free: vec![false; sails.len()],
            sails,
            owners,
            yards,
            ropes: None,
        })
    }
    fn copy_state(&self, group: &mut Group) {
        for (s, m) in group.sails.iter_mut().zip(&self.sails) {
            s.rolling = m.rolling;
            s.rolling_delta = m.transition.map(|t| t.delta);
        }
    }
    fn nodes(&self, asset: &ShipAsset) -> Result<BTreeMap<String, [f32; 16]>> {
        node_frames(&self.yards, asset)
    }
    pub fn enable_ropes(&mut self, asset: &ModelAsset, meshes: &[usize]) -> Result<()> {
        self.ropes = Some(ropes::Ropes::new(asset, meshes, &mut self.sails)?);
        Ok(())
    }
    pub fn rope_enabled(&self) -> bool {
        self.ropes.is_some()
    }
    pub fn cloth_rope_input(
        &self,
        index: usize,
    ) -> Option<(
        storm_sim::sail_geometry::Parameters,
        storm_sim::sail_geometry::Sphere,
    )> {
        self.ropes.as_ref().map(|r| r.input(index))
    }
    pub fn rope_outputs(&mut self, index: usize, points: [[f32; 3]; 4]) -> Result<()> {
        if self.ropes.is_some() {
            let s = &mut self.sails[index];
            for (i, p) in points.into_iter().enumerate() {
                if s.ties[i].is_some() {
                    ensure!(
                        p.into_iter().all(f32::is_finite),
                        "invalid cloth rope output"
                    );
                    s.ties[i] = Some(p);
                }
            }
        }
        Ok(())
    }
}
fn node_frames(
    yards: &BTreeMap<String, Yard>,
    asset: &ShipAsset,
) -> Result<BTreeMap<String, [f32; 16]>> {
    let mut deltas = BTreeMap::new();
    for (name, yard) in yards {
        if yard.global != 0.0 {
            deltas.insert(
                name.clone(),
                asset.yard_delta(name, libm::sinf(yard.global), libm::cosf(yard.global))?,
            );
        }
    }
    Ok(asset.posed_nodes(&deltas))
}
impl Motion {
    pub fn advance(
        &mut self,
        asset: &ModelAsset,
        meshes: &[usize],
        group: &mut Group,
        delta_ms: u32,
    ) -> Result<()> {
        // Explicit viewer scheduling: publish local changes once per frame.
        // Native SetRolling itself still emits no UpdateGlobalMatrix call.
        for yard in self.yards.values_mut() {
            yard.global = yard.local;
        }
        for (sail, free) in self.sails.iter_mut().zip(&self.free) {
            if !free {
                sail.advance(delta_ms, ROLL_RATE)?;
            }
        }
        self.copy_state(group);
        let nodes = self.nodes(&asset.ship)?;
        for ((sail, owner), &mesh) in group.sails.iter_mut().zip(&self.owners).zip(meshes) {
            let base = asset.meshes[mesh]
                .sail
                .as_ref()
                .unwrap()
                .native_drive(0)
                .axes;
            sail.axes = owner
                .as_ref()
                .and_then(|n| nodes.get(n))
                .map_or(base, |m| ShipAsset::posed_axes(base, *m));
        }
        Ok(())
    }
    pub fn turn(
        &mut self,
        asset: &ModelAsset,
        outputs: &[SailOutput],
        delta_ms: u32,
    ) -> Result<()> {
        if let Some(r) = &mut self.ropes {
            return r.turn(
                asset,
                &mut self.sails,
                &self.owners,
                &mut self.yards,
                outputs,
                delta_ms,
            );
        }
        for ((motion, owner), output) in self.sails.iter_mut().zip(&self.owners).zip(outputs) {
            if let Some(owner) = owner.as_ref().filter(|n| n.starts_with("rey_")) {
                motion.turn(
                    output.wind[0],
                    delta_ms as f32 * TURN_RATE,
                    TURN_DISCRETE,
                    self.yards.get_mut(owner).unwrap(),
                )?;
            }
        }
        Ok(())
    }
    pub fn command(&mut self, index: usize, up: bool, group: &mut Group) -> Result<()> {
        // Models without a node still have native rolling; their node port is inert.
        let mut inert = Yard::default();
        let yard = self.owners[index]
            .as_ref()
            .and_then(|n| self.yards.get_mut(n))
            .unwrap_or(&mut inert);
        self.sails[index].set_rolling(up, TURN_RATE, yard)?;
        self.copy_state(group);
        Ok(())
    }
    pub fn publish(&self, asset: &mut ModelAsset, meshes: &[usize]) -> Result<()> {
        let mut frame = NativeRigFrame {
            nodes: self.nodes(&asset.ship)?,
            ..Default::default()
        };
        for (&mesh, motion) in meshes.iter().zip(&self.sails) {
            let deployed = motion.deployment();
            frame.sails.insert(mesh, deployed);
            if let Some(origin) = &asset.meshes[mesh].sail.as_ref().unwrap().origin {
                if self.ropes.is_some() {
                    frame
                        .rope_points
                        .entry(origin.node.clone())
                        .or_default()
                        .insert(origin.group.clone(), motion.ties);
                }
                frame
                    .origins
                    .entry(origin.node.clone())
                    .or_default()
                    .insert(origin.group.clone(), deployed);
            }
        }
        asset.ship_state.native_rig = Some(Arc::new(frame));
        asset.ship_state.validate(&asset.ship)
    }
    pub fn report(&self) -> Json {
        let mut out = json!({"sails":self.sails.iter().map(|m|json!({"rolled":m.rolling,"deployment_bits":m.deployment().to_bits(),"old_wind_angle_bits":m.old_wind_angle.to_bits(),"transition":m.transition.map(|t|json!({"up":t.up,"delta_bits":t.delta.to_bits()}))})).collect::<Vec<_>>(),"nodes":self.yards.iter().map(|(name,y)|json!({"name":name,"local_angle_bits":y.local.to_bits(),"global_angle_bits":y.global.to_bits()})).collect::<Vec<_>>()});
        if let Some(r) = &self.ropes {
            out["rope_constraints"] = r.report(&self.sails);
        }
        out
    }
}
