use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser, ValueEnum};
use std::{path::PathBuf, sync::Arc};
mod damage_events;
mod flags;
mod particle_fleet;
mod rig_report;
mod sailing;
mod ship_controls;
mod ship_fire;
mod ship_script;
mod timeline;
use storm_render::{
    DiagnosticScene, Gpu, create_instance,
    model::{LoadOptions, ModelAsset, ModelPose, ModelScene, OrbitCamera, SailWind},
    render_offscreen, render_sailing_offscreen,
};
use timeline::Timeline;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, KeyCode, NamedKey, PhysicalKey},
    window::{Window, WindowId},
};

#[derive(Clone, Copy, Debug, Default, ValueEnum)]
enum Backend {
    #[default]
    Auto,
    Metal,
    Vulkan,
    Dx12,
}

impl Backend {
    fn flags(self) -> wgpu::Backends {
        match self {
            Self::Auto => wgpu::Backends::PRIMARY,
            Self::Metal => wgpu::Backends::METAL,
            Self::Vulkan => wgpu::Backends::VULKAN,
            Self::Dx12 => wgpu::Backends::DX12,
        }
    }
}

#[derive(Parser)]
#[command(
    version,
    group(ArgGroup::new("preview").args(["model", "sail_demo", "rigging_demo","ship_demo"])),
    about = "Storm Rust: native diagnostic sea and original GM/TX model preview"
)]
struct Options {
    #[arg(long, value_enum, default_value_t)]
    backend: Backend,
    /// Print available adapters as JSON and exit.
    #[arg(long, conflicts_with_all = ["headless", "frames"])]
    gpu_info: bool,
    /// Render a deterministic PNG without creating a window.
    #[arg(long, value_name = "PNG", conflicts_with = "frames")]
    headless: Option<PathBuf>,
    #[arg(long, default_value_t = 1280, value_parser = clap::value_parser!(u32).range(1..=8192))]
    width: u32,
    #[arg(long, default_value_t = 720, value_parser = clap::value_parser!(u32).range(1..=8192))]
    height: u32,
    /// Exit after this many presented frames (for native window smoke checks).
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    frames: Option<u64>,
    /// Original GM file to preview instead of the resource-free diagnostic.
    #[arg(long)]
    model: Option<PathBuf>,
    /// Preview generated quad/triangle cloth without original game resources.
    #[arg(long)]
    sail_demo: bool,
    /// Resource-free ropes, shrouds, quad flags and pennants fixture.
    #[arg(long)]
    rigging_demo: bool,
    /// Resource-free crew, fire, masts and hull damage fixture.
    #[arg(long)]
    ship_demo: bool,
    /// Two ships share the particle service; deterministic fire/restart/release fixture.
    #[arg(long, conflicts_with_all = ["sail_demo", "rigging_demo", "ship_demo", "sail", "gpu_info"])]
    particle_fleet_demo: bool,
    /// Use retained SHIP inertia/movement and impulse dispatch for the fleet fixture.
    #[arg(long, requires = "particle_fleet_demo")]
    native_ship_motion: bool,
    /// Add retained 36-point ShipRocking over the diagnostic wave surface.
    #[arg(long, requires = "native_ship_motion")]
    native_ship_rocking: bool,
    /// Apply native character immersion and mounted waterline to the model root.
    #[arg(long, requires = "native_ship_motion")]
    native_ship_placement: bool,
    /// Use living Execute's delayed model pose, turn heel and script state outputs.
    #[arg(long, requires_all = ["native_ship_placement", "native_ship_rocking"])]
    native_ship_presentation: bool,
    /// Diagnostic character Ship.Immersion in metres (original scripts are not loaded).
    #[arg(long, requires = "native_ship_placement", default_value_t = 0.0, value_parser = finite, allow_negative_numbers = true)]
    ship_immersion: f32,
    /// Diagnostic ship-type WaterLine in metres.
    #[arg(long, requires = "native_ship_placement", default_value_t = 0.0, value_parser = finite, allow_negative_numbers = true)]
    ship_waterline: f32,
    /// Open an interactive two-ship viewer instead of the scripted removal fixture.
    #[arg(long, requires_all = ["particle_fleet_demo", "native_ship_motion", "native_ship_rocking"], conflicts_with_all = ["headless", "particle_fleet_report"])]
    fleet_window: bool,
    /// Export the shared-service fleet scenario without requiring a GPU.
    #[arg(long, requires = "particle_fleet_demo")]
    particle_fleet_report: Option<PathBuf>,
    /// Game installation or its RESOURCE directory (original files stay outside Git).
    #[arg(long, requires = "model")]
    game_dir: Option<PathBuf>,
    /// Texture search directory relative to RESOURCE/Textures; default ship skin is Hull1.
    #[arg(long, requires = "game_dir")]
    texture_dir: Option<PathBuf>,
    /// Cache an additional XPS profile for native fire messages (repeatable).
    #[arg(long, requires = "game_dir", conflicts_with = "single_mesh")]
    particle_profile: Vec<String>,
    /// Particle project name, "default", or an absolute descriptor path.
    #[arg(long, requires = "game_dir", conflicts_with = "single_mesh")]
    particle_project: Option<PathBuf>,
    /// Render geometry with a white material, without loading commercial textures.
    #[arg(long, requires = "model", conflicts_with = "game_dir")]
    untextured: bool,
    /// Preview only the supplied GM, without children or locator-generated sails.
    #[arg(long, requires = "model")]
    single_mesh: bool,
    /// Initial animation time; fixed for deterministic headless captures.
    #[arg(long, default_value_t = 0.0, value_parser = finite, allow_negative_numbers = true)]
    time: f32,
    /// Initial orbit angle in degrees.
    #[arg(long, requires = "preview", value_parser = finite, allow_negative_numbers = true)]
    yaw: Option<f32>,
    /// Wind speed, 0..=40 m/s; drives cloth and optional sailing physics.
    #[arg(long, requires = "preview", default_value_t = 8.0, value_parser = wind_speed)]
    wind_speed: f32,
    /// Wind travel direction: 0 degrees is +Z, 90 degrees is +X.
    #[arg(long, requires = "preview", default_value_t = 45.0, value_parser = finite, allow_negative_numbers = true)]
    wind_direction: f32,
    /// Start the native window with animation paused (Space toggles pause).
    #[arg(long, conflicts_with = "headless")]
    paused: bool,
    /// Enable approximate ship motion (W/S select original sail sets, A/D rudder).
    #[arg(long, requires = "preview")]
    sail: bool,
    /// Initial course in degrees: 0=+Z, 90=+X. Default: follow wind direction.
    #[arg(long, requires = "sail", value_parser = finite, allow_negative_numbers = true)]
    heading: Option<f32>,
    /// Initial sail set: 0 furled, 0.5 normal, 1 full.
    #[arg(long, requires = "preview", default_value_t = 1.0, value_parser = sail_setting)]
    sail_setting: f32,
    /// Requested sail set after starting: 0, 0.5, or 1.
    #[arg(long, requires = "sail", value_parser = sail_setting)]
    sail_target: Option<f32>,
    /// Run a bounded startup source with the ported SHIP sail/query messages.
    #[arg(long, requires = "sail")]
    sail_script: Option<PathBuf>,
    /// Save script and sailing state, then exit without creating a GPU device.
    #[arg(long, requires = "sail_script", conflicts_with_all = ["headless", "frames", "gpu_info"])]
    sail_script_report: Option<PathBuf>,
    /// Initial held rudder, -1 port to +1 starboard; useful for deterministic replay.
    #[arg(long, requires = "sail", default_value_t = 0.0, value_parser = rudder, allow_negative_numbers = true)]
    rudder: f32,
    /// Save headless simulation state as JSON, alongside the rendered PNG.
    #[arg(long, requires_all = ["sail", "headless"])]
    state_json: Option<PathBuf>,
    /// Crest-to-trough wave envelope, 0..=4 metres (default in sailing mode: 1.5).
    #[arg(long, requires = "sail", value_parser = wave_height)]
    wave_height: Option<f32>,
    /// Wave travel direction in degrees (default: initial wind direction).
    #[arg(long, requires = "sail", value_parser = finite, allow_negative_numbers = true)]
    wave_direction: Option<f32>,
    /// Export original per-sail set membership and exit without creating a GPU device.
    #[arg(long, requires = "preview", conflicts_with_all = ["headless", "frames", "gpu_info"])]
    rig_report: Option<PathBuf>,
    /// TEHO ship nation: england, france, spain, holland or pirate.
    #[arg(long, requires = "preview")]
    flag_nation: Option<storm_sim::FlagNation>,
    /// Special-flag role: personal (default), commander, naval or merchant.
    #[arg(long, requires = "flag_nation")]
    flag_role: Option<storm_sim::ShipFlagRole>,
    /// Surrender changes non-pirate special flags to white.
    #[arg(long, requires = "flag_nation")]
    surrendered: bool,
    /// Explicit pirate atlas column; character/group lookup is not running.
    #[arg(long, requires="flag_nation",value_parser=clap::value_parser!(u32).range(0..=3))]
    pirate_flag: Option<u32>,
    /// Hide flag surfaces while retaining ropes, poles and sails.
    #[arg(long, requires="preview", conflicts_with_all=["flag_nation","flag_role","surrendered","pirate_flag"])]
    hide_flags: bool,
    /// Visible original sailors, 0..=50; default five when original routes exist.
    #[arg(long,requires="game_dir",value_parser=clap::value_parser!(u32).range(0..=50))]
    crew: Option<u32>,
    /// Start a fire/smoke preview at original fireplace locators.
    #[arg(long, requires = "preview")]
    fire: bool,
    /// Break a mast and its original numbered topmast group at time zero.
    #[arg(long,requires="preview",value_parser=clap::value_parser!(u32).range(1..=100000))]
    break_mast: Vec<u32>,
    /// Detach a named original shatter hull node, for example shatter4.
    #[arg(long, requires = "preview")]
    break_hull: Vec<String>,
    /// Fire reproducible swept test shots into N sections of every deployed sail.
    #[arg(long,requires="preview",value_parser=clap::value_parser!(u32).range(1..=12))]
    sail_volley: Option<u32>,
    /// Write observed original sail event payloads; also updated by native X shots.
    #[arg(long, requires = "preview")]
    event_report: Option<PathBuf>,
    /// Explicit preview attacker character index; no character scripts are loaded.
    #[arg(long, requires = "preview", default_value_t = -1, allow_negative_numbers = true)]
    shot_owner: i32,
    /// Explicit preview target character index; -1 suppresses DoSailHole as in C++.
    #[arg(long, requires = "preview", default_value_t = -1, allow_negative_numbers = true)]
    ship_character: i32,
    /// Focus the initial camera on the original deck walking network.
    #[arg(long, requires = "preview")]
    deck_view: bool,
}

