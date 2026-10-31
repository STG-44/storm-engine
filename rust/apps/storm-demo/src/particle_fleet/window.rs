//! Bounded interactive viewer for the same native bodies and shared rendering.
use super::{Fleet, ModelScene, Options, OrbitCamera, load};
use crate::timeline::Timeline;
use anyhow::{Context, Result, bail};
use std::sync::Arc;
use storm_render::Gpu;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowId},
};

fn prepare(
    options: &Options,
    gpu: &Gpu,
    format: wgpu::TextureFormat,
    seconds: f32,
) -> Result<(Fleet, Vec<ModelScene>, OrbitCamera)> {
    let mut fleet = load(options)?;
    fleet.preview = true;
    if fleet.native_motion.as_ref().unwrap().placement.is_none() {
        fleet.enable_native_placement(0.0, 0.0)?;
    }
    if fleet.native_motion.as_ref().unwrap().parameters.is_none() {
        fleet.enable_native_parameters(None)?;
    }
    fleet.enable_native_presentation()?;
    if fleet.drive.is_none() {
        fleet.enable_native_drive(
            options.fleet_sail_setting,
            options.fleet_wind_direction.to_radians(),
        )?;
    }
    fleet
        .drive
        .as_mut()
        .unwrap()
        .enable_motion(&mut fleet.assets)?;
    fleet.native_motion.as_mut().unwrap().bodies[0].strengths[0].rotation[1] = 0.0;
    fleet.advance_frames(seconds)?;
    fleet.present_at(seconds, false)?;
    let mut camera = OrbitCamera::new(&fleet.assets[0]);
    camera.distance *= 1.7;
    camera.radius *= 3.0;
    camera.pitch = 0.3;
    camera.yaw = 0.2;
    let scenes = fleet
        .assets
        .iter()
        .enumerate()
        .map(|(i, a)| {
            let mut scene = ModelScene::new(gpu, format, a)?;
            scene.set_rig_state(fleet.drive.as_ref().unwrap().rig(i))?;
            Ok(scene)
        })
        .collect::<Result<Vec<_>>>()?;
    Ok((fleet, scenes, camera))
}

