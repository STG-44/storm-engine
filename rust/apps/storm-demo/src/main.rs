use anyhow::{Context, Result, bail};
use clap::{Parser, ValueEnum};
use std::{path::PathBuf, sync::Arc, time::Instant};
use storm_render::{
    DiagnosticScene, Gpu, create_instance,
    model::{LoadOptions, ModelAsset, ModelScene, OrbitCamera},
    render_model_offscreen, render_offscreen,
};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
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
    /// Fixed simulation time for deterministic headless captures.
    #[arg(long, default_value_t = 0.0, value_parser = finite)]
    time: f32,
    /// Initial orbit angle in degrees.
    #[arg(long, requires = "model", value_parser = finite)]
    yaw: Option<f32>,
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
    start: Instant,
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
            .unwrap_or_else(|| "diagnostic sea".to_owned());
        window.set_title(&format!(
            "Storm Rust — {scene_name} — {:?} — {}",
            info.backend, info.name
        ));
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
        let camera = asset.map(|asset| {
            let mut c = OrbitCamera::new(asset);
            if let Some(yaw) = options.yaw {
                c.yaw = yaw.to_radians();
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
            start: Instant::now(),
            frames: 0,
        };
        state.resize(size);
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
        let seconds = self.start.elapsed().as_secs_f32();
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
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                if let Some(camera) = state.camera.as_mut() {
                    match event.logical_key {
                        Key::Named(NamedKey::ArrowLeft) => camera.orbit(-8.0, 0.0),
                        Key::Named(NamedKey::ArrowRight) => camera.orbit(8.0, 0.0),
                        Key::Named(NamedKey::ArrowUp) => camera.orbit(0.0, 5.0),
                        Key::Named(NamedKey::ArrowDown) => camera.orbit(0.0, -5.0),
                        Key::Character(key) if key.eq_ignore_ascii_case("r") => {
                            if let Some(asset) = self.asset.as_ref() {
                                *camera = OrbitCamera::new(asset);
                            }
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::Focused(false) => {
                state.dragging = false;
                state.last_cursor = None;
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
    let asset = options.model.as_ref().map(|path| {
        let asset = ModelAsset::load(path, &LoadOptions {
            game_dir: options.game_dir.clone(),
            texture_dir: options.texture_dir.clone(),
            untextured: options.untextured,
            single_mesh: options.single_mesh,
        })?;
        eprintln!("Loaded {} geometry files, {} draw meshes, {} textures, {} static sails, {} triangles",
            asset.geometry_files, asset.meshes.len(), asset.textures.len() - 1,
            asset.sails, asset.meshes.iter().map(|m| m.indices.len() / 3).sum::<usize>());
        Ok::<_, anyhow::Error>(Arc::new(asset))
    }).transpose()?;
    if let Some(path) = &options.headless {
        let gpu = pollster::block_on(Gpu::request(&instance, None))?;
        let info = gpu.adapter.get_info();
        eprintln!("GPU: {} ({:?})", info.name, info.backend);
        let frame = if let Some(asset) = &asset {
            let mut camera = OrbitCamera::new(asset);
            if let Some(yaw) = options.yaw {
                camera.yaw = yaw.to_radians();
            }
            render_model_offscreen(
                &gpu,
                asset,
                &camera,
                options.width,
                options.height,
                options.time,
            )?
        } else {
            render_offscreen(&gpu, options.width, options.height, options.time)?
        };
        frame.save_png(path)?;
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