fn wave_height(value: &str) -> std::result::Result<f32, String> {
    let v = finite(value)?;
    if (0.0..=4.0).contains(&v) {
        Ok(v)
    } else {
        Err("wave height must be 0..=4 metres".into())
    }
}

fn sail_setting(value: &str) -> std::result::Result<f32, String> {
    let v = finite(value)?;
    if [0.0, 0.5, 1.0].contains(&v) {
        Ok(v)
    } else {
        Err("sail set must be 0 (furled), 0.5 (normal), or 1 (full)".into())
    }
}
fn rudder(value: &str) -> std::result::Result<f32, String> {
    let v = finite(value)?;
    if (-1.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err("rudder must be -1..=1".into())
    }
}

impl Options {
    fn flag_controls(&self) -> Result<flags::Controls> {
        if self.pirate_flag.is_some() && self.flag_nation != Some(storm_sim::FlagNation::Pirate) {
            bail!("--pirate-flag requires --flag-nation pirate");
        }
        Ok(flags::Controls::new(
            self.flag_nation.map(|nation| storm_sim::ShipFlagState {
                nation,
                role: self.flag_role.unwrap_or_default(),
                surrendered: self.surrendered,
                pirate_variant: self.pirate_flag.unwrap_or(0),
            }),
            self.hide_flags,
        ))
    }
    fn wind(&self) -> SailWind {
        SailWind {
            speed: self.wind_speed,
            direction: self.wind_direction.rem_euclid(360.0).to_radians(),
        }
    }
}

