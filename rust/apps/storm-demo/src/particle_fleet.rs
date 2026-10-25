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

pub struct Fleet {
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
            ensure!(
                !asset.ship.fires.is_empty(),
                "fleet ships require fire locators"
            );
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
            assets,
            sessions,
            poses: vec![ModelPose::default(); 2],
            visible: [true; 2],
            snapshots: Vec::new(),
            seconds: 0.0,
        })
    }
    fn pose(&self, index: usize, seconds: f32) -> ModelPose {
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
        Ok(
            json!({"fixture":"two_ship_particle_service","original_gameplay":false,"seconds_bits":self.seconds.to_bits(),"snapshots":self.snapshots,
            "service":self.sessions[0].world.borrow().report(),
            "ships":self.assets.iter().enumerate().map(|(i,a)|Ok(json!({"owner":self.sessions[i].owner.raw(),"visible":self.visible[i],"pose":self.poses[i].matrix()?.to_cols_array(),"ship":super::rig_report::ship_frame(a,self.seconds)?}))).collect::<Result<Vec<_>>>()?}),
        )
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
    let fleet = if let Some(path) = &options.model {
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
    }
    .advance(options.time)?;
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
}
