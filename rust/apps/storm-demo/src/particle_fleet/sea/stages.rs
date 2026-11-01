use super::*;
use storm_sim::sea::camera::Matrix;
#[derive(Default)]
pub(super) struct Stages {
    trace: Vec<Json>,
    execute: Option<(u32, u64, [f32; 2])>,
    last_view: Option<Matrix>,
}
impl State {
    pub fn staged(&self) -> bool {
        self.stages.is_some()
    }
    pub fn enable_stages(&mut self, view: Matrix) -> Result<()> {
        ensure!(
            self.weather.is_some(),
            "stages require weather/entity binding"
        );
        self.stages = Some(Stages::default());
        self.weather.as_mut().unwrap().enable_stages()?;
        self.bind_view(view);
        self.weather.as_mut().unwrap().realize(0)?;
        Ok(())
    }
    fn bind_view(&mut self, view: Matrix) {
        self.sea.borrow_mut().set_realize_view(view);
        self.stages.as_mut().unwrap().last_view = Some(view);
    }
    pub fn begin_frame(&mut self, frame: u32, delta: u32, view: Matrix) -> Result<()> {
        self.bind_view(view);
        let session = self.weather.as_mut().unwrap();
        session.advance(delta)?;
        if !session.ship_staged() {
            session.execute_boundary(delta)?;
        }
        let sea = self.sea.borrow();
        self.stages.as_mut().unwrap().execute = Some((frame, sea.revision(), sea.surface().camera));
        Ok(())
    }
    pub fn end_frame(&mut self, delta: u32, view: Matrix) -> Result<()> {
        self.bind_view(view);
        let applied = self.weather.as_mut().unwrap().realize(delta)?;
        let sea = self.sea.borrow();
        let (frame, revision, camera) = self
            .stages
            .as_mut()
            .unwrap()
            .execute
            .take()
            .ok_or_else(|| anyhow::anyhow!("missing Execute boundary"))?;
        self.stages.as_mut().unwrap().trace.push(json!({"frame":frame,"delta_ms":delta,"execute_revision":revision,"realize_revision":sea.revision(),"query_camera_bits":camera.map(f32::to_bits),"render_camera_bits":sea.surface().camera.map(f32::to_bits),"applied":applied}));
        Ok(())
    }
    /// Paused viewer camera refresh: no ship step or wave-time advancement.
    /// Repeated presentation of the same view is deliberately cached.
    pub fn refresh_view(&mut self, view: Matrix) -> Result<()> {
        let previous = self
            .stages
            .as_ref()
            .unwrap()
            .last_view
            .map(|m| m.map(f32::to_bits));
        if previous != Some(view.map(f32::to_bits)) {
            self.bind_view(view);
            self.weather.as_mut().unwrap().realize(0)?;
        }
        Ok(())
    }
    pub fn stages_report(&self) -> Json {
        let sea = self.sea.borrow();
        let camera = sea.realized_camera();
        let mut report = json!({"order":"script_execute_realize","ships":"retained_numeric_body_calls_inside_execute_boundary","sea":self.weather.as_ref().unwrap().stages_report(),
            "revision":sea.revision(),"view_bits":camera.map(|c|c.view.map(f32::to_bits)),"eye_bits":camera.map(|c|c.eye.map(f32::to_bits)),"world_offset_bits":camera.map(|c|c.world_offset.map(f32::to_bits)),
            "trace":self.stages.as_ref().unwrap().trace,"full_sea_entity":false});
        if let Some(ships) = self.weather.as_ref().unwrap().ships_report() {
            report["ships"] = json!("core_entity_execute_numeric_body");
            report["ship_entities"] = ships;
        }
        report
    }
    pub fn ship_staged(&self) -> bool {
        self.weather
            .as_ref()
            .is_some_and(weather::Session::ship_staged)
    }
    pub fn enable_ship_stages(&mut self, frames: Vec<ship_stages::Frame>) -> Result<()> {
        self.weather
            .as_mut()
            .unwrap()
            .enable_ship_stages(self.sea.clone(), frames)
    }
    pub fn execute_ships(
        &mut self,
        delta: u32,
        frames: Vec<Option<ship_stages::Frame>>,
    ) -> Result<Vec<Option<ship_stages::Frame>>> {
        self.weather.as_mut().unwrap().execute_ships(delta, frames)
    }
    pub fn erase_ship(&mut self, index: usize) -> Result<()> {
        self.weather.as_mut().unwrap().erase_ship(index)
    }
    pub fn finish_lifecycle(&mut self) {
        self.weather.as_mut().unwrap().finish_lifecycle();
    }
}
impl Fleet {
    pub(in crate::particle_fleet) fn sea_staged(&self) -> bool {
        self.native_motion
            .as_ref()
            .and_then(|m| m.rocking.as_ref())
            .and_then(|r| r.native.as_ref())
            .is_some_and(State::staged)
    }
    pub(in crate::particle_fleet) fn stage_camera(
        &self,
        external: Option<&OrbitCamera>,
    ) -> Result<OrbitCamera> {
        let mut camera = if let Some(c) = external {
            let mut c = c.clone();
            c.target.x = (self.poses[0].translation[0] + self.poses[1].translation[0]) * 0.5;
            c.target.z = (self.poses[0].translation[2] + self.poses[1].translation[2]) * 0.5;
            c
        } else {
            self.camera()
        };
        camera.set_native_sea_view(true);
        Ok(camera)
    }
    pub(in crate::particle_fleet) fn refresh_sea_view(
        &mut self,
        camera: &OrbitCamera,
    ) -> Result<()> {
        if let Some(state) = self
            .native_motion
            .as_mut()
            .and_then(|m| m.rocking.as_mut())
            .and_then(|r| r.native.as_mut())
            .filter(|s| s.staged())
        {
            state.refresh_view(camera.sea_view()?)?;
        }
        Ok(())
    }
    pub(in crate::particle_fleet) fn enable_native_sea_stages(&mut self) -> Result<()> {
        let camera = self.stage_camera(None)?;
        self.native_motion
            .as_mut()
            .and_then(|m| m.rocking.as_mut())
            .and_then(|r| r.native.as_mut())
            .ok_or_else(|| anyhow::anyhow!("missing native sea"))?
            .enable_stages(camera.sea_view()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fleet() -> Fleet {
        let mut f = generated().unwrap();
        f.enable_native_motion();
        f.enable_native_rocking().unwrap();
        f.enable_native_sea(None).unwrap();
        f.enable_native_sea_weather().unwrap();
        f.enable_native_sea_stages().unwrap();
        f
    }
    #[test]
    fn rocking_queries_previous_surface_before_realize_and_stage_partition_is_stable() {
        let mut f = fleet();
        for body in &mut f.native_motion.as_mut().unwrap().bodies {
            body.strengths[0].speed = [0.0; 3];
            body.strengths[0].rotation = [0.0; 3];
            body.state.angles[1] = 0.0;
        }
        f.advance_frames(0.25).unwrap();
        let m = f.native_motion.as_ref().unwrap();
        let r = m.rocking.as_ref().unwrap();
        let old = r.native.as_ref().unwrap().sea.borrow().surface().clone();
        let x = m.bodies[0].state.position[0] - r.bodies[0].parameters.size[0] * 0.5;
        let z = m.bodies[0].state.position[2] - r.bodies[0].parameters.size[1] * 0.5;
        let expected = old.sample(x, z).unwrap().height;
        f.advance_frames(0.267).unwrap();
        let r = f.native_motion.as_ref().unwrap().rocking.as_ref().unwrap();
        assert_eq!(r.bodies[0].heights[0][0].to_bits(), expected.to_bits());
        assert_ne!(
            r.native
                .as_ref()
                .unwrap()
                .sea
                .borrow()
                .surface()
                .sample(x, z)
                .unwrap()
                .height
                .to_bits(),
            expected.to_bits()
        );
        let mut a = fleet();
        let mut b = fleet();
        a.advance_frames(3.0).unwrap();
        for t in [0.25, 0.75, 1.5, 1.75, 2.25, 3.0] {
            b.advance_frames(t).unwrap();
            b.present_at(t, false).unwrap();
        }
        assert_eq!(a.motion_report(), b.motion_report());
        let report = a.motion_report().unwrap();
        let stages = &report["rocking"]["sea"]["stages"];
        assert_eq!(stages["trace"].as_array().unwrap().len(), 181);
        for frame in stages["trace"].as_array().unwrap() {
            let before = frame["execute_revision"].as_u64().unwrap();
            let after = frame["realize_revision"].as_u64().unwrap();
            assert_eq!(
                after,
                before + u64::from(frame["applied"].as_bool().unwrap())
            );
        }
    }
    #[test]
    fn paused_camera_refresh_updates_cutoff_without_physics_and_stop_retains_camera() {
        let mut f = fleet();
        f.preview = true;
        f.advance_frames(1.25).unwrap();
        f.present_at(1.25, false).unwrap();
        let before = f.motion_report().unwrap();
        let mut camera = f.camera();
        camera.distance = 4000.0;
        f.refresh_sea_view(&camera).unwrap();
        let moved = f.motion_report().unwrap();
        assert_eq!(before["bodies"], moved["bodies"]);
        assert_eq!(
            before["rocking"]["sea"]["frame_bits"],
            moved["rocking"]["sea"]["frame_bits"]
        );
        assert_ne!(
            before["rocking"]["sea"]["camera_bits"],
            moved["rocking"]["sea"]["camera_bits"]
        );
        let sea = f
            .native_motion
            .as_ref()
            .unwrap()
            .rocking
            .as_ref()
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .sea
            .borrow();
        assert_eq!(sea.surface().sample(0.0, 0.0).unwrap().height, 0.0);
        drop(sea);
        f.refresh_sea_view(&camera).unwrap();
        assert_eq!(moved, f.motion_report().unwrap());
        f.advance_frames_with_camera(1.75, Some(&camera)).unwrap();
        let stopped = f.motion_report().unwrap();
        camera.distance = 50.0;
        f.refresh_sea_view(&camera).unwrap();
        let held = f.motion_report().unwrap();
        assert_eq!(
            stopped["rocking"]["sea"]["stages"]["eye_bits"],
            held["rocking"]["sea"]["stages"]["eye_bits"]
        );
        assert_eq!(
            stopped["rocking"]["sea"]["frame_bits"],
            held["rocking"]["sea"]["frame_bits"]
        );
        f.advance_frames_with_camera(2.25, Some(&camera)).unwrap();
        let resumed = f.motion_report().unwrap();
        assert_ne!(
            held["rocking"]["sea"]["stages"]["eye_bits"],
            resumed["rocking"]["sea"]["stages"]["eye_bits"]
        );
    }
}