fn wind_speed(value: &str) -> std::result::Result<f32, String> {
    let speed = finite(value)?;
    if (0.0..=40.0).contains(&speed) {
        Ok(speed)
    } else {
        Err("wind speed must be 0..=40 m/s".into())
    }
}

fn finite(value: &str) -> std::result::Result<f32, String> {
    let v = value.parse::<f32>().map_err(|e| e.to_string())?;
    if v.is_finite() {
        Ok(v)
    } else {
        Err("value must be finite".to_owned())
    }
}

enum Scene {
    Diagnostic(Box<DiagnosticScene>),
    Model(Box<ModelScene>),
}

struct WindowState {
    surface: wgpu::Surface<'static>,
    window: Arc<Window>,
    gpu: Gpu,
    scene: Scene,
    camera: Option<OrbitCamera>,
    dragging: bool,
    last_cursor: Option<(f64, f64)>,
    config: wgpu::SurfaceConfiguration,
    configured: bool,
    occluded: bool,
    timeline: Timeline,
    wind: SailWind,
    title: String,
    sailing: Option<sailing::Session>,
    model_center: [f32; 3],
    last_title_time: f32,
    frames: u64,
    flag_controls: flags::Controls,
    ship_state: storm_render::model::ShipState,
}

impl WindowState {
    async fn new(
        instance: &wgpu::Instance,
        event_loop: &ActiveEventLoop,
        options: &Options,
        asset: Option<&ModelAsset>,
    ) -> Result<Self> {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_title("Storm Rust — renderer diagnostic")
                        .with_inner_size(PhysicalSize::new(options.width, options.height)),
                )
                .context("creating native window")?,
        );
        let surface = instance
            .create_surface(window.clone())
            .context("creating window surface")?;
        let gpu = Gpu::request(instance, Some(&surface)).await?;
        let info = gpu.adapter.get_info();
        eprintln!(
            "GPU: {} ({:?}, {:?})",
            info.name, info.backend, info.device_type
        );
        let scene_name = options
            .model
            .as_ref()
            .and_then(|p| p.file_stem())
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                if options.sail_demo {
                    "sail demo"
                } else {
                    "diagnostic sea"
                }
                .to_owned()
            });
        let title = format!(
            "Storm Rust — {scene_name} — {:?} — {}",
            info.backend, info.name
        );
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .context("surface has no supported configuration")?;
        let capabilities = surface.get_capabilities(&gpu.adapter);
        config.format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .context("surface has no sRGB color format")?;
        config.present_mode = wgpu::PresentMode::AutoVsync;
        let mut scene = match asset {
            Some(asset) => Scene::Model(Box::new(ModelScene::new(&gpu, config.format, asset)?)),
            None => Scene::Diagnostic(Box::new(DiagnosticScene::new(&gpu.device, config.format))),
        };
        let sailing = sailing::Session::new(
            options,
            asset.map_or_else(storm_sim::RigLayout::default, ModelAsset::rig_layout),
            asset.map(|asset| &asset.ship),
        )?;
        if let Scene::Model(scene) = &mut scene {
            scene.set_rig_state(storm_sim::RigState::for_mode(
                storm_sim::SailMode::from_setting(f64::from(options.sail_setting))?,
            ))?;
        }
        let camera = asset.map(|asset| {
            let mut c = OrbitCamera::new(asset);
            if options.deck_view {
                c.target = asset.ship.deck_center.unwrap_or(asset.center);
                c.distance = 20.0;
                c.pitch = 0.45;
            }
            if let Some(yaw) = options.yaw {
                c.yaw = yaw.to_radians();
            } else if let Some(sailing) = &sailing {
                c.yaw += sailing.initial_yaw;
            }
            c
        });
        let mut state = Self {
            surface,
            window,
            gpu,
            scene,
            camera,
            dragging: false,
            last_cursor: None,
            config,
            configured: false,
            occluded: false,
            timeline: Timeline::new(options.time, options.paused),
            wind: options.wind(),
            title,
            sailing,
            model_center: asset.map_or([0.0; 3], |a| {
                if options.deck_view {
                    a.ship.deck_center.unwrap_or(a.center).to_array()
                } else {
                    a.center.to_array()
                }
            }),
            last_title_time: options.time,
            frames: 0,
            flag_controls: options.flag_controls()?,
            ship_state: asset.map_or_else(Default::default, |a| a.ship_state.clone()),
        };
        state.resize(size);
        if let Some(sailing) = &state.sailing
            && sailing.apply_fire(&mut state.ship_state)
            && let Scene::Model(scene) = &mut state.scene
        {
            scene.set_ship_state(state.ship_state.clone())?;
        }
        state.refresh_title();
        eprintln!(
            "Controls: drag/arrows orbit, scroll zoom, R camera, Space pause, +/- wind speed, ,/. wind direction; physical F nation, G flag role, H hide flags, J surrender, C crew, B fire, M break next mast, P break next hull piece, X test shot, 0 repair/reset; Esc close"
        );
        if state.sailing.is_some() {
            eprintln!(
                "Sailing: physical W/S select sail sets, hold A/D rudder; release to centre (works with any keyboard layout)"
            );
        }
        Ok(state)
    }

    fn refresh_title(&self) {
        let pause = if self.timeline.paused {
            " — paused"
        } else {
            ""
        };
        let wind = if self.camera.is_some() {
            format!(
                " — wind {:.0} m/s {:.0}°",
                self.wind.speed,
                self.wind.direction.to_degrees().rem_euclid(360.0)
            )
        } else {
            String::new()
        };
        let sailing = self
            .sailing
            .as_ref()
            .map_or_else(String::new, sailing::Session::title);
        let flags = if self.camera.is_some() {
            self.flag_controls.title()
        } else {
            String::new()
        };
        let damage = if self.camera.is_some() {
            format!(
                " — crew {} / fire {} / broken M{} H{}",
                if self.ship_state.crew_visible {
                    "on"
                } else {
                    "off"
                },
                self.ship_state
                    .native_fire
                    .as_ref()
                    .map_or(self.ship_state.fire_started.is_some(), |v| v
                        .iter()
                        .any(|e| e.stopped.is_none())),
                self.ship_state.broken_masts.len(),
                self.ship_state.broken_hulls.len()
            )
        } else {
            String::new()
        };
        self.window.set_title(&format!(
            "{}{wind}{sailing}{flags}{damage}{pause}",
            self.title
        ));
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.configured = size.width > 0 && size.height > 0;
        if self.configured {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    fn draw(
        &mut self,
        instance: &wgpu::Instance,
        events: &mut damage_events::DamageEvents,
    ) -> Result<()> {
        if !self.configured || self.occluded {
            return Ok(());
        }
        let (frame, reconfigure) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
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
                self.surface = instance
                    .create_surface(self.window.clone())
                    .context("recreating lost surface")?;
                self.resize(self.window.inner_size());
                return Ok(());
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                bail!("surface acquisition failed graphics validation")
            }
        };
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        let target = frame.texture.create_view(&Default::default());
        let seconds = self.timeline.seconds();
        if !self.timeline.paused {
            events.frame(self.timeline.milliseconds())?;
        }
        let mut frame_seconds = seconds;
        let mut frame_wind = self.wind;
        if let Some(sailing) = &mut self.sailing {
            sailing.update(seconds, self.wind)?;
            frame_seconds = sailing.simulation.time() as f32;
            let pose = sailing.pose();
            frame_wind = sailing.cloth_wind(self.wind);
            if let Scene::Model(scene) = &mut self.scene {
                scene.set_pose(pose)?;
                scene.set_waves(sailing.simulation.waves())?;
                scene.set_rig_state(sailing.simulation.rig())?;
                if sailing.apply_fire(&mut self.ship_state) {
                    scene.set_ship_state(self.ship_state.clone())?;
                }
            }
            if let Some(camera) = &mut self.camera {
                // Follow horizontal travel without rolling the camera with the hull.
                camera.target = self.model_center.into();
                camera.target.x += pose.translation[0];
                camera.target.z += pose.translation[2];
            }
        }
        if seconds - self.last_title_time >= 0.25 {
            self.refresh_title();
            self.last_title_time = seconds;
        }
        match &mut self.scene {
            Scene::Diagnostic(scene) => scene.draw(
                &self.gpu.queue,
                &mut encoder,
                &target,
                self.config.width,
                self.config.height,
                seconds,
            ),
            Scene::Model(scene) => scene.draw(
                &self.gpu,
                &mut encoder,
                &target,
                self.config.width,
                self.config.height,
                frame_seconds,
                self.camera.as_ref().expect("model camera"),
                frame_wind,
            )?,
        }
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

struct Application {
    instance: wgpu::Instance,
    options: Options,
    state: Option<WindowState>,
    failure: Option<anyhow::Error>,
    asset: Option<Arc<ModelAsset>>,
    damage_events: damage_events::DamageEvents,
}

impl Application {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.failure = Some(error);
        event_loop.exit();
    }
}

