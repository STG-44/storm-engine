//! Explicit fleet adapter: static node frames and settled sail sets, native drive.
use super::*;
use storm_sim::{
    RigState, SailMode,
    sail_drive::{self, Coefficients, Group, RollRequest, SailOutput},
    ship_sails::ShipSailState,
};

/// The adapter's T * Ry * Rx * Rz frame with the same pinned trig as motion.
/// Keep this scoped to the new drive; legacy diagnostic reports retain their math.
pub(super) fn effect_matrix(pose: ModelPose) -> Result<[f32; 16]> {
    ensure!(
        pose.translation
            .into_iter()
            .chain([pose.yaw, pose.pitch, pose.roll])
            .all(f32::is_finite),
        "invalid effect pose"
    );
    let (sy, cy) = (libm::sinf(pose.yaw), libm::cosf(pose.yaw));
    let (sx, cx) = (libm::sinf(pose.pitch), libm::cosf(pose.pitch));
    let (sz, cz) = (libm::sinf(pose.roll), libm::cosf(pose.roll));
    Ok([
        cy * cz + sy * sx * sz,
        cx * sz,
        -sy * cz + cy * sx * sz,
        0.0,
        -cy * sz + sy * sx * cz,
        cx * cz,
        sy * sz + cy * sx * cz,
        0.0,
        sy * cx,
        -sx,
        cy * cx,
        0.0,
        pose.translation[0],
        pose.translation[1],
        pose.translation[2],
        1.0,
    ])
}
pub(super) fn update_effects(
    controller: &mut Controller,
    delta: f32,
    pose: ModelPose,
) -> Result<()> {
    let values = effect_matrix(pose)?;
    let mut matrix = pose.matrix()?;
    let column = |i: usize| {
        [
            values[i * 4],
            values[i * 4 + 1],
            values[i * 4 + 2],
            values[i * 4 + 3],
        ]
    };
    matrix.x_axis = column(0).into();
    matrix.y_axis = column(1).into();
    matrix.z_axis = column(2).into();
    matrix.w_axis = column(3).into();
    controller.set_transform(matrix)?;
    controller.execute(delta);
    Ok(())
}