struct State {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    configured: bool,
    occluded: bool,
    fleet: Fleet,
    scenes: Vec<ModelScene>,
    camera: OrbitCamera,
    clock: Timeline,
    dragging: bool,
    cursor: Option<(f64, f64)>,
    frames: u64,
    left: bool,
    right: bool,
}
impl State {
    async fn new(
        instance: &wgpu::Instance,
        event_loop: &ActiveEventLoop,
        options: &Options,
    ) -> Result<Self> {
        let window = Arc::new(
            event_loop.create_window(
                Window::default_attributes()
                    .with_title("Storm — просмотр кораблей")
                    .with_inner_size(PhysicalSize::new(options.width, options.height)),
            )?,
        );
        let surface = instance.create_surface(window.clone())?;
        let gpu = Gpu::request(instance, Some(&surface)).await?;
        let info = gpu.adapter.get_info();
        eprintln!("Fleet viewer GPU: {} ({:?})", info.name, info.backend);
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .context("surface configuration")?;
        config.format = surface
            .get_capabilities(&gpu.adapter)
            .formats
            .into_iter()
            .find(wgpu::TextureFormat::is_srgb)
            .context("sRGB surface")?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        let (fleet, scenes, camera) = prepare(options, &gpu, config.format, options.time)?;
        let mut state = Self {
            window,
            surface,
            gpu,
            config,
            configured: false,
            occluded: false,
            fleet,
            scenes,
            camera,
            clock: Timeline::new(options.time, options.paused),
            dragging: false,
            cursor: None,
            frames: 0,
            left: false,
            right: false,
        };
        state.resize(size);
        state.window.focus_window();
        eprintln!(
            "Viewer: drag/arrows orbit; scroll zoom; Space pause; hold A/D steer and W/S change first ship sails; [/] immersion; B toggle fire; R restart the 30-second preview; Esc close. Both ships stay visible."
        );
        Ok(state)
    }
    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.configured = size.width > 0 && size.height > 0;
        if self.configured {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }
    fn immersion(&mut self, change: f32) -> Result<()> {
        let binding = self
            .fleet
            .native_motion
            .as_mut()
            .unwrap()
            .placement
            .as_mut()
            .unwrap();
        // This viewer bounds its user control; the native component retains raw finite values.
        for i in 0..2 {
            let value = (binding.states[i].immersion + change).clamp(-5.0, 10.0);
            let attr = binding
                .attributes
                .create_path(binding.characters[i], "Ship.Immersion")?;
            binding.attributes.set_float(attr, None, value)?;
            binding.states[i].calculate_immersion(&binding.attributes, binding.characters[i])?;
        }
        Ok(())
    }
    fn steer(&mut self) {
        self.fleet.native_motion.as_mut().unwrap().bodies[0].strengths[0].rotation[1] =
            if self.right {
                1.0
            } else if self.left {
                -1.0
            } else {
                0.0
            };
    }
    fn toggle_fire(&mut self) -> Result<()> {
        let active = self.fleet.sessions.iter().any(|s| {
            s.world
                .borrow()
                .emissions_for(s.owner)
                .iter()
                .any(|e| e.stopped.is_none())
        });
        for i in 0..2 {
            if active {
                self.fleet.sessions[i]
                    .controller
                    .as_ref()
                    .unwrap()
                    .borrow_mut()
                    .stop_all();
            } else {
                self.fleet.activate(i, false)?;
            }
        }
        Ok(())
    }
    fn draw(&mut self, instance: &wgpu::Instance) -> Result<()> {
        if !self.configured || self.occluded {
            return Ok(());
        }
        let (frame, reconfigure) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(f) => (f, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(f) => (f, true),
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(()),
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.occluded = true;
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize(self.window.inner_size());
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = instance.create_surface(self.window.clone())?;
                self.resize(self.window.inner_size());
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => bail!("fleet surface validation"),
        };
        let seconds = self.clock.seconds().min(30.0);
        if seconds >= 30.0 {
            self.clock.paused = true;
        }
        self.fleet.advance_frames(seconds)?;
        self.fleet.present_at(seconds, false)?;
        for i in 0..2 {
            self.scenes[i].set_pose(self.fleet.poses[i])?;
            self.scenes[i].set_ship_state(self.fleet.assets[i].ship_state.clone())?;
            self.scenes[i].set_rig_state(self.fleet.drive.as_ref().unwrap().rig(i))?;
            let waves = self
                .fleet
                .native_motion
                .as_ref()
                .unwrap()
                .rocking
                .as_ref()
                .unwrap()
                .waves;
            self.scenes[i].set_waves(waves)?;
        }
        self.camera.target.x =
            (self.fleet.poses[0].translation[0] + self.fleet.poses[1].translation[0]) * 0.5;
        self.camera.target.z =
            (self.fleet.poses[0].translation[2] + self.fleet.poses[1].translation[2]) * 0.5;
        let immersion = self
            .fleet
            .native_motion
            .as_ref()
            .unwrap()
            .placement
            .as_ref()
            .unwrap()
            .states[0]
            .immersion;
        self.window.set_title(&format!("Storm — корабли • A/D: руль • W/S: паруса • осадка {immersion:.2} м • {seconds:.1}/30 с{} • Пробел: пауза • R: сначала",
            if self.clock.paused {" • пауза"} else {""}));
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        ModelScene::draw_fleet(
            &mut self.scenes,
            &self.gpu,
            &mut encoder,
            &view,
            self.config.width,
            self.config.height,
            seconds,
            &self.camera,
            self.fleet.drive.as_ref().unwrap().wind(),
        )?;
        self.gpu.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
        self.frames += 1;
        if reconfigure {
            self.resize(self.window.inner_size());
        }
        Ok(())
    }
}