impl ApplicationHandler for Application {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_none() {
            match pollster::block_on(WindowState::new(
                &self.instance,
                event_loop,
                &self.options,
                self.asset.as_deref(),
            )) {
                Ok(state) => {
                    state.window.request_redraw();
                    self.state = Some(state);
                }
                Err(error) => self.fail(event_loop, error),
            }
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // Drop the surface and window together; recreate both on resume.
        self.state = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };
        if state.window.id() != window_id {
            return;
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::KeyboardInput { event, .. }
                if event.state == ElementState::Pressed
                    && event.logical_key == Key::Named(NamedKey::Escape) =>
            {
                event_loop.exit()
            }
            WindowEvent::MouseInput {
                state: button,
                button: MouseButton::Left,
                ..
            } => {
                state.dragging = button == ElementState::Pressed;
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let (Some((x, y)), Some(camera)) = (state.last_cursor, state.camera.as_mut())
                    && state.dragging
                {
                    camera.orbit((position.x - x) as f32, (position.y - y) as f32);
                }
                state.last_cursor = Some((position.x, position.y));
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if let Some(camera) = state.camera.as_mut() {
                    camera.zoom(match delta {
                        MouseScrollDelta::LineDelta(_, y) => y,
                        MouseScrollDelta::PixelDelta(p) => p.y as f32 / 60.0,
                    });
                }
            }
            WindowEvent::KeyboardInput { event, .. }
                if state.sailing.is_some()
                    && matches!(
                        event.physical_key,
                        PhysicalKey::Code(
                            KeyCode::KeyW | KeyCode::KeyS | KeyCode::KeyA | KeyCode::KeyD
                        )
                    ) =>
            {
                if let PhysicalKey::Code(code) = event.physical_key
                    && let Some(sailing) = &mut state.sailing
                    && let Err(error) = sailing.key(code, event.state)
                {
                    self.fail(event_loop, error);
                    return;
                }
                state.refresh_title();
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if !event.repeat
                    && let PhysicalKey::Code(code) = event.physical_key
                    && let Some(asset) = &self.asset
                {
                    let seconds = state
                        .sailing
                        .as_ref()
                        .map_or_else(|| state.timeline.seconds(), |s| s.simulation.time() as f32);
                    let changed = if code == KeyCode::KeyX {
                        let pose = state
                            .sailing
                            .as_ref()
                            .map_or_else(Default::default, |s| s.pose());
                        let rig = state.sailing.as_ref().map_or_else(
                            || {
                                storm_sim::RigState::for_mode(
                                    storm_sim::SailMode::from_setting(f64::from(
                                        self.options.sail_setting,
                                    ))
                                    .expect("validated sail set"),
                                )
                            },
                            |s| s.simulation.rig(),
                        );
                        let wind = state
                            .sailing
                            .as_ref()
                            .map_or_else(|| state.wind, |s| s.cloth_wind(state.wind));
                        let (from, to) =
                            state.camera.as_ref().expect("model camera").shot_segment();
                        match asset.trace_sail_shot(
                            &mut state.ship_state,
                            from,
                            to,
                            wind,
                            seconds,
                            rig,
                            pose,
                        ) {
                            Ok(hits) => {
                                if let Err(error) = self.damage_events.shot(
                                    asset,
                                    &hits,
                                    self.options.shot_owner,
                                    self.options.ship_character,
                                ) {
                                    self.fail(event_loop, error);
                                    return;
                                }
                                eprintln!("Cannonball segment: {} sail intersections", hits.len());
                                !hits.is_empty()
                            }
                            Err(error) => {
                                self.fail(event_loop, error);
                                return;
                            }
                        }
                    } else {
                        let native =
                            match state.sailing.as_mut().map(|s| s.fire_key(code)).transpose() {
                                Ok(value) => value.flatten(),
                                Err(error) => {
                                    self.fail(event_loop, error);
                                    return;
                                }
                            };
                        native.unwrap_or_else(|| {
                            ship_controls::key(&asset.ship, &mut state.ship_state, code, seconds)
                        })
                    };
                    if changed {
                        let update_attributes = if code == KeyCode::Digit0 {
                            self.damage_events
                                .clear_sail_damage(self.options.ship_character)
                        } else {
                            Ok(())
                        };
                        if let Err(error) = update_attributes.and_then(|()| {
                            self.options.event_report.as_ref().map_or(Ok(()), |path| {
                                self.damage_events
                                    .write(path, asset, self.options.ship_character)
                            })
                        }) {
                            self.fail(event_loop, error);
                            return;
                        }
                        if let Scene::Model(scene) = &mut state.scene
                            && let Err(error) = scene.set_ship_state(state.ship_state.clone())
                        {
                            self.fail(event_loop, error);
                            return;
                        }
                        if let Some(sailing) = &mut state.sailing
                            && let Err(error) = sailing
                                .simulation
                                .set_rig_layout(asset.rig_layout_at(&state.ship_state, seconds))
                        {
                            self.fail(event_loop, error.into());
                            return;
                        }
                    }
                }
                if !event.repeat
                    && let PhysicalKey::Code(code) = event.physical_key
                    && matches!(&state.scene, Scene::Model(_))
                    && state.flag_controls.key(code)
                {
                    if let Scene::Model(scene) = &mut state.scene
                        && let Err(error) = scene.set_flags(state.flag_controls.selection())
                    {
                        self.fail(event_loop, error);
                        return;
                    }
                    eprintln!("{}", state.flag_controls.title());
                }
                if event.logical_key == Key::Named(NamedKey::Space) && !event.repeat {
                    state.timeline.toggle();
                }
                if let Some(camera) = state.camera.as_mut() {
                    match event.logical_key {
                        Key::Named(NamedKey::ArrowLeft) => camera.orbit(-8.0, 0.0),
                        Key::Named(NamedKey::ArrowRight) => camera.orbit(8.0, 0.0),
                        Key::Named(NamedKey::ArrowUp) => camera.orbit(0.0, 5.0),
                        Key::Named(NamedKey::ArrowDown) => camera.orbit(0.0, -5.0),
                        Key::Character(key) if key.eq_ignore_ascii_case("r") => {
                            if let Some(asset) = self.asset.as_ref() {
                                *camera = OrbitCamera::new(asset);
                                if let Some(sailing) = &state.sailing {
                                    camera.yaw += sailing.pose().yaw;
                                }
                            }
                        }
                        Key::Character(key) if matches!(key.as_str(), "+" | "=") => {
                            state.wind.speed = (state.wind.speed + 1.0).min(40.0);
                        }
                        Key::Character(key) if key == "-" => {
                            state.wind.speed = (state.wind.speed - 1.0).max(0.0);
                        }
                        Key::Character(key) if key == "," || key == "." => {
                            let delta = if key == "," { -15_f32 } else { 15_f32 };
                            state.wind.direction = (state.wind.direction + delta.to_radians())
                                .rem_euclid(std::f32::consts::TAU);
                        }
                        _ => {}
                    }
                }
                state.refresh_title();
            }
            WindowEvent::Focused(false) => {
                state.dragging = false;
                state.last_cursor = None;
                if let Some(sailing) = &mut state.sailing
                    && let Err(error) = sailing.release_rudder()
                {
                    self.fail(event_loop, error);
                }
            }
            WindowEvent::Resized(size) => {
                state.resize(size);
                state.window.request_redraw();
            }
            WindowEvent::Occluded(occluded) => {
                state.occluded = occluded;
                if !occluded {
                    state.window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                match state.draw(&self.instance, &mut self.damage_events) {
                    Ok(()) => {
                        if self
                            .options
                            .frames
                            .is_some_and(|limit| state.frames >= limit)
                        {
                            eprintln!("Presented {} frames", state.frames);
                            event_loop.exit();
                        } else if state.configured && !state.occluded {
                            state.window.request_redraw();
                        }
                    }
                    Err(error) => self.fail(event_loop, error),
                }
            }
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    let options = Options::parse();
    if options.particle_fleet_demo {
        return particle_fleet::run(&options);
    }
    if options.gpu_info {
        let instance = create_instance(options.backend.flags())?;
        let adapters = pollster::block_on(instance.enumerate_adapters(options.backend.flags()));
        let info: Vec<_> = adapters
            .into_iter()
            .map(|adapter| {
                let info = adapter.get_info();
                serde_json::json!({ "name": info.name, "backend": format!("{:?}", info.backend),
                "device_type": format!("{:?}", info.device_type), "driver": info.driver,
                "max_texture_dimension_2d": adapter.limits().max_texture_dimension_2d })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&info)?);
        return Ok(());
    }
    let mut damage_events = damage_events::DamageEvents::new()?;
    let mut asset = if options.ship_demo {
        Some(Arc::new(ModelAsset::ship_demo()?))
    } else if options.rigging_demo {
        Some(Arc::new(ModelAsset::rigging_demo()?))
    } else if options.sail_demo {
        Some(Arc::new(ModelAsset::sail_demo()?))
    } else {
        options
            .model
            .as_ref()
            .map(|path| {
                let asset = ModelAsset::load(
                    path,
                    &LoadOptions {
                        game_dir: options.game_dir.clone(),
                        texture_dir: options.texture_dir.clone(),
                        particle_profiles: options.particle_profile.clone(),
                        particle_project: options.particle_project.clone(),
                        untextured: options.untextured,
                        single_mesh: options.single_mesh,
                        crew: options.crew.map(|n| n as usize),
                    },
                )?;
                Ok::<_, anyhow::Error>(Arc::new(asset))
            })
            .transpose()?
    };
    let selection = options.flag_controls()?.selection();
    if let Some(asset) = &mut asset {
        let asset = Arc::get_mut(asset).expect("newly loaded unique asset");
        asset.set_flags(selection)?;
        asset.ship_state.broken_masts = options.break_mast.iter().map(|&n| (n, 0.0)).collect();
        asset.ship_state.broken_hulls = options
            .break_hull
            .iter()
            .map(|n| (n.clone(), 0.0))
            .collect();
        asset.ship_state.fire_started = options.fire.then_some(0.0);
        if options.fire && asset.ship.fires.is_empty() {
            bail!("this model has no fireplace locators; use --ship-demo for a generated fixture");
        }
        asset.ship_state.validate(&asset.ship)?;
        if let Some(sections) = options.sail_volley {
            let mut damage = asset.ship_state.clone();
            let hits = asset.sail_volley_with(
                &mut damage,
                sections,
                options.wind(),
                0.0,
                storm_sim::RigState::for_mode(
                    storm_sim::SailMode::from_setting(f64::from(options.sail_setting))
                        .expect("validated sail set"),
                ),
                |hits| damage_events.shot(asset, hits, options.shot_owner, options.ship_character),
            )?;
            eprintln!(
                "Test cannonball segments: {} intersections, {} newly damaged sections",
                hits.len(),
                hits.iter().filter(|h| h.new_damage).count()
            );
            asset.ship_state = damage;
        }
        eprintln!(
            "Ship: {} visible crew, {} fire points, masts {:?}, hull nodes {:?}",
            asset.ship.crew_count,
            asset.ship.fires.len(),
            asset.ship.masts(),
            asset.ship.hulls()
        );
    }
    if let Some(asset) = &asset {
        let r = asset.rigging_stats;
        eprintln!(
            "Rigging: {} ropes, {} shroud sets, {} flags; unmatched ropes {}, incomplete shrouds {}, incomplete flags {}",
            r.ropes,
            r.shrouds,
            r.flags,
            r.unmatched_ropes,
            r.incomplete_shrouds,
            r.incomplete_flags
        );
        eprintln!(
            "Loaded {} geometry files, {} draw meshes, {} texture slots, {} sails ({} triangular), {} triangles",
            asset.geometry_files,
            asset.meshes.len(),
            asset.textures.len(),
            asset.sails,
            asset.triangular_sails(),
            asset
                .meshes
                .iter()
                .map(|m| m.indices.len() / 3)
                .sum::<usize>()
        );
    }
    if let Some(path) = &options.event_report {
        damage_events.write(
            path,
            asset.as_deref().expect("model required by CLI"),
            options.ship_character,
        )?;
    }
    if let Some(path) = &options.rig_report {
        let asset = Arc::get_mut(asset.as_mut().expect("model required by CLI"))
            .expect("unique report asset");
        let sailing = sailing::Session::new(&options, asset.rig_layout(), Some(&asset.ship))?;
        if let Some(sailing) = &sailing {
            sailing.apply_fire(&mut asset.ship_state);
        }
        asset.sync_particle_profiles()?;
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&rig_report::report(
                asset,
                sailing
                    .as_ref()
                    .map_or(options.time, |s| s.simulation.time() as f32),
            )?)?,
        )?;
        return Ok(());
    }
    if let Some(path) = &options.sail_script_report {
        let sailing = sailing::Session::new(
            &options,
            asset
                .as_ref()
                .map_or_else(storm_sim::RigLayout::default, |a| a.rig_layout()),
            asset.as_ref().map(|asset| &asset.ship),
        )?
        .expect("sailing required by CLI");
        std::fs::write(path, serde_json::to_vec_pretty(&sailing.report())?)?;
        return Ok(());
    }
    let instance = create_instance(options.backend.flags())?;
    if let Some(path) = &options.headless {
        let sailing = sailing::Session::new(
            &options,
            asset
                .as_ref()
                .map_or_else(storm_sim::RigLayout::default, |a| a.rig_layout()),
            asset.as_ref().map(|asset| &asset.ship),
        )?;
        if let (Some(sailing), Some(asset)) = (&sailing, &mut asset) {
            sailing.apply_fire(
                &mut Arc::get_mut(asset)
                    .expect("unique headless asset")
                    .ship_state,
            );
        }
        if let Some(asset) = &mut asset {
            Arc::get_mut(asset)
                .expect("unique headless asset")
                .sync_particle_profiles()?;
        }
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let info = gpu.adapter.get_info();
        eprintln!("GPU: {} ({:?})", info.name, info.backend);
        let frame = if let Some(asset) = &asset {
            let mut camera = OrbitCamera::new(asset);
            if options.deck_view {
                camera.target = asset.ship.deck_center.unwrap_or(asset.center);
                camera.distance = 20.0;
                camera.pitch = 0.45;
            }
            if let Some(yaw) = options.yaw {
                camera.yaw = yaw.to_radians();
            } else if let Some(sailing) = &sailing {
                camera.yaw += sailing.initial_yaw;
            }
            let pose = sailing
                .as_ref()
                .map_or_else(ModelPose::default, sailing::Session::pose);
            if !options.deck_view {
                camera.target = asset.center;
            }
            camera.target.x += pose.translation[0];
            camera.target.z += pose.translation[2];
            let wind = sailing
                .as_ref()
                .map_or_else(|| options.wind(), |s| s.cloth_wind(options.wind()));
            render_sailing_offscreen(
                &gpu,
                asset,
                &camera,
                wind,
                pose,
                sailing
                    .as_ref()
                    .map_or_else(storm_sim::Waves::default, |s| s.simulation.waves()),
                sailing.as_ref().map_or(
                    storm_sim::RigState::for_mode(storm_sim::SailMode::from_setting(f64::from(
                        options.sail_setting,
                    ))?),
                    |s| s.simulation.rig(),
                ),
                options.width,
                options.height,
                sailing
                    .as_ref()
                    .map_or(options.time, |s| s.simulation.time() as f32),
            )?
        } else {
            render_offscreen(&gpu, options.width, options.height, options.time)?
        };
        frame.save_png(path)?;
        if let Some(sailing) = &sailing {
            let mut report = sailing.report();
            if let Some(asset) = &asset {
                report["ship"] = rig_report::ship_frame(asset, sailing.simulation.time() as f32)?;
            }
            eprintln!("Sailing state: {}", serde_json::to_string(&report)?);
            if let Some(state_path) = &options.state_json {
                std::fs::write(state_path, serde_json::to_vec_pretty(&report)?)?;
            }
        }
        eprintln!(
            "Saved {}x{} frame to {}",
            options.width,
            options.height,
            path.display()
        );
        return Ok(());
    }
    let event_loop = EventLoop::new().context("creating native event loop")?;
    let mut application = Application {
        instance,
        options,
        state: None,
        failure: None,
        asset,
        damage_events,
    };
    event_loop
        .run_app(&mut application)
        .context("running native application")?;
    match application.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
