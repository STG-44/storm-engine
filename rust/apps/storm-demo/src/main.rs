use anyhow::{Context, Result, bail};
use clap::{ArgGroup, Parser, ValueEnum};
use std::{path::PathBuf, sync::Arc};
mod sailing;
mod timeline;
use storm_render::{
    DiagnosticScene, Gpu, create_instance,
    model::{LoadOptions, ModelAsset, ModelPose, ModelScene, OrbitCamera, SailWind},
    render_model_offscreen_at_pose, render_offscreen,
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
    group(ArgGroup::new("preview").args(["model", "sail_demo"])),
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
    /// Game installation or its RESOURCE directory (original files stay outside Git).
    #[arg(long, requires = "model")]
    game_dir: Option<PathBuf>,
    /// Texture search directory relative to RESOURCE/Textures; default ship skin is Hull1.
    #[arg(long, requires = "game_dir")]
    texture_dir: Option<PathBuf>,
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
    /// Preview wind speed, 0..=40 m/s. This does not drive ship physics yet.
    #[arg(long, requires = "preview", default_value_t = 8.0, value_parser = wind_speed)]
    wind_speed: f32,
    /// Wind travel direction: 0 degrees is +Z, 90 degrees is +X.
    #[arg(long, requires = "preview", default_value_t = 45.0, value_parser = finite, allow_negative_numbers = true)]
    wind_direction: f32,
    /// Start the native window with animation paused (Space toggles pause).
    #[arg(long, conflicts_with = "headless")]
    paused: bool,
    /// Enable approximate ship motion for the original model (W/S sail power, A/D rudder).
    #[arg(long, requires = "model")]
    sail: bool,
    /// Initial course in degrees: 0=+Z, 90=+X. Default: follow wind direction.
    #[arg(long, requires = "sail", value_parser = finite, allow_negative_numbers = true)]
    heading: Option<f32>,
    /// Effective exposed sail area fraction, 0..=1 (does not animate furling yet).
    #[arg(long, requires = "sail", default_value_t = 1.0, value_parser = sail_setting)]
    sail_setting: f32,
    /// Initial held rudder, -1 port to +1 starboard; useful for deterministic replay.
    #[arg(long, requires = "sail", default_value_t = 0.0, value_parser = rudder, allow_negative_numbers = true)]
    rudder: f32,
    /// Save headless simulation state as JSON, alongside the rendered PNG.
    #[arg(long, requires_all = ["sail", "headless"])]
    state_json: Option<PathBuf>,
}

fn sail_setting(value: &str) -> std::result::Result<f32, String> {
    let v = finite(value)?;
    if (0.0..=1.0).contains(&v) {
        Ok(v)
    } else {
        Err("sail setting must be 0..=1".into())
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
        let scene = match asset {
            Some(asset) => Scene::Model(Box::new(ModelScene::new(&gpu, config.format, asset)?)),
            None => Scene::Diagnostic(Box::new(DiagnosticScene::new(&gpu.device, config.format))),
        };
        let sailing = sailing::Session::new(options)?;
        let camera = asset.map(|asset| {
            let mut c = OrbitCamera::new(asset);
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
            model_center: asset.map_or([0.0; 3], |a| a.center.to_array()),
            last_title_time: options.time,
            frames: 0,
        };
        state.resize(size);
        state.refresh_title();
        eprintln!(
            "Controls: drag/arrows orbit, scroll zoom, R reset, Space pause, +/- wind speed, ,/. wind direction, Esc close"
        );
        if state.sailing.is_some() {
            eprintln!(
                "Sailing: physical W/S sail power, hold A/D rudder; release to centre (works with any keyboard layout)"
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
        self.window
            .set_title(&format!("{}{wind}{sailing}{pause}", self.title));
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        self.configured = size.width > 0 && size.height > 0;
        if self.configured {
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.gpu.device, &self.config);
        }
    }

    fn draw(&mut self, instance: &wgpu::Instance) -> Result<()> {
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
        let mut frame_wind = self.wind;
        if let Some(sailing) = &mut self.sailing {
            sailing.update(seconds, self.wind)?;
            let pose = sailing.pose();
            frame_wind = sailing.cloth_wind(self.wind);
            if let Scene::Model(scene) = &mut self.scene {
                scene.set_pose(pose)?;
            }
            if let Some(camera) = &mut self.camera {
                camera.target = pose.transform_point(self.model_center.into())?;
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
                seconds,
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
            WindowEvent::RedrawRequested => match state.draw(&self.instance) {
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
            },
            _ => {}
        }
    }
}

fn main() -> Result<()> {
    let options = Options::parse();
    let instance = create_instance(options.backend.flags())?;
    if options.gpu_info {
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
    let asset = if options.sail_demo {
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
                        untextured: options.untextured,
                        single_mesh: options.single_mesh,
                    },
                )?;
                Ok::<_, anyhow::Error>(Arc::new(asset))
            })
            .transpose()?
    };
    if let Some(asset) = &asset {
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
    if let Some(path) = &options.headless {
        let sailing = sailing::Session::new(&options)?;
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let info = gpu.adapter.get_info();
        eprintln!("GPU: {} ({:?})", info.name, info.backend);
        let frame = if let Some(asset) = &asset {
            let mut camera = OrbitCamera::new(asset);
            if let Some(yaw) = options.yaw {
                camera.yaw = yaw.to_radians();
            } else if let Some(sailing) = &sailing {
                camera.yaw += sailing.initial_yaw;
            }
            let pose = sailing
                .as_ref()
                .map_or_else(ModelPose::default, sailing::Session::pose);
            camera.target = pose.transform_point(asset.center)?;
            let wind = sailing
                .as_ref()
                .map_or_else(|| options.wind(), |s| s.cloth_wind(options.wind()));
            render_model_offscreen_at_pose(
                &gpu,
                asset,
                &camera,
                wind,
                pose,
                options.width,
                options.height,
                options.time,
            )?
        } else {
            render_offscreen(&gpu, options.width, options.height, options.time)?
        };
        frame.save_png(path)?;
        if let Some(sailing) = &sailing {
            let report = sailing.report();
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
    };
    event_loop
        .run_app(&mut application)
        .context("running native application")?;
    match application.failure {
        Some(error) => Err(error),
        None => Ok(()),
    }
}
