//! Camera presentation adapter. Canonical near-grid simulation is unchanged.
use super::*;
use storm_sim::{
    sail_lod::{Camera, Selection},
    sail_topology::{self, Topology},
};
struct Replay {
    state: State,
    vertices: Vec<Vertex>,
    delta: u32,
}
struct Draw {
    vertices: Vec<Vertex>,
    topology: Topology,
    state: State,
}
#[derive(Default)]
pub(super) struct Entry {
    selection: Selection,
    replay: Option<Replay>,
    draw: Option<Draw>,
    frame: Option<u32>,
    changes: u32,
    view: Option<Camera>,
    matrix: Option<[f32; 16]>,
    distance: Option<f32>,
}
impl Entry {
    pub fn save(
        &mut self,
        rolled: bool,
        free: bool,
        state: &State,
        vertices: &[Vertex],
        delta: u32,
    ) {
        self.replay = if rolled || free {
            None
        } else {
            Some(Replay {
                state: state.clone(),
                vertices: vertices.to_vec(),
                delta,
            })
        };
    }
}
fn multiply(a: [f32; 16], b: [f32; 16]) -> [f32; 16] {
    std::array::from_fn(|i| {
        let r = i % 4;
        let c = i / 4 * 4;
        a[r] * b[c] + a[r + 4] * b[c + 1] + a[r + 8] * b[c + 2] + a[r + 12] * b[c + 3]
    })
}
impl ClothFleet {
    pub fn enable_lod(&mut self, distance: Option<f32>) -> Result<()> {
        ensure!(
            self.rolls
                && distance.is_none_or(|d| d.is_finite() && (0.1..=1_000_000.0).contains(&d)),
            "LOD requires native rolls and a bounded positive camera distance"
        );
        self.lod_distance = Some(distance);
        Ok(())
    }
    pub fn lod_enabled(&self) -> bool {
        self.lod_distance.is_some()
    }
    pub fn configure_camera(&self, camera: &mut OrbitCamera) {
        if let Some(distance) = self.lod_distance {
            camera.set_native_sail_math(true);
            if let Some(d) = distance {
                camera.distance = d;
            }
        }
    }
    pub fn present_lod(
        &mut self,
        assets: &mut [ModelAsset],
        poses: &[ModelPose],
        visible: [bool; 2],
        seconds: f32,
        view: Camera,
    ) -> Result<()> {
        if !self.lod_enabled() {
            return Ok(());
        }
        for (i, body) in self.bodies.iter_mut().enumerate() {
            if !visible[i] {
                continue;
            }
            for cloth in body {
                let patch = assets[i].meshes[cloth.mesh].sail.as_ref().unwrap();
                let mut matrix = patch.native_frame();
                if let Some(delta) = patch.origin.as_ref().and_then(|o| {
                    assets[i]
                        .ship
                        .transform(&o.node, &assets[i].ship_state, seconds)
                }) {
                    matrix = multiply(delta.to_cols_array(), matrix);
                }
                matrix = multiply(poses[i].matrix()?.to_cols_array(), matrix);
                let entry = &mut cloth.lod;
                let change = entry.selection.update(
                    cloth.input.sphere.center,
                    matrix,
                    view,
                    cloth.motion.rolling,
                    cloth.free.released,
                )?;
                entry.view = Some(view);
                entry.matrix = Some(matrix);
                entry.distance = change.distance;
                if change.changed {
                    entry.changes += 1;
                }
                if change.changed || entry.frame != Some(cloth.frames) {
                    let (mut state, mut vertices) = if let Some(replay) = &entry.replay {
                        (replay.state.clone(), replay.vertices.clone())
                    } else {
                        (cloth.state.clone(), cloth.vertices.clone())
                    };
                    if let Some(replay) = &entry.replay {
                        let mut input = cloth.input;
                        input.rows = entry.selection.grid.rows;
                        input.columns = entry.selection.grid.columns;
                        sail_wave::advance(
                            &mut state,
                            &cloth.motion,
                            &mut vertices,
                            input,
                            &self.table,
                            Settings::default(),
                            replay.delta,
                        )?;
                    }
                    let topology = sail_topology::fill(
                        cloth.motion.kind,
                        cloth.motion.rolling,
                        entry.selection.grid,
                        cloth.mask,
                    )?;
                    entry.draw = Some(Draw {
                        vertices,
                        topology,
                        state,
                    });
                    entry.frame = Some(cloth.frames);
                }
                let draw = entry.draw.as_ref().unwrap();
                let frame = Arc::make_mut(
                    assets[i]
                        .ship_state
                        .native_rig
                        .as_mut()
                        .ok_or_else(|| anyhow::anyhow!("LOD requires native rig"))?,
                );
                frame.cloth.insert(cloth.mesh, draw.vertices.clone());
                frame
                    .cloth_indices
                    .insert(cloth.mesh, draw.topology.clone());
            }
            assets[i].ship_state.validate(&assets[i].ship)?;
        }
        Ok(())
    }
}
pub(super) fn report(fleet: &ClothFleet, out: &mut Json) {
    if !fleet.lod_enabled() {
        return;
    }
    out["presentation_lod"] = json!({"canonical_simulation":"unchanged_near_grid","camera_provider":"actual_gpu_orbit_eye_vertical_fov_pinned_math","replay":"immutable_input_to_latest_attached_wave_step","native_entity_scheduling":false,"bodies":fleet.bodies.iter().map(|b|json!({"sails":b.iter().map(|c|{let e=&c.lod;json!({"mesh":c.mesh,"grid":[e.selection.grid.rows,e.selection.grid.columns],"world_center_bits":e.selection.world_center.map(f32::to_bits),"distance_bits":e.distance.map(f32::to_bits),"camera":e.view.map(|v|json!({"eye_bits":v.eye.map(f32::to_bits),"fov_bits":v.perspective.to_bits()})),"matrix_bits":e.matrix.map(|m|m.map(f32::to_bits)),"grid_changes":e.changes,"source_frame":e.frame,"sampled":e.draw.as_ref().map(|d|json!({"phase":[d.state.vertical,d.state.horizontal],"wind_up":d.state.wind_up,"sum_bits":d.state.sum_wind.to_bits(),"point_bits":d.state.points.map(|p|p.map(f32::to_bits)),"rope_bits":d.state.rope_points.map(|p|p.map(f32::to_bits)),"box_center_bits":d.state.box_center.map(f32::to_bits),"box_size_bits":d.state.box_size.map(f32::to_bits),"vertex_bits":d.vertices.iter().map(|v|v.position.into_iter().chain(v.normal).chain(v.uv.into_iter().flatten()).map(f32::to_bits).collect::<Vec<_>>()).collect::<Vec<_>>(),"topology":{"indices":d.topology.indices,"normal_count":d.topology.normal_count,"hole_start":d.topology.hole_start,"hole_count":d.topology.hole_count}}))})}).collect::<Vec<_>>()})).collect::<Vec<_>>()});
}
