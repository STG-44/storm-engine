//! Two diagnostic ships share a project and service, with independent poses and effects.
use super::{
    Options,
    ship_fire::{Controller, EffectsWorld, PreviewEffects, Session},
};
use anyhow::{Result, ensure};
use serde_json::{Value as Json, json};
use std::{cell::RefCell, rc::Rc, sync::Arc};
use storm_core::{AttributeId, AttributeStore, Entity, EntityId, Message, Value};
use storm_sim::ship_placement::ShipPlacement;
use storm_sim::{ship_placement::RootPose, ship_presentation};
mod drive;
mod parameters;
mod window;
use storm_render::{
    Gpu,
    model::{
        DynamicMesh, LoadOptions, ModelAsset, ModelPose, ModelScene, OrbitCamera, ParticleLibrary,
    },
    render_fleet_offscreen,
};
use storm_sim::ship_parameters::{MountedParameters, ShipParameters};
use storm_sim::{
    ship_motion::{FlatMovement, MovementPort},
    ship_rocking::{DiagnosticSea, RockingInput, SeaSurface, ShipRocking},
};

struct NativeMotion {
    bodies: Vec<storm_sim::ship_motion::ShipMotion>,
    inputs: Vec<storm_sim::ship_motion::FlatMovement>,
    elapsed_ms: u32,
    rocking: Option<NativeRocking>,
    placement: Option<NativePlacement>,
    presentation: Option<Vec<RootPose>>,
    parameters: Option<NativeParameters>,
}
struct NativeParameters {
    loaded: Vec<ShipParameters>,
    mounted: Vec<MountedParameters>,
    external: bool,
}
fn step_body(
    body: &mut storm_sim::ship_motion::ShipMotion,
    presented: Option<&mut RootPose>,
    delta: u32,
    port: &mut impl MovementPort,
) -> Result<()> {
    if let Some(pose) = presented {
        *pose = ship_presentation::advance_living(body, delta, port)?;
    } else {
        body.advance(delta, false, port)?;
    }
    Ok(())
}
struct NativePlacement {
    attributes: AttributeStore,
    characters: Vec<AttributeId>,
    ship_types: Vec<AttributeId>,
    states: Vec<ShipPlacement>,
}
struct NativeRocking {
    bodies: Vec<ShipRocking>,
    waves: storm_sim::Waves,
    sea: DiagnosticSea,
}
struct RockingMovement<'a> {
    limits: &'a FlatMovement,
    rocking: &'a mut ShipRocking,
    sea: &'a mut DiagnosticSea,
}
struct ReflectedSea<'a>(&'a mut DiagnosticSea);
impl SeaSurface for ReflectedSea<'_> {
    fn wave_xz(&mut self, x: f32, z: f32) -> Result<f32> {
        self.0.wave_xz(x, -z)
    }
}
impl MovementPort for RockingMovement<'_> {
    fn max_speed_z(&mut self) -> f32 {
        self.limits.max_speed_z
    }
    fn max_speed_y(&mut self) -> f32 {
        self.limits.max_speed_y
    }
    fn rocking(
        &mut self,
        position: [f32; 3],
        angles: [f32; 3],
        delta: f32,
    ) -> Result<(f32, [f32; 3])> {
        // Living SHIP::Execute captures vAng = State.vAng immediately before Move.
        let mut sea = ReflectedSea(self.sea);
        let result = self.rocking.advance(
            RockingInput {
                position,
                angles,
                previous_angles: angles,
            },
            delta,
            Some(&mut sea),
        )?;
        Ok((result.height, result.angles))
    }
}
pub struct Fleet {
    next_frame: u32,
    preview: bool,
    native_motion: Option<NativeMotion>,
    drive: Option<drive::Drive>,
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
            next_frame: 0,
            preview: false,
            native_motion: None,
            drive: None,
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
            rocking: None,
            placement: None,
            presentation: None,
            parameters: None,
        });
    }
    fn enable_native_drive(&mut self, request: f32, direction: f32) -> Result<()> {
        ensure!(
            self.native_motion
                .as_ref()
                .is_some_and(|m| m.parameters.is_some()),
            "native sail drive requires ship parameters"
        );
        self.drive = Some(drive::Drive::new(&self.assets, request, direction)?);
        for i in 0..2 {
            self.poses[i] = self.placed_pose(i, self.seconds)?;
        }
        Ok(())
    }
    fn enable_native_placement(&mut self, immersion: f32, waterline: f32) -> Result<()> {
        ensure!(
            immersion.is_finite() && waterline.is_finite(),
            "invalid placement parameters"
        );
        let motion = self
            .native_motion
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("placement requires native movement"))?;
        let mut binding = NativePlacement {
            attributes: AttributeStore::default(),
            characters: Vec::new(),
            ship_types: Vec::new(),
            states: Vec::new(),
        };
        for body in &motion.bodies {
            let character = binding.attributes.create_root()?;
            let ship = binding.attributes.create_root()?;
            let value = binding
                .attributes
                .create_path(character, "Ship.Immersion")?;
            binding.attributes.set_float(value, None, immersion)?;
            let value = binding.attributes.create_path(ship, "WaterLine")?;
            binding.attributes.set_float(value, None, waterline)?;
            let mut state = ShipPlacement::default();
            state.load_waterline(&binding.attributes, ship)?;
            state.calculate_immersion(&binding.attributes, character)?;
            state.root(
                body.state.position,
                body.state.angles,
                body.offsets,
                [0.0; 2],
            )?;
            binding.characters.push(character);
            binding.ship_types.push(ship);
            binding.states.push(state);
        }
        motion.placement = Some(binding);
        Ok(())
    }
    fn enable_native_parameters(&mut self, config: Option<&Json>) -> Result<()> {
        let sizes = self
            .assets
            .iter()
            .map(|a| a.root_box_size.unwrap_or([6.0, 4.0, 16.0]))
            .collect::<Vec<_>>();
        let motion = self
            .native_motion
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("parameters require native movement"))?;
        let binding = motion
            .placement
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("parameters require placement bindings"))?;
        let mut result = NativeParameters {
            loaded: Vec::new(),
            mounted: Vec::new(),
            external: config.is_some(),
        };
        for (i, size) in sizes.into_iter().enumerate() {
            let ship = binding.ship_types[i];
            let character = binding.characters[i];
            for (name, value) in [
                ("Class", 1 + i as u32),
                ("MinCrew", 5),
                ("MaxCrew", 50),
                ("Capacity", 1000 + 1000 * i as u32),
            ] {
                binding.attributes.set_dword(ship, Some(name), value)?;
            }
            for (name, value) in [
                ("SpeedRate", 12.0 + 2.0 * i as f32),
                ("TurnRate", 35.52),
                ("Weight", 125.0 + 25.0 * i as f32),
                ("SpeedDependWeight", 0.1),
                ("SubSeaDependWeight", 0.2),
                ("TurnDependWeight", 0.3),
                ("TurnDependSpeed", 0.4),
                ("InertiaAccelerationX", 0.5),
                ("InertiaAccelerationY", 1.0),
                ("InertiaAccelerationZ", 2.0),
                ("InertiaBrakingX", 0.2),
                ("InertiaBrakingY", 0.7),
                ("InertiaBrakingZ", 0.45),
            ] {
                binding.attributes.set_float(ship, Some(name), value)?;
            }
            for (path, value) in [
                ("Ship.MaxSpeedZ", motion.inputs[i].max_speed_z),
                ("Ship.MaxSpeedY", motion.inputs[i].max_speed_y),
            ] {
                let id = binding.attributes.create_path(character, path)?;
                binding.attributes.set_float(id, None, value)?;
            }
            if let Some(config) = config {
                parameters::apply(&mut binding.attributes, ship, character, config)?;
            }
            let p = ShipParameters::load(&binding.attributes, ship)?;
            let limits = FlatMovement::from_character(&binding.attributes, character)?;
            ensure!(
                [limits.max_speed_z, limits.max_speed_y]
                    .into_iter()
                    .all(f32::is_finite),
                "non-finite character speed limits"
            );
            let mounted = p.configure(&mut motion.bodies[i].state, &mut binding.states[i], size)?;
            binding.states[i].calculate_immersion(&binding.attributes, character)?;
            motion.inputs[i] = limits;
            result.loaded.push(p);
            result.mounted.push(mounted);
        }
        motion.parameters = Some(result);
        Ok(())
    }
    fn placed_pose(&self, index: usize, seconds: f32) -> Result<ModelPose> {
        if let Some(motion) = &self.native_motion
            && let Some(binding) = &motion.placement
        {
            let body = &motion.bodies[index];
            let presented = motion.presentation.as_ref().map_or(
                RootPose {
                    position: body.state.position,
                    angles: body.state.angles,
                },
                |p| p[index],
            );
            let root = binding.states[index].root(
                presented.position,
                presented.angles,
                body.offsets,
                [0.0; 2],
            )?;
            return Ok(ModelPose {
                translation: [root.position[0], root.position[1], -root.position[2]],
                yaw: -root.angles[1],
                pitch: -root.angles[0],
                roll: root.angles[2],
            });
        }
        Ok(self.pose(index, seconds))
    }
    fn enable_native_presentation(&mut self) -> Result<()> {
        let motion = self
            .native_motion
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("presentation requires native motion"))?;
        ensure!(
            motion.placement.is_some() && motion.rocking.is_some(),
            "presentation requires placement and rocking"
        );
        ensure!(
            motion.bodies.iter().all(|b| !b.dead),
            "presentation only supports living ships"
        );
        motion.presentation = Some(
            motion
                .bodies
                .iter()
                .map(|b| RootPose {
                    position: b.state.position,
                    angles: b.state.angles,
                })
                .collect(),
        );
        Ok(())
    }
    fn enable_native_rocking(&mut self) -> Result<()> {
        let mut bodies = Vec::new();
        for asset in &self.assets {
            // A generated hull uses explicit fixture dimensions; real ships use
            // precisely the root GM bounds that native Mount assigns to vBoxSize.
            let size = asset.root_box_size.unwrap_or([6.0, 4.0, 16.0]);
            ensure!(
                [size[0], size[2]]
                    .into_iter()
                    .all(|v| v.is_finite() && v > 0.0),
                "invalid root hull dimensions"
            );
            bodies.push(ShipRocking::new([size[0], size[2]]));
        }
        let waves = storm_sim::Waves {
            height_m: 1.5,
            direction: 1.2,
        };
        let sea = DiagnosticSea::new(waves)?;
        self.native_motion
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("rocking requires native ship movement"))?
            .rocking = Some(NativeRocking { bodies, waves, sea });
        Ok(())
    }
    fn motion_report(&self) -> Option<Json> {
        self.native_motion.as_ref().map(|m| {
            let mut report = json!({"speed_units":"knots","native_frame_units":"milliseconds","elapsed_ms":m.elapsed_ms,
                "bodies":m.bodies.iter().map(|b|json!({"position_bits":b.state.position.map(f32::to_bits),"angles_bits":b.state.angles.map(f32::to_bits),"speed_bits":b.state.speed.map(f32::to_bits),"rotation_bits":b.state.rotation.map(f32::to_bits),"speed_accel_bits":b.speed_accel.map(f32::to_bits),"strengths":b.strengths.iter().map(|s|json!({"used":s.used,"inertia":s.inertia,"speed_bits":s.speed.map(f32::to_bits),"rotation_bits":s.rotation.map(f32::to_bits)})).collect::<Vec<_>>()})).collect::<Vec<_>>() });
            if let Some(rocking) = &m.rocking {
                report["rocking"] = json!({"surface":"diagnostic_two_sine_waves","original_sea":false,"native_to_surface_z":-1,
                    "coefficient_bits":rocking.sea.coefficients.map(|r|r.map(f32::to_bits)),
                    "bodies":rocking.bodies.iter().enumerate().map(|(i,r)|json!({
                        "dimensions_source":if self.assets[i].root_box_size.is_some() {"root_gm"} else {"generated_fixture"},
                        "size_bits":r.parameters.size.map(f32::to_bits),"offset_bits":r.parameters.offsets.map(f32::to_bits),
                        "height_factor_bits":r.parameters.height_factor.to_bits(),"angle_factor_bits":r.parameters.angle_factor.to_bits(),
                        "height_bits":r.heights.map(|row|row.map(f32::to_bits))})).collect::<Vec<_>>()});
            }
            if let Some(binding)=&m.placement {
                report["placement"]=json!({"original_ship_parameters":false,"bodies":binding.states.iter().map(|s|json!({"immersion_bits":s.immersion.to_bits(),"waterline_bits":s.waterline.to_bits()})).collect::<Vec<_>>()});
            }
            if let Some(poses)=&m.presentation {
                let binding=m.placement.as_ref().unwrap();
                report["presentation"]=json!({"living_only":true,"bodies":poses.iter().enumerate().map(|(i,p)| {
                    let fields=["ship.speed","ship.pos","ship.ang"].map(|path| {
                        let node=binding.attributes.lookup_path(binding.characters[i],path).unwrap();
                        ["x","y","z"].map(|key|node.and_then(|n|binding.attributes.value(n,Some(key))))
                    });
                    json!({"position_bits":p.position.map(f32::to_bits),"angles_bits":p.angles.map(f32::to_bits),"script":{"speed":fields[0],"pos":fields[1],"ang":fields[2]}})
                }).collect::<Vec<_>>()});
            }
            if let Some(parameters)=&m.parameters {
                report["parameters"]=json!({"source":if parameters.external {"external_attributes"} else {"diagnostic_attributes"},"original_game_scripts":false,
                    "bodies":parameters.loaded.iter().zip(&parameters.mounted).enumerate().map(|(i,(p,v))|json!({
                    "integers":[p.class,p.crew_min,p.crew_max,p.capacity],"type_bits":([p.speed_rate,p.turn_rate,p.weight,p.speed_depend_weight,p.sub_sea_depend_weight,p.turn_depend_weight,p.turn_depend_speed,p.waterline].map(f32::to_bits)),
                    "accel_bits":p.inertia_accel.map(f32::to_bits),"brake_bits":p.inertia_brake.map(f32::to_bits),
                    "mounted_bits":([v.length,v.width,v.load,v.nominal_speed_z,v.nominal_speed_y,v.inertia_factor,v.mass_inertia,v.weight,v.gravity].map(f32::to_bits)),
                    "limit_bits":[m.inputs[i].max_speed_z.to_bits(),m.inputs[i].max_speed_y.to_bits()]})).collect::<Vec<_>>()});
            }
            report
        })
    }
    fn pose(&self, index: usize, seconds: f32) -> ModelPose {
        if let Some(motion) = &self.native_motion {
            let state = motion.bodies[index].state;
            return ModelPose {
                translation: [state.position[0], state.position[1], -state.position[2]],
                yaw: -state.angles[1],
                pitch: if motion.rocking.is_some() {
                    -state.angles[0]
                } else {
                    0.0
                },
                roll: if motion.rocking.is_some() {
                    state.angles[2]
                } else {
                    0.0
                },
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
    fn advance_frames(&mut self, through: f32) -> Result<()> {
        ensure!(
            through.is_finite() && (self.seconds..=30.0).contains(&through),
            "fleet fixture time must be 0..=30 seconds"
        );
        let frames = (f64::from(through) * 60.0).floor() as u32;
        for frame in self.next_frame..=frames {
            self.seconds = (f64::from(frame) / 60.0) as f32;
            self.sessions[0]
                .world
                .borrow_mut()
                .set_clock(self.seconds)?;
            if let Some(drive) = &mut self.drive {
                drive.step(
                    &self.assets,
                    &self.poses,
                    self.visible,
                    self.seconds,
                    &mut self.native_motion.as_mut().unwrap().bodies,
                )?;
            }
            if let Some(motion) = &mut self.native_motion {
                if let Some(binding) = &mut motion.placement {
                    for index in 0..2 {
                        if self.visible[index] {
                            binding.states[index].calculate_immersion(
                                &binding.attributes,
                                binding.characters[index],
                            )?;
                            if motion.parameters.is_some() {
                                motion.inputs[index] = FlatMovement::from_character(
                                    &binding.attributes,
                                    binding.characters[index],
                                )?;
                            }
                        }
                    }
                }
                let elapsed = (f64::from(frame) * 1000.0 / 60.0).round() as u32;
                let delta = elapsed - motion.elapsed_ms;
                for index in 0..2 {
                    if self.visible[index] {
                        let presented = motion.presentation.as_mut().map(|p| &mut p[index]);
                        if let Some(rocking) = &mut motion.rocking {
                            rocking.sea.seconds = self.seconds;
                            rocking.bodies[index].parameters.offsets = motion.bodies[index].offsets;
                            let mut port = RockingMovement {
                                limits: &motion.inputs[index],
                                rocking: &mut rocking.bodies[index],
                                sea: &mut rocking.sea,
                            };
                            step_body(&mut motion.bodies[index], presented, delta, &mut port)?;
                        } else {
                            step_body(
                                &mut motion.bodies[index],
                                presented,
                                delta,
                                &mut motion.inputs[index],
                            )?;
                        }
                        if motion.presentation.is_some() {
                            let binding = motion.placement.as_mut().unwrap();
                            ship_presentation::publish_character(
                                &mut binding.attributes,
                                binding.characters[index],
                                &motion.bodies[index],
                            )?;
                        }
                    }
                }
                motion.elapsed_ms = elapsed;
            }
            for index in 0..2 {
                self.poses[index] = self.placed_pose(index, self.seconds)?;
                self.assets[index]
                    .ship_state
                    .particle_history
                    .as_ref()
                    .unwrap()
                    .record(self.poses[index].sample(f64::from(self.seconds)))?;
                if let Some(controller) = &self.sessions[index].controller {
                    let update = if self.drive.is_some() {
                        drive::update_effects
                    } else {
                        super::ship_fire::update
                    };
                    update(
                        &mut controller.borrow_mut(),
                        if frame == 0 { 0.0 } else { 1.0 / 60.0 },
                        self.poses[index],
                    )?;
                }
            }
            if !self.preview {
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
                        self.native_motion.as_mut().unwrap().bodies[0]
                            .process_message(&mut message)?;
                        self.snapshot("first_impulse_queued");
                    }
                    120 => {
                        self.sessions[0].controller.take();
                        self.sessions[0]
                            .world
                            .borrow_mut()
                            .release_owner(self.sessions[0].owner);
                        self.visible[0] = false;
                        if let Some(drive) = &mut self.drive {
                            drive.groups[0].deleted = true;
                        }
                        self.snapshot("first_released");
                    }
                    _ => {}
                }
            }
            self.next_frame = frame + 1;
        }
        Ok(())
    }
    fn present_at(&mut self, through: f32, record_fractional: bool) -> Result<()> {
        // Preserve exact requested draw time without inventing fractional births.
        self.seconds = through;
        self.sessions[0].world.borrow_mut().set_clock(through)?;
        for index in 0..2 {
            self.poses[index] = self.placed_pose(index, through)?;
            if record_fractional {
                self.assets[index]
                    .ship_state
                    .particle_history
                    .as_ref()
                    .unwrap()
                    .record(self.poses[index].sample(f64::from(through)))?;
            }
            self.sessions[index].apply(&mut self.assets[index].ship_state);
            self.assets[index].sync_particle_profiles()?;
        }
        Ok(())
    }
    pub fn advance(mut self, through: f32) -> Result<Self> {
        self.advance_frames(through)?;
        self.present_at(through, true)?;
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
        if let Some(drive) = &self.drive {
            report["native_sail_drive"] = drive.report();
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
                if let Some(drive) = &self.drive {
                    scene.set_rig_state(drive.rig(i))?;
                }
                if let Some(rocking) = self.native_motion.as_ref().and_then(|m| m.rocking.as_ref())
                {
                    scene.set_waves(rocking.waves)?;
                }
                Ok(scene)
            })
            .collect::<Result<Vec<_>>>()?;
        let wind = self.drive.as_ref().map_or(wind, drive::Drive::wind);
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
fn load(options: &Options) -> Result<Fleet> {
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
    if options.native_ship_rocking {
        fleet.enable_native_rocking()?;
    }
    if options.native_ship_placement {
        fleet.enable_native_placement(options.ship_immersion, options.ship_waterline)?;
    }
    if options.native_ship_presentation {
        fleet.enable_native_presentation()?;
    }
    if options.native_ship_parameters {
        let config = options
            .ship_parameter_file
            .as_deref()
            .map(parameters::read)
            .transpose()?;
        fleet.enable_native_parameters(config.as_ref())?;
    }
    if options.native_sail_drive {
        fleet.enable_native_drive(
            options.fleet_sail_setting,
            options.fleet_wind_direction.to_radians(),
        )?;
    }
    Ok(fleet)
}
pub fn run(options: &Options) -> Result<()> {
    if options.fleet_window {
        return window::run(options);
    }
    ensure!(
        options.frames.is_none(),
        "--frames requires --fleet-window for a fleet"
    );
    ensure!(
        options.headless.is_some() || options.particle_fleet_report.is_some(),
        "select --fleet-window, --headless or --particle-fleet-report"
    );
    let fleet = load(options)?.advance(options.time)?;
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
    fn parameter_fixture(config: Option<&Json>) -> Fleet {
        let mut f = generated().unwrap();
        f.enable_native_motion();
        f.enable_native_rocking().unwrap();
        f.enable_native_placement(0.75, 0.5).unwrap();
        f.enable_native_presentation().unwrap();
        f.enable_native_parameters(config).unwrap();
        f
    }
    fn sail_fixture(request: f32, direction: f32, damaged: bool) -> Fleet {
        let mut f = parameter_fixture(None);
        f.preview = true;
        if damaged {
            for a in &mut f.assets {
                for (i, m) in a.meshes.iter().enumerate() {
                    if let Some(p) = &m.sail {
                        a.ship_state
                            .sail_holes
                            .insert(i, (1 << p.section_count()) - 1);
                    }
                }
            }
        }
        f.enable_native_drive(request, direction).unwrap();
        f
    }
    #[test]
    fn pinned_drive_effect_frames_match_the_rendered_pose_orientation() {
        for yaw in [-2.5, 0.0, 0.75] {
            for pitch in [-0.25, 0.0, 0.125] {
                for roll in [-0.3, 0.0, 0.4] {
                    let pose = ModelPose {
                        translation: [3.0, -0.75, 2.0],
                        yaw,
                        pitch,
                        roll,
                    };
                    let pinned = drive::effect_matrix(pose).unwrap();
                    for (actual, expected) in pinned
                        .into_iter()
                        .zip(pose.matrix().unwrap().to_cols_array())
                    {
                        assert!((actual - expected).abs() <= 0.000001);
                    }
                    assert_eq!(&pinned[12..15], &pose.translation);
                }
            }
        }
        assert!(
            drive::effect_matrix(ModelPose {
                pitch: f32::NAN,
                ..Default::default()
            })
            .is_err()
        );
    }
    #[test]
    fn native_sail_drive_changes_real_motion_with_wind_sets_and_holes() {
        let full = sail_fixture(1.0, 0.0, false).advance(1.5).unwrap();
        let normal = sail_fixture(0.5, 0.0, false).advance(1.5).unwrap();
        let furled = sail_fixture(0.0, 0.0, false).advance(1.5).unwrap();
        let reverse = sail_fixture(1.0, std::f32::consts::PI, false)
            .advance(1.5)
            .unwrap();
        let holed = sail_fixture(1.0, 0.0, true).advance(1.5).unwrap();
        let speed = |f: &Fleet| f.native_motion.as_ref().unwrap().bodies[0].state.speed[2];
        assert!(speed(&full) > speed(&normal));
        assert!(speed(&normal) > speed(&furled));
        assert_eq!(speed(&furled), 0.0);
        assert_eq!(speed(&holed), 0.0);
        assert!(speed(&full) > speed(&reverse));
        assert_eq!(normal.drive.as_ref().unwrap().groups[0].current_set, 1);
        assert_eq!(
            furled.drive.as_ref().unwrap().rig(0),
            storm_sim::RigState::uniform(0.0)
        );
    }
    #[test]
    fn native_sail_selection_and_release_preserve_the_other_owner() {
        let mut changed = sail_fixture(1.0, 0.0, false);
        let mut baseline = sail_fixture(1.0, 0.0, false);
        for f in [&mut changed, &mut baseline] {
            f.advance_frames(0.5).unwrap();
            f.present_at(0.5, false).unwrap();
        }
        changed.drive.as_mut().unwrap().change(0, -0.5);
        changed.advance_frames(0.52).unwrap();
        baseline.advance_frames(0.52).unwrap();
        assert_eq!(
            changed.native_motion.as_ref().unwrap().bodies[0].strengths[0].speed[2],
            baseline.native_motion.as_ref().unwrap().bodies[0].strengths[0].speed[2]
        );
        assert_eq!(changed.drive.as_ref().unwrap().groups[0].current_set, 1);
        assert_eq!(changed.drive.as_ref().unwrap().groups[1].current_set, 2);
        for f in [&mut changed, &mut baseline] {
            f.advance_frames(1.0).unwrap();
        }
        assert_eq!(
            changed.native_motion.as_ref().unwrap().bodies[1].state,
            baseline.native_motion.as_ref().unwrap().bodies[1].state
        );
        let run = |time| {
            let mut f = sail_fixture(1.0, 0.0, false);
            f.preview = false;
            f.advance(time).unwrap()
        };
        let at = run(2.0);
        let after = run(3.0);
        assert_eq!(
            at.drive.as_ref().unwrap().report()["bodies"][0],
            after.drive.as_ref().unwrap().report()["bodies"][0]
        );
        assert_ne!(
            at.drive.as_ref().unwrap().report()["bodies"][1],
            after.drive.as_ref().unwrap().report()["bodies"][1]
        );
    }
    #[test]
    fn native_parameters_preserve_default_motion_and_external_attributes_change_the_real_body() {
        let configured = parameter_fixture(None).advance(3.0).unwrap();
        let mut base = generated().unwrap();
        base.enable_native_motion();
        base.enable_native_rocking().unwrap();
        base.enable_native_placement(0.75, 0.5).unwrap();
        base.enable_native_presentation().unwrap();
        let base = base.advance(3.0).unwrap();
        let a = base.motion_report().unwrap();
        let b = configured.motion_report().unwrap();
        for field in ["bodies", "rocking", "placement", "presentation"] {
            assert_eq!(a[field], b[field], "{field}");
        }
        let config: Json =
            serde_json::from_str(include_str!("../fixtures/ship_parameters.json")).unwrap();
        let external = parameter_fixture(Some(&config)).advance(3.0).unwrap();
        let m = external.native_motion.as_ref().unwrap();
        let p = m.parameters.as_ref().unwrap();
        assert_eq!(p.loaded[0].speed_rate, 18.0);
        assert_eq!(p.loaded[0].capacity, 3000);
        assert_eq!(p.mounted[0].mass_inertia, 0.1);
        assert_eq!(m.inputs[0].max_speed_z, 10.0);
        assert_eq!(m.inputs[0].max_speed_y, 0.06);
        assert_eq!(m.bodies[0].state.inertia_accel, [0.25, 0.75, 1.5]);
        assert_eq!(m.placement.as_ref().unwrap().states[0].waterline, 0.25);
        assert!(
            m.bodies[0].state.speed[2]
                < configured.native_motion.as_ref().unwrap().bodies[0]
                    .state
                    .speed[2]
        );
    }
    #[test]
    fn runtime_character_limits_refresh_independently_of_mounted_type_parameters() {
        let mut f = parameter_fixture(None);
        f.preview = true;
        f.advance_frames(1.0).unwrap();
        f.present_at(1.0, false).unwrap();
        let m = f.native_motion.as_mut().unwrap();
        let b = m.placement.as_mut().unwrap();
        let id = b
            .attributes
            .create_path(b.characters[0], "Ship.MaxSpeedZ")
            .unwrap();
        b.attributes.set_float(id, None, 3.0).unwrap();
        b.attributes
            .set_float(b.ship_types[0], Some("SpeedRate"), 99.0)
            .unwrap();
        f.advance_frames(1.02).unwrap();
        f.present_at(1.02, false).unwrap();
        let m = f.native_motion.as_ref().unwrap();
        assert_eq!(m.inputs[0].max_speed_z, 3.0);
        assert_eq!(m.inputs[1].max_speed_z, 14.0);
        let p = m.parameters.as_ref().unwrap();
        assert_eq!(p.loaded[0].speed_rate, 12.0);
        assert_eq!(p.mounted[0].nominal_speed_z, 12.0);
    }
    #[test]
    fn native_presentation_uses_pre_move_pose_and_publishes_post_move_state() {
        let mut f = generated().unwrap();
        f.preview = true;
        f.enable_native_motion();
        f.enable_native_rocking().unwrap();
        f.enable_native_placement(0.75, 0.5).unwrap();
        f.enable_native_presentation().unwrap();
        f.native_motion.as_mut().unwrap().bodies[0].state.rotation[1] = 0.6;
        f.advance_frames(1.5).unwrap();
        f.present_at(1.5, false).unwrap();
        let previous = f
            .native_motion
            .as_ref()
            .unwrap()
            .bodies
            .iter()
            .map(|b| b.state)
            .collect::<Vec<_>>();
        f.advance_frames(1.52).unwrap();
        f.present_at(1.52, false).unwrap();
        let m = f.native_motion.as_ref().unwrap();
        let poses = m.presentation.as_ref().unwrap();
        let report = f.motion_report().unwrap();
        for i in 0..2 {
            let current = m.bodies[i].state;
            assert_eq!(poses[i].position, previous[i].position);
            assert_eq!(poses[i].angles[1], previous[i].angles[1]);
            assert_eq!(
                poses[i].angles[2],
                previous[i].angles[2] + current.rotation[1].clamp(-0.25, 0.25)
            );
            assert_eq!(
                f.poses[i].translation[1].to_bits(),
                (previous[i].position[1] - 0.75 - 0.5).to_bits()
            );
            assert_ne!(current.position, previous[i].position);
            assert_eq!(
                report["presentation"]["bodies"][i]["script"]["pos"][1],
                storm_core::format_float(current.position[1])
            );
            assert_eq!(
                report["presentation"]["bodies"][i]["script"]["ang"][2],
                storm_core::format_float(current.angles[2])
            );
            let history = f.assets[i]
                .ship_state
                .particle_history
                .as_ref()
                .unwrap()
                .read()
                .unwrap();
            assert_eq!(history.last().unwrap().roll, f.poses[i].roll);
        }
    }
    #[test]
    fn presentation_and_script_outputs_freeze_only_for_the_released_owner() {
        let run = |time| {
            let mut f = generated().unwrap();
            f.enable_native_motion();
            f.enable_native_rocking().unwrap();
            f.enable_native_placement(0.75, 0.5).unwrap();
            f.enable_native_presentation().unwrap();
            f.advance(time).unwrap()
        };
        let at_release = run(2.0);
        let after = run(3.0);
        let a = at_release.motion_report().unwrap();
        let b = after.motion_report().unwrap();
        assert_eq!(
            a["presentation"]["bodies"][0],
            b["presentation"]["bodies"][0]
        );
        assert_ne!(
            a["presentation"]["bodies"][1],
            b["presentation"]["bodies"][1]
        );
        let whole = run(1.0);
        let fractional = run(1.005);
        assert_eq!(whole.motion_report(), fractional.motion_report());
        assert_eq!(
            whole.poses[0].matrix().unwrap(),
            fractional.poses[0].matrix().unwrap()
        );
    }
    #[test]
    fn immersion_and_waterline_shift_presentation_without_changing_native_motion() {
        let run = |placement| {
            let mut fleet = generated().unwrap();
            fleet.enable_native_motion();
            fleet.enable_native_rocking().unwrap();
            if placement {
                fleet.enable_native_placement(0.75, 0.5).unwrap();
            }
            fleet.advance(3.0).unwrap()
        };
        let original = run(false);
        let placed = run(true);
        for i in 0..2 {
            let old = original.native_motion.as_ref().unwrap().bodies[i].state;
            let new = placed.native_motion.as_ref().unwrap().bodies[i].state;
            assert_eq!(old, new);
            assert_eq!(
                placed.poses[i].translation[1].to_bits(),
                (new.position[1] - 0.75 - 0.5).to_bits()
            );
            assert_eq!(
                placed.poses[i].translation[0],
                original.poses[i].translation[0]
            );
            assert_eq!(
                placed.poses[i].translation[2],
                original.poses[i].translation[2]
            );
            let history = placed.assets[i]
                .ship_state
                .particle_history
                .as_ref()
                .unwrap()
                .read()
                .unwrap();
            assert_eq!(
                history.last().unwrap().translation[1],
                placed.poses[i].translation[1]
            );
        }
        assert!(
            !placed.report().unwrap()["native_motion"]["placement"]["original_ship_parameters"]
                .as_bool()
                .unwrap()
        );
    }
    #[test]
    fn preview_partition_and_repeated_presentation_preserve_fixed_motion_and_pose_history() {
        let create = || {
            let mut f = generated().unwrap();
            f.preview = true;
            f.enable_native_motion();
            f.enable_native_rocking().unwrap();
            f.enable_native_placement(0.25, 0.5).unwrap();
            f
        };
        let mut single = create();
        single.advance_frames(3.0).unwrap();
        single.present_at(3.0, false).unwrap();
        let mut split = create();
        for seconds in [0.0, 0.005, 0.2, 0.333, 1.005, 1.5, 3.0, 3.0] {
            split.advance_frames(seconds).unwrap();
            split.present_at(seconds, false).unwrap();
        }
        assert_eq!(single.motion_report(), split.motion_report());
        assert_eq!(split.visible, [true, true]);
        assert!(split.snapshots.is_empty());
        assert_eq!(
            split.sessions[0].world.borrow().report()["manager"]["systems"],
            0
        );
        for i in 0..2 {
            let a = single.assets[i]
                .ship_state
                .particle_history
                .as_ref()
                .unwrap()
                .read()
                .unwrap();
            let b = split.assets[i]
                .ship_state
                .particle_history
                .as_ref()
                .unwrap()
                .read()
                .unwrap();
            assert_eq!(&*a, &*b);
            assert_eq!(a.len(), 181);
        }
        let binding = split
            .native_motion
            .as_mut()
            .unwrap()
            .placement
            .as_mut()
            .unwrap();
        let attr = binding
            .attributes
            .create_path(binding.characters[0], "Ship.Immersion")
            .unwrap();
        binding.attributes.set_float(attr, None, 1.5).unwrap();
        split.advance_frames(3.05).unwrap();
        split.present_at(3.05, false).unwrap();
        let binding = split
            .native_motion
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap();
        assert_eq!(binding.states[0].immersion, 1.5);
        assert_eq!(binding.states[1].immersion, 0.25);
    }
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
    #[test]
    fn native_rocking_moves_independent_hulls_and_keeps_released_grid_frozen() {
        let mut fleet = generated().unwrap();
        fleet.assets[0].root_box_size = Some([9.0, 8.0, 40.0]);
        fleet.enable_native_motion();
        fleet.enable_native_rocking().unwrap();
        let fleet = fleet.advance(3.0).unwrap();
        let motion = fleet.native_motion.as_ref().unwrap();
        let rocking = motion.rocking.as_ref().unwrap();
        assert_eq!(rocking.bodies[0].parameters.size, [9.0, 40.0]);
        assert_eq!(rocking.bodies[1].parameters.size, [6.0, 16.0]);
        assert_ne!(rocking.bodies[0].heights, rocking.bodies[1].heights);
        for i in 0..2 {
            let state = motion.bodies[i].state;
            assert!(state.position[1] != 0.0 && state.angles[0] != 0.0 && state.angles[2] != 0.0);
            assert_eq!(fleet.poses[i].pitch, -state.angles[0]);
            assert_eq!(fleet.poses[i].roll, state.angles[2]);
            let history = fleet.assets[i]
                .ship_state
                .particle_history
                .as_ref()
                .unwrap()
                .read()
                .unwrap();
            assert_eq!(history.last().unwrap().pitch, fleet.poses[i].pitch);
            assert_eq!(history.last().unwrap().roll, fleet.poses[i].roll);
        }
        let report = fleet.report().unwrap();
        let released = report["snapshots"]
            .as_array()
            .unwrap()
            .iter()
            .find(|s| s["phase"] == "first_released")
            .unwrap();
        assert_eq!(
            released["native_motion"]["bodies"][0],
            report["native_motion"]["bodies"][0]
        );
        assert_eq!(
            released["native_motion"]["rocking"]["bodies"][0],
            report["native_motion"]["rocking"]["bodies"][0]
        );
        assert_ne!(
            released["native_motion"]["rocking"]["bodies"][1],
            report["native_motion"]["rocking"]["bodies"][1]
        );
    }
    #[test]
    fn sea_reflection_matches_render_coordinates_and_fractional_draw_does_not_advance_rocking() {
        let waves = storm_sim::Waves {
            height_m: 1.5,
            direction: 1.2,
        };
        let mut a = DiagnosticSea::new(waves).unwrap();
        let mut b = a.clone();
        a.seconds = 1.25;
        b.seconds = 1.25;
        assert_eq!(
            ReflectedSea(&mut a).wave_xz(3.0, 7.0).unwrap(),
            b.wave_xz(3.0, -7.0).unwrap()
        );
        let run = |seconds| {
            let mut fleet = generated().unwrap();
            fleet.enable_native_motion();
            fleet.enable_native_rocking().unwrap();
            fleet.advance(seconds).unwrap()
        };
        let whole = run(1.0);
        let fractional = run(1.005);
        assert_eq!(whole.motion_report(), fractional.motion_report());
        let mut invalid = generated().unwrap();
        invalid.enable_native_motion();
        invalid.assets[0].root_box_size = Some([0.0, 1.0, 2.0]);
        assert!(invalid.enable_native_rocking().is_err());
        assert!(invalid.native_motion.unwrap().rocking.is_none());
    }
}