struct Application<'a> {
    instance: wgpu::Instance,
    options: &'a Options,
    state: Option<State>,
    failure: Option<anyhow::Error>,
}
impl Application<'_> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        eprintln!("Fleet viewer: {error:#}");
        self.failure = Some(error);
        event_loop.exit();
    }
    fn event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) -> Result<()> {
        let s = self.state.as_mut().unwrap();
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => s.resize(size),
            WindowEvent::Occluded(value) => s.occluded = value,
            WindowEvent::Focused(false) => {
                s.dragging = false;
                s.cursor = None;
                s.left = false;
                s.right = false;
                s.steer();
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => s.dragging = state == ElementState::Pressed,
            WindowEvent::CursorMoved { position, .. } => {
                if s.dragging
                    && let Some((x, y)) = s.cursor
                {
                    s.camera.yaw -= (position.x - x) as f32 * 0.006;
                    s.camera.pitch =
                        (s.camera.pitch + (position.y - y) as f32 * 0.006).clamp(-0.2, 1.4);
                }
                s.cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let lines = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y,
                    MouseScrollDelta::PixelDelta(p) => p.y as f32 / 40.0,
                };
                s.camera.distance = (s.camera.distance * 0.9_f32.powf(lines))
                    .clamp(s.camera.radius * 0.1, s.camera.radius * 20.0);
            }
            WindowEvent::KeyboardInput { event, .. }
                if matches!(
                    event.physical_key,
                    PhysicalKey::Code(KeyCode::KeyA | KeyCode::KeyD)
                ) =>
            {
                let pressed = event.state == ElementState::Pressed;
                if event.physical_key == PhysicalKey::Code(KeyCode::KeyA) {
                    s.left = pressed;
                } else {
                    s.right = pressed;
                }
                s.steer();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                match event.physical_key {
                    PhysicalKey::Code(KeyCode::Escape) => event_loop.exit(),
                    PhysicalKey::Code(KeyCode::Space) if !event.repeat => s.clock.toggle(),
                    PhysicalKey::Code(KeyCode::BracketLeft) => s.immersion(-0.25)?,
                    PhysicalKey::Code(KeyCode::BracketRight) => s.immersion(0.25)?,
                    PhysicalKey::Code(KeyCode::KeyB) if !event.repeat => s.toggle_fire()?,
                    PhysicalKey::Code(KeyCode::KeyW) if !event.repeat => {
                        s.fleet.drive.as_mut().unwrap().change(0, 0.5)
                    }
                    PhysicalKey::Code(KeyCode::KeyS) if !event.repeat => {
                        s.fleet.drive.as_mut().unwrap().change(0, -0.5)
                    }
                    PhysicalKey::Code(KeyCode::KeyR) if !event.repeat => {
                        let (fleet, scenes, camera) =
                            prepare(self.options, &s.gpu, s.config.format, 0.0)?;
                        s.fleet = fleet;
                        s.scenes = scenes;
                        s.camera = camera;
                        s.clock = Timeline::new(0.0, self.options.paused);
                        s.left = false;
                        s.right = false;
                    }
                    PhysicalKey::Code(KeyCode::ArrowLeft) => s.camera.yaw -= 0.08,
                    PhysicalKey::Code(KeyCode::ArrowRight) => s.camera.yaw += 0.08,
                    PhysicalKey::Code(KeyCode::ArrowUp) => {
                        s.camera.pitch = (s.camera.pitch + 0.08).min(1.4)
                    }
                    PhysicalKey::Code(KeyCode::ArrowDown) => {
                        s.camera.pitch = (s.camera.pitch - 0.08).max(-0.2)
                    }
                    _ => {}
                }
            }
            WindowEvent::RedrawRequested => {
                s.draw(&self.instance)?;
                if self.options.frames.is_some_and(|n| s.frames >= n) {
                    eprintln!("Presented {} fleet frames", s.frames);
                    event_loop.exit();
                }
            }
            _ => {}
        }
        Ok(())
    }
}
impl ApplicationHandler for Application<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            match pollster::block_on(State::new(&self.instance, event_loop, self.options)) {
                Ok(s) => self.state = Some(s),
                Err(e) => self.fail(event_loop, e),
            }
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        if self.state.as_ref().is_some_and(|s| s.window.id() == id)
            && let Err(e) = self.event(event_loop, event)
        {
            self.fail(event_loop, e);
        }
    }
    fn about_to_wait(&mut self, _: &ActiveEventLoop) {
        if let Some(s) = &self.state
            && s.configured
            && !s.occluded
        {
            s.window.request_redraw();
        }
    }
}
pub(super) fn run(options: &Options) -> Result<()> {
    let instance = storm_render::create_instance(options.backend.flags())?;
    let event_loop = EventLoop::new()?;
    let mut app = Application {
        instance,
        options,
        state: None,
        failure: None,
    };
    event_loop.run_app(&mut app)?;
    app.failure.map_or(Ok(()), Err)
}