pub(super) struct Drive {
    pub groups: Vec<Group>,
    requests: Vec<ShipSailState>,
    meshes: Vec<Vec<usize>>,
    outputs: Vec<Vec<SailOutput>>,
    pub direction: f32,
}
impl Drive {
    pub fn new(assets: &[ModelAsset], request: f32, direction: f32) -> Result<Self> {
        ensure!(direction.is_finite(), "invalid fleet wind direction");
        SailMode::from_setting(f64::from(request))?;
        let mut groups = Vec::new();
        let mut meshes = Vec::new();
        let mut requests = Vec::new();
        for (i, a) in assets.iter().enumerate() {
            let entries = a
                .meshes
                .iter()
                .enumerate()
                .filter_map(|(m, v)| {
                    v.sail.as_ref().map(|s| {
                        (
                            m,
                            s.native_drive(a.ship_state.sail_holes.get(&m).copied().unwrap_or(0)),
                        )
                    })
                })
                .collect::<Vec<_>>();
            let mut reference = 0.0;
            for (_, s) in &entries {
                reference += s.maximum;
            }
            groups.push(Group {
                ship: i as u64 + 1,
                deleted: false,
                current_set: 2,
                speed: 0.0,
                maximum: 0.0,
                reference,
                speed_multiplier: 1.0,
                sails: entries.iter().map(|(_, s)| s.clone()).collect(),
            });
            meshes.push(entries.iter().map(|(m, _)| *m).collect());
            let mut state = ShipSailState::default();
            state.set(request);
            requests.push(state);
        }
        let mut drive = Self {
            outputs: vec![Vec::new(); groups.len()],
            groups,
            requests,
            meshes,
            direction,
        };
        // Initialization uses the same selection case; the geometry port settles
        // immediately. Runtime queries still return the preceding sail-set speed.
        for i in 0..drive.groups.len() {
            let mut ignored = 0.0;
            let calls = sail_drive::request_speed(
                &mut drive.groups,
                i as u64 + 1,
                Some(&mut ignored),
                request,
                0.0,
            )?;
            drive.settle(calls);
        }
        Ok(drive)
    }
    fn settle(&mut self, calls: Vec<RollRequest>) {
        for c in calls {
            let s = &mut self.groups[c.group].sails[c.sail];
            s.rolling = c.roll_up;
            s.rolling_delta = None;
        }
    }
    pub fn change(&mut self, index: usize, delta: f32) {
        let request = (self.requests[index].get() + delta).clamp(0.0, 1.0);
        self.requests[index].set(request);
    }
    pub fn rig(&self, index: usize) -> RigState {
        RigState::for_mode(match self.groups[index].current_set {
            0 => SailMode::Furled,
            1 => SailMode::Normal,
            _ => SailMode::Full,
        })
    }
    pub fn wind(&self) -> super::super::SailWind {
        super::super::SailWind {
            direction: self.direction,
            ..Default::default()
        }
    }
    pub fn step(
        &mut self,
        assets: &[ModelAsset],
        poses: &[ModelPose],
        visible: [bool; 2],
        seconds: f32,
        bodies: &mut [storm_sim::ship_motion::ShipMotion],
    ) -> Result<()> {
        let wind = [libm::sinf(self.direction), 0.0, libm::cosf(self.direction)];
        for i in 0..self.groups.len() {
            if !visible[i] {
                continue;
            }
            let pose = poses[i];
            // Invert the viewer's Ry * Rx * Rz orientation with pinned scalar
            // trig. Node axes then enter the retained MulToInvNorm arithmetic.
            let (s, c) = (libm::sinf(pose.yaw), libm::cosf(pose.yaw));
            let v = [
                c * wind[0] - s * wind[2],
                wind[1],
                s * wind[0] + c * wind[2],
            ];
            let (s, c) = (libm::sinf(pose.pitch), libm::cosf(pose.pitch));
            let v = [v[0], c * v[1] + s * v[2], -s * v[1] + c * v[2]];
            let (s, c) = (libm::sinf(pose.roll), libm::cosf(pose.roll));
            let v = [c * v[0] + s * v[1], -s * v[0] + c * v[1], v[2]];
            for (j, &mesh) in self.meshes[i].iter().enumerate() {
                let patch = assets[i].meshes[mesh].sail.as_ref().unwrap();
                let input = patch.native_drive(
                    assets[i]
                        .ship_state
                        .sail_holes
                        .get(&mesh)
                        .copied()
                        .unwrap_or(0),
                );
                let lost = patch.origin.as_ref().is_some_and(|o| {
                    assets[i]
                        .ship
                        .fallen_mast(&o.node, &assets[i].ship_state)
                        .is_some_and(|(_, t, _)| seconds >= t)
                });
                let sail = &mut self.groups[i].sails[j];
                sail.holes = if lost { sail.hole_maximum } else { input.holes };
                sail.hole_dependence = if lost { 1.0 } else { input.hole_dependence };
            }
            self.outputs[i] = self.groups[i].evaluate(v, Coefficients::default())?;
            let mut output = bodies[i].strengths[0].speed[2];
            let calls = sail_drive::request_speed(
                &mut self.groups,
                i as u64 + 1,
                Some(&mut output),
                self.requests[i].get(),
                0.0,
            )?;
            self.settle(calls);
            bodies[i].strengths[0].speed[2] = if bodies[i].dead { 0.0 } else { output };
        }
        Ok(())
    }
    pub fn report(&self) -> Json {
        json!({"original_weather":false,"rolling_geometry":"settled_adapter","wind_direction_bits":self.direction.to_bits(),"bodies":self.groups.iter().enumerate().map(|(i,g)|json!({
            "request_bits":self.requests[i].get().to_bits(),"set":g.current_set,"deleted":g.deleted,"speed_bits":g.speed.to_bits(),"maximum_bits":g.maximum.to_bits(),"reference_bits":g.reference.to_bits(),"multiplier_bits":g.speed_multiplier.to_bits(),
            "sails":g.sails.iter().zip(&self.outputs[i]).map(|(s,o)|json!({"kind":s.kind as i32,"roll_type":s.roll_type,"rolled":s.rolling,"holes":s.holes,"capacity_bits":s.maximum.to_bits(),"wind_bits":o.wind.map(f32::to_bits),"current_bits":o.current.to_bits(),"maximum_bits":o.maximum.to_bits()})).collect::<Vec<_>>() })).collect::<Vec<_>>()})
    }
}
