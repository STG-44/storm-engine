//! Falling-mast selection and deterministic inputs are diagnostic service ports.
use super::*;
use storm_render::model::ShipState;
use storm_sim::sail_free::{self, Draws, NativeNonFinite, Parameters};
#[derive(Clone, Copy, Debug)]
pub(super) struct Schedule {
    pub mast: Option<u32>,
    pub at: f32,
    pub triggered: bool,
}
#[derive(Default)]
pub(super) struct Entry {
    pub released: bool,
    pub updates: u32,
    pub held: u32,
    pub draws: u64,
    pub axes: [[f32; 3]; 3],
}
fn transformed_axes(axes: [[f32; 3]; 3], m: [f32; 16]) -> [[f32; 3]; 3] {
    axes.map(|v| {
        [
            m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
            m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
            m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
        ]
    })
}
fn attached_count(asset: &ModelAsset, mast: u32) -> usize {
    let mut probe = ShipState::default();
    probe.broken_masts.insert(mast, 0.0);
    asset
        .meshes
        .iter()
        .filter_map(|m| m.sail.as_ref())
        .filter(|s| {
            s.origin
                .as_ref()
                .is_some_and(|o| asset.ship.fallen_mast(&o.node, &probe).is_some())
        })
        .count()
}
pub(super) fn pin(asset: &mut ModelAsset) -> Result<()> {
    Arc::make_mut(
        asset
            .ship_state
            .native_rig
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("free cloth requires native rig"))?,
    )
    .pinned_cloth_fall = true;
    Ok(())
}
impl ClothFleet {
    pub fn enable_free(&mut self, assets: &[ModelAsset], mast: Option<u32>, at: f32) -> Result<()> {
        ensure!(
            self.rolls && at.is_finite() && (0.0..=30.0).contains(&at),
            "free cloth requires native rolls and a bounded mast time"
        );
        let asset = &assets[0];
        let mast = if let Some(mast) = mast {
            ensure!(
                asset.ship.masts().contains(&mast) && attached_count(asset, mast) > 0,
                "free mast has no attached sail"
            );
            Some(mast)
        } else {
            asset
                .ship
                .masts()
                .into_iter()
                .filter_map(|mast| {
                    let n = attached_count(asset, mast);
                    (n > 0).then_some((mast, n))
                })
                .max_by_key(|(mast, n)| (*n, std::cmp::Reverse(*mast)))
                .map(|(mast, _)| mast)
        };
        self.free_mode = Some(Schedule {
            mast,
            at,
            triggered: false,
        });
        Ok(())
    }
    pub fn free_enabled(&self) -> bool {
        self.free_mode.is_some()
    }
    pub(super) fn prepare_free(
        &mut self,
        assets: &mut [ModelAsset],
        drive: &mut drive::Drive,
        visible: [bool; 2],
        seconds: f32,
    ) -> Result<()> {
        let Some(mut mode) = self.free_mode else {
            return Ok(());
        };
        if !mode.triggered
            && visible[0]
            && seconds >= mode.at
            && let Some(mast) = mode.mast
        {
            assets[0].ship_state.broken_masts.insert(mast, mode.at);
            mode.triggered = true;
            self.free_mode = Some(mode);
        }
        for (i, body) in self.bodies.iter_mut().enumerate() {
            if !visible[i] {
                continue;
            }
            pin(&mut assets[i])?;
            for (j, cloth) in body.iter_mut().enumerate() {
                let fallen = assets[i].meshes[cloth.mesh]
                    .sail
                    .as_ref()
                    .unwrap()
                    .origin
                    .as_ref()
                    .and_then(|o| assets[i].ship.fallen_mast(&o.node, &assets[i].ship_state))
                    .is_some_and(|(_, start, _)| seconds >= start);
                if fallen && !cloth.free.released {
                    drive.free_cloth(i, j)?;
                    cloth.free.released = true;
                }
            }
        }
        Ok(())
    }
}
pub(super) fn advance(
    cloth: &mut Cloth,
    asset: &ModelAsset,
    pose: ModelPose,
    seconds: f32,
) -> Result<()> {
    let patch = asset.meshes[cloth.mesh].sail.as_ref().unwrap();
    let mut axes = patch.native_drive(0).axes;
    if let Some(delta) = patch
        .origin
        .as_ref()
        .and_then(|o| asset.ship.transform(&o.node, &asset.ship_state, seconds))
    {
        axes = transformed_axes(axes, delta.to_cols_array());
    }
    axes = transformed_axes(axes, drive::effect_matrix(pose)?);
    cloth.free.axes = axes;
    let count = sail_free::draw_count(cloth.motion.kind);
    let samples = [16383; 16];
    let result = sail_free::advance(
        &mut cloth.state,
        &cloth.motion,
        &mut cloth.vertices,
        Parameters {
            height: cloth.input.height,
            sail_wind: cloth.input.local_wind,
            wind_base: cloth.input.wind_base,
            axes,
            ..Default::default()
        },
        Draws {
            values: &samples[..count],
            maximum: 32767,
        },
    );
    cloth.free.draws += count as u64;
    match result {
        Ok(used) => {
            ensure!(used == count, "free cloth draw count mismatch");
            cloth.free.updates += 1;
        }
        Err(e) if e.is::<NativeNonFinite>() => cloth.free.held += 1,
        Err(e) => return Err(e),
    }
    Ok(())
}
pub(super) fn report(fleet: &ClothFleet, report: &mut Json) {
    let Some(mode) = fleet.free_mode else {
        return;
    };
    report["mode"] = json!("attached_rolled_and_free");
    report["native_free_vertices"] = json!(true);
    report["free_provider"] = json!({"random":"fixed_16383_of_32767","axes":"preceding_fleet_pose_pinned_fall_yard_static_node","native_rng":false,"native_mast_physics":false,"group_owner_migration":false,"nonfinite_policy":"hold_last_finite_frame","mast":mode.mast,"at_bits":mode.at.to_bits(),"triggered":mode.triggered});
    for (i, body) in fleet.bodies.iter().enumerate() {
        for (j, c) in body.iter().enumerate() {
            let out = &mut report["bodies"][i]["sails"][j];
            out["free"] = json!(c.free.released);
            out["free_updates"] = json!(c.free.updates);
            out["held_updates"] = json!(c.free.held);
            out["random_draws"] = json!(c.free.draws);
            out["free_axes_bits"] = json!(c.free.axes.map(|v| v.map(f32::to_bits)));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_nonfinite_free_update_holds_vertices_and_records_all_consumed_draws() {
        let asset = ModelAsset::ship_demo().unwrap();
        let mesh = asset
            .meshes
            .iter()
            .position(|m| m.sail.as_ref().is_some_and(|s| !s.is_triangle()))
            .unwrap();
        let patch = asset.meshes[mesh].sail.as_ref().unwrap();
        let (motion, sphere) = patch.native_geometry(0).unwrap();
        let input = patch.native_wave_input(0).unwrap();
        let vertices = sail_geometry::fill(&motion, sphere, [1.0; 2]).unwrap();
        let mut state = State::default();
        let direction = sail_free::local_direction(Parameters {
            height: input.height,
            axes: patch.native_drive(0).axes,
            ..Default::default()
        })
        .unwrap();
        let dv = direction.map(|v| v * (input.height * 0.25));
        let h = motion.hard_points;
        for x in 0..4 {
            let mut point =
                std::array::from_fn(|i| h[0][i] + (h[1][i] - h[0][i]) * (x as f32 / 3.0));
            for y in 0..5 {
                state.points[x * 5 + y] = point;
                point = std::array::from_fn(|i| point[i] + dv[i]);
            }
        }
        let before = (state.clone(), vertices.clone());
        let mut cloth = Cloth {
            mesh,
            motion,
            input,
            state,
            vertices,
            frames: 0,
            mask: 0,
            topology: None,
            free: Entry {
                released: true,
                ..Default::default()
            },
        };
        advance(&mut cloth, &asset, ModelPose::default(), 0.0).unwrap();
        assert_eq!((cloth.state, cloth.vertices), before);
        assert_eq!(
            (cloth.free.held, cloth.free.updates, cloth.free.draws),
            (1, 0, 16)
        );
    }
}
