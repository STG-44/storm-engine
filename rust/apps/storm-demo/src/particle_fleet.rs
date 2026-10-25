//! Two diagnostic ships share a project and service, with independent poses and effects.
use super::{
    Options,
    ship_fire::{Controller, EffectsWorld, PreviewEffects, Session},
};
use anyhow::{Result, ensure};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use storm_core::{Entity, EntityId, Message, Value};
use storm_render::{
    Gpu,
    model::{
        DynamicMesh, LoadOptions, ModelAsset, ModelPose, ModelScene, OrbitCamera, ParticleLibrary,
    },
    render_fleet_offscreen,
};

struct NativeMotion {
    bodies: Vec<storm_sim::ship_motion::ShipMotion>,
    inputs: Vec<storm_sim::ship_motion::FlatMovement>,
    elapsed_ms: u32,
}
pub struct Fleet {
    native_motion: Option<NativeMotion>,
    assets: Vec<ModelAsset>,
    sessions: Vec<Session>,
    poses: Vec<ModelPose>,
    visible: [bool; 2],
    snapshots: Vec<Json>,
    seconds: f32,
}
impl Fleet {
    fn new(mut assets: Vec<ModelAsset>, library: Arc<ParticleLibrary>) -> Result<Self> {
        ensure!(assets.len() == 2, "fleet fixture requires two ships");
        let world = Rc::new(RefCell::new(EffectsWorld::default()));
        world.borrow_mut().set_library(Some(library.clone()))?;
        world
            .borrow_mut()
            .set_profiles(Some(library.names().cloned().collect()));
        let mut sessions = Vec::new();
        for (index, asset) in assets.iter_mut().enumerate() {
            asset.ship.particle_library = Some(library.clone());
            let owner = EntityId::from_raw(index as u64 + 1);
            let controller = Rc::new(RefCell::new(Controller::new(
                asset.ship.fires.clone(),
                PreviewEffects {
                    world: world.clone(),
                    entity: owner,
                },
            )));
            asset.ship_state.particle_history = Some(Arc::default());
            sessions.push(Session {
                owner,
                controller: Some(controller),
                world: world.clone(),
            });
        }
        Ok(Self {
            native_motion: None,
            assets,
            sessions,
            poses: vec![ModelPose::default(); 2],
            visible: [true; 2],
            snapshots: Vec::new(),
            seconds: 0.0,
        })
    }
    fn enable_native_motion(&mut self) {
        use storm_sim::ship_motion::{FlatMovement, MotionState, ShipMotion};
        let mut bodies = Vec::new();
        let mut inputs = Vec::new();
        for index in 0..2 {
            let pose = self.pose(index, 0.0);
            let mut body = ShipMotion {
                state: MotionState {
                    position: [
                        pose.translation[0],
                        pose.translation[1],
                        -pose.translation[2],
                    ],
                    angles: [0.0, -pose.yaw, 0.0],
                    inertia_accel: [0.5, 1.0, 2.0],
                    inertia_brake: [0.2, 0.7, 0.45],
                    mass_inertia: 0.1,
                    ..Default::default()
                },
                ..Default::default()
            };
            body.strengths[0].speed[2] = 1.0;
            body.strengths[0].rotation[1] = if index == 0 { 0.25 } else { -0.35 };
            bodies.push(body);
            inputs.push(FlatMovement {
                max_speed_z: 12.0 + 2.0 * index as f32,
                max_speed_y: 0.08,
            });
        }
        self.native_motion = Some(NativeMotion {
            bodies,
            inputs,
            elapsed_ms: 0,
        });
    }
    fn motion_report(&self) -> Option<Json> {
        self.native_motion.as_ref().map(|m|json!({"speed_units":"knots","native_frame_units":"milliseconds","elapsed_ms":m.elapsed_ms,
            "bodies":m.bodies.iter().map(|b|json!({"position_bits":b.state.position.map(f32::to_bits),"angles_bits":b.state.angles.map(f32::to_bits),"speed_bits":b.state.speed.map(f32::to_bits),"rotation_bits":b.state.rotation.map(f32::to_bits),"speed_accel_bits":b.speed_accel.map(f32::to_bits),"strengths":b.strengths.iter().map(|s|json!({"used":s.used,"inertia":s.inertia,"speed_bits":s.speed.map(f32::to_bits),"rotation_bits":s.rotation.map(f32::to_bits)})).collect::<Vec<_>>()})).collect::<Vec<_>>() }))
    }
    fn pose(&self, index: usize, seconds: f32) -> ModelPose {
        if let Some(motion) = &self.native_motion {
            let state = motion.bodies[index].state;
            return ModelPose {
                translation: [state.position[0], state.position[1], -state.position[2]],
                yaw: -state.angles[1],
                pitch: 0.0,
                roll: 0.0,
            };
        }

        let spacing = self.assets[0].radius.max(self.assets[1].radius) * 1.4;
        ModelPose {
            translation: [
                if index == 0 { -spacing } else { spacing },
                0.0,
                seconds * (index as f32 + 1.0),
            ],
            yaw: if index == 0 { 0.15 } else { -0.15 },
            ..Default::default()
        }
    }
    fn activate(&mut self, index: usize, alternate: bool) -> Result<()> {
        let smoke = if alternate {
            "request_only"
        } else {
            "Ship_Smoke"
        };
        let fire = if alternate {
            "SHIP_FIRE.XPS"
        } else {
            "Ship_Fire"
        };
        // The real-resource project may not contain the synthetic third profile.
        let smoke = if self.assets[index]
            .ship
            .particle_library
            .as_ref()
            .unwrap()
            .contains(smoke)
        {
            smoke
        } else {
            "smoke"
        };
        for point in 0..self.assets[index].ship.fires.len() {
            let mut message = Message::new(
                "llsssfl",
                vec![
                    Value::Long(storm_sim::fire::MSG_SHIP_ACTIVATE_FIRE_PLACE),
                    Value::Long(point as i32),
                    Value::String(smoke.into()),
                    Value::String(fire.into()),
                    Value::String("silent".into()),
                    Value::Float(60.0),
                    Value::Long(index as i32),
                ],
            )?;
            self.sessions[index]
                .controller
                .as_ref()
                .unwrap()
                .borrow_mut()
                .process_message(&mut message)?;
        }
        Ok(())
    }
    fn snapshot(&mut self, phase: &str) {
        self.snapshots.push(json!({"phase":phase,"seconds_bits":self.seconds.to_bits(),"visible":self.visible,
            "ships":self.sessions.iter().map(|s|json!({"owner":s.owner.raw(),"emissions":s.world.borrow().emissions_for(s.owner).iter().map(|e|json!({"id":e.instance,"profile":e.profile,"paused":e.stopped.is_some()})).collect::<Vec<_>>()})).collect::<Vec<_>>() }));
        if let Some(motion) = self.motion_report() {
            self.snapshots.last_mut().unwrap()["native_motion"] = motion;
        }
    }
    pub fn advance(mut self, through: f32) -> Result<Self> {
        ensure!(
            through.is_finite() && (0.0..=30.0).contains(&through),
            "fleet fixture time must be 0..=30 seconds"
        );
        let frames = (f64::from(through) * 60.0).floor() as u32;
        for frame in 0..=frames {
            self.seconds = (f64::from(frame) / 60.0) as f32;
            self.sessions[0]
                .world
                .borrow_mut()
                .set_clock(self.seconds)?;
            if let Some(motion) = &mut self.native_motion {
                let elapsed = (f64::from(frame) * 1000.0 / 60.0).round() as u32;
                let delta = elapsed - motion.elapsed_ms;
                for index in 0..2 {
                    if self.visible[index] {
                        motion.bodies[index].advance(delta, false, &mut motion.inputs[index])?;
                    }
                }
                motion.elapsed_ms = elapsed;
            }
            for index in 0..2 {
                self.poses[index] = self.pose(index, self.seconds);
                self.assets[index]
                    .ship_state
                    .particle_history
                    .as_ref()
                    .unwrap()
                    .record(self.poses[index].sample(f64::from(self.seconds)))?;
                if let Some(controller) = &self.sessions[index].controller {
                    super::ship_fire::update(
                        &mut controller.borrow_mut(),
                        if frame == 0 { 0.0 } else { 1.0 / 60.0 },
                        self.poses[index],
                    )?;
                }
            }
            match frame {
                0 => {
                    self.activate(0, false)?;
                    self.activate(1, false)?;
                    self.snapshot("both_started");
                }
                30 => {
                    self.sessions[0]
                        .controller
                        .as_ref()
                        .unwrap()
                        .borrow_mut()
                        .stop_all();
                    self.snapshot("first_paused");
                }
                45 => {
                    self.activate(0, true)?;
                    self.snapshot("first_restarted");
                }
                60 if self.native_motion.is_some() => {
                    let mut message = Message::new(
                        "llffffff",
                        vec![
                            Value::Long(storm_sim::ship_motion::MSG_SHIP_ADD_MOVE_IMPULSE),
                            Value::Long(0),
                            Value::Float(3.0),
                            Value::Float(0.0),
                            Value::Float(1.5),
                            Value::Float(0.0),
                            Value::Float(0.1),
                            Value::Float(0.0),
                        ],
                    )?;
                    self.native_motion.as_mut().unwrap().bodies[0].process_message(&mut message)?;
                    self.snapshot("first_impulse_queued");
                }
                120 => {
                    self.sessions[0].controller.take();
                    self.sessions[0]
                        .world
                        .borrow_mut()
                        .release_owner(self.sessions[0].owner);
                    self.visible[0] = false;
                    self.snapshot("first_released");
                }
                _ => {}
            }
        }
        // Preserve exact requested draw time without inventing fractional births.
        self.seconds = through;
        self.sessions[0].world.borrow_mut().set_clock(through)?;
        for index in 0..2 {
            self.poses[index] = self.pose(index, through);
            self.assets[index]
                .ship_state
                .particle_history
                .as_ref()
                .unwrap()
                .record(self.poses[index].sample(f64::from(through)))?;
            self.sessions[index].apply(&mut self.assets[index].ship_state);
            self.assets[index].sync_particle_profiles()?;
        }
        self.snapshot("final");
        Ok(self)
    }
    pub fn report(&self) -> Result<Json> {
        let mut report = json!({"fixture":"two_ship_particle_service","original_gameplay":false,"seconds_bits":self.seconds.to_bits(),"snapshots":self.snapshots,
            "service":self.sessions[0].world.borrow().report(),
            "ships":self.assets.iter().enumerate().map(|(i,a)|Ok(json!({"owner":self.sessions[i].owner.raw(),"visible":self.visible[i],"pose":self.poses[i].matrix()?.to_cols_array(),"ship":super::rig_report::ship_frame(a,self.seconds)?}))).collect::<Result<Vec<_>>>()?});
        if let Some(motion) = self.motion_report() {
            report["native_motion"] = motion;
        }
        Ok(report)
    }
    pub fn render(
        &self,
        gpu: &Gpu,
        width: u32,
        height: u32,
        wind: super::SailWind,
    ) -> Result<storm_render::Frame> {
        let mut camera = OrbitCamera::new(&self.assets[0]);
        camera.target.x = 0.0;
        camera.target.z = self.seconds;
        if self.native_motion.is_some() {
            let poses = self
                .poses
                .iter()
                .enumerate()
                .filter(|(i, _)| self.visible[*i])
                .map(|(_, p)| p)
                .collect::<Vec<_>>();
            camera.target.x =
                poses.iter().map(|p| p.translation[0]).sum::<f32>() / poses.len() as f32;
            camera.target.z =
                poses.iter().map(|p| p.translation[2]).sum::<f32>() / poses.len() as f32;
        }

        camera.distance *= 2.4;
        camera.radius *= 3.0;
        camera.pitch = 0.4;
        camera.yaw = 0.2;
        let mut scenes = self
            .assets
            .iter()
            .enumerate()
            .filter(|(i, _)| self.visible[*i])
            .map(|(i, a)| {
                let mut scene = ModelScene::new(gpu, wgpu::TextureFormat::Rgba8UnormSrgb, a)?;
                scene.set_pose(self.poses[i])?;
                Ok(scene)
            })
            .collect::<Result<Vec<_>>>()?;
        render_fleet_offscreen(gpu, &mut scenes, &camera, wind, width, height, self.seconds)
    }
}
fn generated() -> Result<Fleet> {
    let bytes =
        include_bytes!("../../../crates/storm-formats/reference/particle_project/billboard.xps");
    let library = Arc::new(ParticleLibrary::from_bytes(
        ["Ship_Fire", "Ship_Smoke", "request_only"]
            .into_iter()
            .map(|name| (name.into(), bytes.to_vec())),
    )?);
    let mut assets = Vec::new();
    for _ in 0..2 {
        let mut asset = ModelAsset::ship_demo()?;
        asset.ship.particle_texture = asset.meshes.iter().find_map(|m| {
            matches!(&m.dynamic, Some(DynamicMesh::Particles(_))).then_some(m.texture)
        });
        asset
            .meshes
            .retain(|m| !matches!(&m.dynamic, Some(DynamicMesh::Particles(_))));
        asset.ship.particle_profiles = Some(Default::default());
        assets.push(asset);
    }
    Fleet::new(assets, library)
}
pub fn run(options: &Options) -> Result<()> {
    ensure!(
        options.headless.is_some() || options.particle_fleet_report.is_some(),
        "select --headless or --particle-fleet-report for the fleet fixture"
    );
    let mut fleet = if let Some(path) = &options.model {
        let mut assets = Vec::new();
        for _ in 0..2 {
            assets.push(ModelAsset::load(
                path,
                &LoadOptions {
                    game_dir: options.game_dir.clone(),
                    texture_dir: options.texture_dir.clone(),
                    particle_project: Some(
                        options
                            .particle_project
                            .clone()
                            .unwrap_or_else(|| "default".into()),
                    ),
                    crew: options.crew.map(|n| n as usize),
                    ..Default::default()
                },
            )?);
        }
        let library = assets[0]
            .ship
            .particle_library
            .clone()
            .ok_or_else(|| anyhow::anyhow!("fleet requires an original particle project"))?;
        Fleet::new(assets, library)?
    } else {
        generated()?
    };
    if options.native_ship_motion {
        fleet.enable_native_motion();
    }
    let fleet = fleet.advance(options.time)?;
    if let Some(path) = &options.particle_fleet_report {
        std::fs::write(path, serde_json::to_vec_pretty(&fleet.report()?)?)?;
    }
    if let Some(path) = &options.headless {
        let instance = storm_render::create_instance(options.backend.flags())?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        fleet
            .render(&gpu, options.width, options.height, options.wind())?
            .save_png(path)?;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn two_owners_restart_and_release_independently_from_a_shared_project() {
        let fleet = generated().unwrap().advance(3.0).unwrap();
        let report = fleet.report().unwrap();
        assert_eq!(
            report["snapshots"][2]["ships"][0]["emissions"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
        assert_eq!(
            report["snapshots"][2]["ships"][1]["emissions"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(report["service"]["manager"]["owners"], json!([2]));
        assert_eq!(report["service"]["manager"]["prepared"], 3);
        assert_eq!(report["service"]["manager"]["systems"], 2);
        assert!(
            fleet.sessions[1]
                .world
                .borrow()
                .emissions_for(fleet.sessions[1].owner)
                .iter()
                .all(|e| e.stopped.is_none())
        );
    }
    #[test]
    fn native_motion_impulses_change_only_the_first_body_and_deletion_freezes_it() {
        let mut fleet = generated().unwrap();
        fleet.enable_native_motion();
        let mut expected = fleet.native_motion.as_ref().unwrap().bodies.clone();
        let mut inputs = fleet.native_motion.as_ref().unwrap().inputs.clone();
        let mut previous = 0;
        for frame in 0..=90 {
            let ms = (f64::from(frame) * 1000.0 / 60.0).round() as u32;
            for i in 0..2 {
                expected[i]
                    .advance(ms - previous, false, &mut inputs[i])
                    .unwrap();
            }
            previous = ms;
        }
        let fleet = fleet.advance(1.5).unwrap();
        let motion = fleet.native_motion.as_ref().unwrap();
        assert_ne!(motion.bodies[0].state, expected[0].state);
        assert_eq!(motion.bodies[1].state, expected[1].state);
        assert!(
            !motion.bodies[0].strengths[1].used,
            "one-shot impulse is consumed by the next ordinary frame"
        );
        assert!(motion.bodies.iter().all(|b| b.state.speed[2] > 0.0));
        let first = fleet.poses[0].translation;
        assert_eq!(first[2], -motion.bodies[0].state.position[2]);
        let mut short = generated().unwrap();
        short.enable_native_motion();
        let short = short.advance(2.0).unwrap();
        let mut long = generated().unwrap();
        long.enable_native_motion();
        let long = long.advance(3.0).unwrap();
        assert_eq!(
            short.native_motion.as_ref().unwrap().bodies[0].state,
            long.native_motion.as_ref().unwrap().bodies[0].state
        );
        assert_ne!(
            short.native_motion.as_ref().unwrap().bodies[1].state,
            long.native_motion.as_ref().unwrap().bodies[1].state
        );
    }
    #[test]
    fn fractional_presentation_keeps_the_native_millisecond_body_at_the_last_complete_frame() {
        let mut whole = generated().unwrap();
        whole.enable_native_motion();
        let whole = whole.advance(1.0).unwrap();
        let mut fractional = generated().unwrap();
        fractional.enable_native_motion();
        let fractional = fractional.advance(1.005).unwrap();
        assert_eq!(whole.native_motion.as_ref().unwrap().elapsed_ms, 1000);
        assert_eq!(fractional.native_motion.as_ref().unwrap().elapsed_ms, 1000);
        for index in 0..2 {
            assert_eq!(
                whole.native_motion.as_ref().unwrap().bodies[index].state,
                fractional.native_motion.as_ref().unwrap().bodies[index].state
            );
        }
        assert!(
            generated()
                .unwrap()
                .report()
                .unwrap()
                .get("native_motion")
                .is_none()
        );
        let mut empty = generated().unwrap();
        for asset in &mut empty.assets {
            asset.ship.fires.clear();
        }
        for session in &mut empty.sessions {
            session
                .controller
                .as_ref()
                .unwrap()
                .borrow_mut()
                .queries
                .fire_places_mut()
                .clear();
        }
        empty.enable_native_motion();
        let empty = empty.advance(3.0).unwrap();
        assert_eq!(empty.report().unwrap()["service"]["manager"]["systems"], 0);
        assert!(empty.native_motion.as_ref().unwrap().bodies[1].state.speed[2] > 0.0);
    }
}
