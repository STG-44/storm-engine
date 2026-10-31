//! Full attached-grid diagnostic. Initialization, weather, ties and scheduling
//! are explicit providers; native free/rolled processors are not selected here.
use super::*;
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
}
pub(super) struct ClothFleet {
    bodies: Vec<Vec<Cloth>>,
    table: Vec<f32>,
}
impl ClothFleet {
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
                });
            }
            bodies.push(cloth);
        }
        let result = Self {
            bodies,
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
        drive: &drive::Drive,
        visible: [bool; 2],
        delta_ms: u32,
    ) -> Result<()> {
        for (i, body) in self.bodies.iter_mut().enumerate() {
            if !visible[i] {
                continue;
            }
            ensure!(
                body.len() == drive.outputs[i].len(),
                "wave/drive sail order mismatch"
            );
            for ((cloth, output), sail) in body
                .iter_mut()
                .zip(&drive.outputs[i])
                .zip(&drive.groups[i].sails)
            {
                ensure!(
                    !sail.rolling && sail.rolling_delta.is_none(),
                    "wave inspection requires settled full sails"
                );
                let mask = assets[i]
                    .ship_state
                    .sail_holes
                    .get(&cloth.mesh)
                    .copied()
                    .unwrap_or(0);
                if cloth.mask != mask {
                    let patch = assets[i].meshes[cloth.mesh].sail.as_ref().unwrap();
                    cloth.input = patch.native_wave_input(mask)?;
                    sail_geometry::rebuild(&mut cloth.motion, cloth.input.parameters)?;
                    cloth.mask = mask;
                }
                cloth.input.local_wind = output.wind;
                sail_wave::advance(
                    &mut cloth.state,
                    &cloth.motion,
                    &mut cloth.vertices,
                    cloth.input,
                    &self.table,
                    Settings::default(),
                    delta_ms,
                )?;
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
            asset.ship_state.validate(&asset.ship)?;
        }
        Ok(())
    }
    pub fn report(&self) -> Json {
        json!({"mode":"attached_full_grid","native_attached_waves":true,"native_rolled_vertices":false,
            "initialization":"fixed_zero_phase_zero_sum_no_ties","weather":"fixed_base_0.4_drive_local_wind",
            "wind_table":"60_pinned_sine_samples","table_bits":self.table.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
            "bodies":self.bodies.iter().map(|b|json!({"sails":b.iter().map(|c|json!({"mesh":c.mesh,"kind":c.motion.kind as i32,"frames":c.frames,"phase":[c.state.vertical,c.state.horizontal],"wind_up":c.state.wind_up,"sum_bits":c.state.sum_wind.to_bits(),"maximum_bits":c.state.maximum_sum_wind.to_bits(),"local_wind_bits":c.input.local_wind.map(f32::to_bits),"point_bits":c.state.points.map(|v|v.map(f32::to_bits)),"rope_bits":c.state.rope_points.map(|v|v.map(f32::to_bits)),"box_center_bits":c.state.box_center.map(f32::to_bits),"box_size_bits":c.state.box_size.map(f32::to_bits),"vertex_bits":c.vertices.iter().map(|v|v.position.into_iter().chain(v.normal).chain(v.uv.into_iter().flatten()).map(f32::to_bits).collect::<Vec<_>>()).collect::<Vec<_>>()})).collect::<Vec<_>>()})).collect::<Vec<_>>()})
    }
}
