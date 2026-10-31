//! Native cloth diagnostics. Initialization, weather, ties, falling mast motion
//! and scheduling remain explicit providers.
use super::*;
mod free;
use storm_sim::{
    sail_geometry::{self, Vertex},
    sail_motion::SailMotion,
    sail_wave::{self, Input, Settings, State},
};
struct Cloth {
    mesh: usize,
    motion: SailMotion,
    input: Input,
    state: State,
    vertices: Vec<Vertex>,
    frames: u32,
    mask: u16,
    topology: Option<(bool, bool, u16, storm_sim::sail_topology::Topology)>,
    free: free::Entry,
}
pub(super) struct ClothFleet {
    bodies: Vec<Vec<Cloth>>,
    table: Vec<f32>,
    rolls: bool,
    free_mode: Option<free::Schedule>,
}
impl ClothFleet {
    pub fn with_rolls(assets: &mut [ModelAsset]) -> Result<Self> {
        let mut result = Self::new(assets)?;
        result.rolls = true;
        Ok(result)
    }
    pub fn rolls(&self) -> bool {
        self.rolls
    }
    /// Native SetGeometry must update the saved transition before DoRollingStep.
    pub fn prepare(
        &mut self,
        assets: &mut [ModelAsset],
        drive: &mut drive::Drive,
        visible: [bool; 2],
        seconds: f32,
    ) -> Result<()> {
        self.prepare_free(assets, drive, visible, seconds)?;
        if !self.rolls {
            return Ok(());
        }
        for (i, body) in self.bodies.iter().enumerate() {
            if !visible[i] {
                continue;
            }
            for (j, cloth) in body.iter().enumerate() {
                let mask = assets[i]
                    .ship_state
                    .sail_holes
                    .get(&cloth.mesh)
                    .copied()
                    .unwrap_or(0);
                if mask != cloth.mask {
                    drive.rebuild_cloth(
                        i,
                        j,
                        assets[i].meshes[cloth.mesh]
                            .sail
                            .as_ref()
                            .unwrap()
                            .native_wave_input(mask)?
                            .parameters,
                    )?;
                }
            }
        }
        Ok(())
    }
    pub fn new(assets: &mut [ModelAsset]) -> Result<Self> {
        let mut bodies = Vec::new();
        for asset in assets.iter() {
            let mut cloth = Vec::new();
            for (mesh, patch) in asset
                .meshes
                .iter()
                .enumerate()
                .filter_map(|(i, m)| m.sail.as_ref().map(|s| (i, s)))
            {
                let mask = asset.ship_state.sail_holes.get(&mesh).copied().unwrap_or(0);
                let (motion, sphere) = patch.native_geometry(mask)?;
                let input = patch.native_wave_input(mask)?;
                cloth.push(Cloth {
                    mesh,
                    vertices: sail_geometry::fill(&motion, sphere, [1.0; 2])?,
                    motion,
                    state: State {
                        maximum_sum_wind: input.height * 0.02,
                        ..Default::default()
                    },
                    input,
                    frames: 0,
                    mask,
                    topology: None,
                    free: Default::default(),
                });
            }
            bodies.push(cloth);
        }
        let result = Self {
            bodies,
            rolls: false,
            free_mode: None,
            table: (0..60)
                .map(|i| libm::sinf(i as f32 / 60.0 * 2.0 * std::f32::consts::PI))
                .collect(),
        };
        result.publish(assets)?;
        Ok(result)
    }
    pub fn advance(
        &mut self,
        assets: &mut [ModelAsset],
        drive: &mut drive::Drive,
        visible: [bool; 2],
        delta_ms: u32,
        poses: &[ModelPose],
        seconds: f32,
    ) -> Result<()> {
        for (i, body) in self.bodies.iter_mut().enumerate() {
            if !visible[i] {
                continue;
            }
            if self.free_mode.is_some() {
                free::pin(&mut assets[i])?;
            }
            if self.rolls {
                drive.sync_cloth_geometry(i)?;
            }
            ensure!(
                body.len() == drive.outputs[i].len(),
                "wave/drive sail order mismatch"
            );
            for (j, ((cloth, output), sail)) in body
                .iter_mut()
                .zip(&drive.outputs[i])
                .zip(&drive.groups[i].sails)
                .enumerate()
            {
                ensure!(
                    self.rolls || (!sail.rolling && sail.rolling_delta.is_none()),
                    "wave inspection requires settled full sails"
                );
                let mask = assets[i]
                    .ship_state
                    .sail_holes
                    .get(&cloth.mesh)
                    .copied()
                    .unwrap_or(0);
                if self.rolls {
                    cloth.motion = drive.cloth_state(i, j)?;
                }
                if cloth.mask != mask {
                    let patch = assets[i].meshes[cloth.mesh].sail.as_ref().unwrap();
                    cloth.input = patch.native_wave_input(mask)?;
                    if !self.rolls {
                        sail_geometry::rebuild(&mut cloth.motion, cloth.input.parameters)?;
                    }
                    cloth.mask = mask;
                }
                cloth.input.local_wind = output.wind;
                if self.rolls && cloth.motion.rolling {
                    let patch = assets[i].meshes[cloth.mesh].sail.as_ref().unwrap();
                    storm_sim::sail_roll::advance(
                        &mut cloth.state,
                        &cloth.motion,
                        &mut cloth.vertices,
                        storm_sim::sail_roll::Parameters {
                            height: cloth.input.height,
                            turning: patch
                                .origin
                                .as_ref()
                                .is_some_and(|o| o.node.starts_with("rey_")),
                            ..Default::default()
                        },
                        &self.table,
                        if cloth.motion.kind == storm_sim::sail_drive::Kind::Triangle {
                            3
                        } else {
                            6
                        },
                    )?;
                } else if cloth.free.released {
                    free::advance(cloth, &assets[i], poses[i], seconds)?;
                } else {
                    sail_wave::advance(
                        &mut cloth.state,
                        &cloth.motion,
                        &mut cloth.vertices,
                        cloth.input,
                        &self.table,
                        Settings::default(),
                        delta_ms,
                    )?;
                }
                if self.rolls
                    && cloth
                        .topology
                        .as_ref()
                        .is_none_or(|(rolled, released, old_mask, _)| {
                            *rolled != cloth.motion.rolling
                                || *released != cloth.free.released
                                || *old_mask != mask
                        })
                {
                    use storm_sim::sail_topology::{self, Grid};
                    let grid = Grid::select(0.0, cloth.motion.rolling, cloth.free.released)?;
                    cloth.topology = Some((
                        cloth.motion.rolling,
                        cloth.free.released,
                        mask,
                        sail_topology::fill(cloth.motion.kind, cloth.motion.rolling, grid, mask)?,
                    ));
                }
                cloth.frames += 1;
            }
        }
        self.publish(assets)
    }
    fn publish(&self, assets: &mut [ModelAsset]) -> Result<()> {
        for (asset, body) in assets.iter_mut().zip(&self.bodies) {
            let frame = Arc::make_mut(
                asset
                    .ship_state
                    .native_rig
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("wave frame requires native motion"))?,
            );
            frame.cloth = body.iter().map(|c| (c.mesh, c.vertices.clone())).collect();
            frame.cloth_indices = body
                .iter()
                .filter_map(|c| c.topology.as_ref().map(|(_, _, _, t)| (c.mesh, t.clone())))
                .collect();
            frame.pinned_cloth_fall = self.free_mode.is_some();
            asset.ship_state.validate(&asset.ship)?;
        }
        Ok(())
    }
    pub fn report(&self) -> Json {
        let mut report = json!({"mode":"attached_full_grid","native_attached_waves":true,"native_rolled_vertices":false,
            "initialization":"fixed_zero_phase_zero_sum_no_ties","weather":"fixed_base_0.4_drive_local_wind",
            "wind_table":"60_pinned_sine_samples","table_bits":self.table.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
            "bodies":self.bodies.iter().map(|b|json!({"sails":b.iter().map(|c|json!({"mesh":c.mesh,"kind":c.motion.kind as i32,"frames":c.frames,"phase":[c.state.vertical,c.state.horizontal],"wind_up":c.state.wind_up,"sum_bits":c.state.sum_wind.to_bits(),"maximum_bits":c.state.maximum_sum_wind.to_bits(),"local_wind_bits":c.input.local_wind.map(f32::to_bits),"point_bits":c.state.points.map(|v|v.map(f32::to_bits)),"rope_bits":c.state.rope_points.map(|v|v.map(f32::to_bits)),"box_center_bits":c.state.box_center.map(f32::to_bits),"box_size_bits":c.state.box_size.map(f32::to_bits),"vertex_bits":c.vertices.iter().map(|v|v.position.into_iter().chain(v.normal).chain(v.uv.into_iter().flatten()).map(f32::to_bits).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()});
        if self.rolls {
            report["mode"] = json!("attached_and_rolled");
            report["native_rolled_vertices"] = json!(true);
            report["grid_provider"] = json!("near_attached_native_rolled");
            for (i, body) in self.bodies.iter().enumerate() {
                for (j, c) in body.iter().enumerate() {
                    let out = &mut report["bodies"][i]["sails"][j];
                    out["rolled"] = json!(c.motion.rolling);
                    out["geometry_bits"] = json!(c.motion.geometry.map(|v| v.map(f32::to_bits)));
                    out["transition"]=json!(c.motion.transition.map(|t|json!({"delta_bits":t.delta.to_bits(),"up":t.up,"geometry_bits":t.geometry.map(|v|v.map(f32::to_bits))})));
                    out["topology"]=json!(c.topology.as_ref().map(|(rolled,released,_,t)|json!({"grid":if *rolled {[5,13]}else if *released {[5,4]}else{[17,13]},"indices":t.indices,"normal_count":t.normal_count,"hole_start":t.hole_start,"hole_count":t.hole_count})));
                }
            }
        }
        free::report(self, &mut report);
        report
    }
}
