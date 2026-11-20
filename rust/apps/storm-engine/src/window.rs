//! Desktop window adapter over the same configured engine and model services.
use crate::input;
use anyhow::{Context, Result, bail, ensure};
use std::{
    cell::RefCell,
    rc::Rc,
    sync::Arc,
    time::{Duration, Instant},
};
use storm_core::Message;
use storm_render::{
    Gpu,
    fade::{FadeCompositor, FadeDraw},
    loading::{LoadingPictures, LoadingScreen},
};
use storm_runtime::{
    ControlInput, Engine, Phase, WindowSettings,
    clock::Timer,
    interface::LoadingProgress,
    model::{Draw, ModelScenes, Models},
};
use winit::{
    application::ApplicationHandler,
    dpi::{PhysicalPosition, PhysicalSize},
    event::{DeviceEvent, DeviceId, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Fullscreen, Window, WindowId},
};

pub struct Options {
    pub settings: WindowSettings,
    pub frames: Option<u64>,
    pub fixed_delta: Option<i32>,
    pub backends: wgpu::Backends,
}
struct Surface {
    surface: wgpu::Surface<'static>,
    window: Arc<Window>,
    gpu: Gpu,
    config: wgpu::SurfaceConfiguration,
    configured: bool,
    occluded: bool,
    scenes: ModelScenes,
    loading: Option<(Option<Rc<LoadingPictures>>, LoadingScreen)>,
    fade: FadeCompositor,
}
impl Surface {
    async fn new(
        instance: &wgpu::Instance,
        event_loop: &ActiveEventLoop,
        settings: &WindowSettings,
    ) -> Result<Self> {
        let monitor = event_loop
            .available_monitors()
            .nth(settings.display)
            .context("configured display is not available")?;
        let position = monitor.position();
        let size = monitor.size();
        let x = i64::from(position.x) + (i64::from(size.width) - i64::from(settings.width)) / 2;
        let y = i64::from(position.y) + (i64::from(size.height) - i64::from(settings.height)) / 2;
        let mut attributes = Window::default_attributes()
            .with_title("Sea Dogs")
            .with_inner_size(PhysicalSize::new(settings.width, settings.height))
            .with_position(PhysicalPosition::new(i32::try_from(x)?, i32::try_from(y)?))
            .with_decorations(settings.borders)
            .with_resizable(false)
            .with_visible(false);
        // Portable fullscreen adapter; exclusive display-mode behavior is pending.
        if settings.fullscreen {
            attributes = attributes.with_fullscreen(Some(Fullscreen::Borderless(Some(monitor))));
        }
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .context("create engine window")?,
        );
        let surface = instance
            .create_surface(window.clone())
            .context("create engine surface")?;
        // Original main shows the window before initializing game services.
        window.set_visible(true);
        let gpu = Gpu::request(instance, Some(&surface)).await?;
        let size = window.inner_size();
        let mut config = surface
            .get_default_config(&gpu.adapter, size.width.max(1), size.height.max(1))
            .context("engine surface has no supported configuration")?;
        config.format = surface
            .get_capabilities(&gpu.adapter)
            .formats
            .into_iter()
            .find(wgpu::TextureFormat::is_srgb)
            .context("engine surface has no sRGB format")?;
        config.present_mode = if settings.vsync {
            wgpu::PresentMode::AutoVsync
        } else {
            wgpu::PresentMode::AutoNoVsync
        };
        let scenes = ModelScenes::new(config.format);
        let fade = FadeCompositor::new(&gpu, config.format);
        let mut state = Self {
            surface,
            window,
            gpu,
            config,
            configured: false,
            occluded: false,
            scenes,
            loading: None,
            fade,
        };
        state.resize(size)?;
        Ok(state)
    }
    fn resize(&mut self, size: PhysicalSize<u32>) -> Result<()> {
        self.configured = size.width > 0 && size.height > 0;
        if self.configured {
            let limit = self.gpu.device.limits().max_texture_dimension_2d;
            ensure!(
                size.width <= limit && size.height <= limit,
                "window size exceeds GPU texture dimensions"
            );
            self.config.width = size.width;
            self.config.height = size.height;
            self.surface.configure(&self.gpu.device, &self.config);
        }
        Ok(())
    }
    fn render_scene(
        &mut self,
        instance: &wgpu::Instance,
        draws: &[Draw],
        live: &[storm_core::EntityId],
        seconds: f32,
        pictures: Option<Rc<LoadingPictures>>,
        fades: &[FadeDraw],
    ) -> Result<bool> {
        self.present(instance, |surface, encoder, view| {
            surface.scenes.prepare(&surface.gpu, draws, live)?;
            surface.scenes.encode(
                &surface.gpu,
                encoder,
                view,
                surface.config.width,
                surface.config.height,
                seconds,
            )?;
            if pictures.is_some() {
                surface.encode_loading(encoder, view, pictures, None)?;
            } else {
                surface.loading = None;
            }
            surface.fade.encode(
                &surface.gpu,
                encoder,
                view,
                [surface.config.width, surface.config.height],
                fades,
            )
        })
    }
    fn render_progress(
        &mut self,
        instance: &wgpu::Instance,
        progress: &LoadingProgress,
    ) -> Result<bool> {
        self.present(instance, |surface, encoder, view| {
            surface.encode_loading(
                encoder,
                view,
                progress.pictures.clone(),
                Some(progress.phase),
            )
        })
    }
    fn encode_loading(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        pictures: Option<Rc<LoadingPictures>>,
        phase: Option<f32>,
    ) -> Result<()> {
        let needs_upload =
            self.loading
                .as_ref()
                .is_none_or(|(cached, _)| match (cached, &pictures) {
                    (Some(old), Some(new)) => !Rc::ptr_eq(old, new),
                    (None, None) => false,
                    _ => true,
                });
        if needs_upload {
            let screen = LoadingScreen::new(
                &self.gpu,
                self.config.format,
                pictures.as_deref().unwrap_or(&LoadingPictures::default()),
            )?;
            self.loading = Some((pictures, screen));
        }
        let (_, screen) = self.loading.as_ref().expect("loading screen prepared");
        let size = [self.config.width, self.config.height];
        if let Some(phase) = phase {
            screen.encode_progress(&self.gpu, encoder, view, size, phase);
        } else {
            screen.encode(&self.gpu, encoder, view, size);
        }
        Ok(())
    }
    fn present(
        &mut self,
        instance: &wgpu::Instance,
        encode: impl FnOnce(&mut Self, &mut wgpu::CommandEncoder, &wgpu::TextureView) -> Result<()>,
    ) -> Result<bool> {
        if !self.configured || self.occluded {
            return Ok(false);
        }
        let (frame, reconfigure) = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(frame) => (frame, false),
            wgpu::CurrentSurfaceTexture::Suboptimal(frame) => (frame, true),
            wgpu::CurrentSurfaceTexture::Timeout => return Ok(false),
            wgpu::CurrentSurfaceTexture::Occluded => {
                self.occluded = true;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                self.resize(self.window.inner_size())?;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surface = instance
                    .create_surface(self.window.clone())
                    .context("recreate engine surface")?;
                self.resize(self.window.inner_size())?;
                return Ok(false);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                bail!("engine surface failed graphics validation")
            }
        };
        let mut encoder = self.gpu.device.create_command_encoder(&Default::default());
        let view = frame.texture.create_view(&Default::default());
        encode(self, &mut encoder, &view)?;
        self.gpu.queue.submit([encoder.finish()]);
        self.window.pre_present_notify();
        self.gpu.queue.present(frame);
        if reconfigure {
            self.resize(self.window.inner_size())?;
        }
        Ok(true)
    }
}
struct Application<'a> {
    engine: &'a mut Engine,
    models: Models,
    instance: wgpu::Instance,
    options: Options,
    surface: Option<Rc<RefCell<Surface>>>,
    timer: Timer,
    last_frame: Instant,
    next_frame: Instant,
    elapsed_ms: u64,
    focused: bool,
    closing: bool,
    presented: u64,
    failure: Option<anyhow::Error>,
}
impl Application<'_> {
    fn fail(&mut self, event_loop: &ActiveEventLoop, error: anyhow::Error) {
        self.failure = Some(error);
        event_loop.exit();
    }
    fn close(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if !self.closing {
            self.closing = true;
            if self.engine.phase() == Phase::Running {
                self.engine
                    .emit_event("DestroyWindow", &mut Message::new("", vec![])?)?;
            }
            self.engine.request_exit();
            event_loop.exit();
        }
        Ok(())
    }
    fn step(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self.closing || (!self.focused && !self.options.settings.run_in_background) {
            return Ok(());
        }
        let now = Instant::now();
        if now < self.next_frame {
            return Ok(());
        }
        let delta = self.timer.advance(now.duration_since(self.last_frame))?;
        self.last_frame = now;
        self.next_frame =
            now + Duration::from_millis(u64::from(self.options.settings.minimum_frame_ms()));
        let frame = self.engine.frame_with_timer(&self.timer)?;
        if !frame.continued {
            event_loop.exit();
            return Ok(());
        }
        self.elapsed_ms = self
            .elapsed_ms
            .checked_add(u64::from(delta))
            .context("window display clock overflow")?;
        if frame.skipped {
            return Ok(());
        }
        let draws = self.models.take_draws()?;
        let live = self.models.live_ids()?;
        if let Some(surface) = &self.surface {
            let presented = {
                let mut surface = surface.borrow_mut();
                surface.scenes.prune(&live);
                let presented = if let Some(progress) = self.engine.host.loading_progress() {
                    surface.render_progress(&self.instance, &progress)?
                } else {
                    let fades = self.engine.host.fade_draws();
                    surface.render_scene(
                        &self.instance,
                        &draws,
                        &live,
                        self.elapsed_ms as f32 * 0.001,
                        self.engine.host.loading_draw(),
                        fades.as_deref().unwrap_or_default(),
                    )?
                };
                surface.scenes.prune(&live);
                presented
            };
            if presented {
                self.presented += 1;
                if self
                    .options
                    .frames
                    .is_some_and(|limit| self.presented >= limit)
                {
                    eprintln!("Presented {} engine frames", self.presented);
                    self.close(event_loop)?;
                }
            }
        }
        Ok(())
    }
}
impl ApplicationHandler for Application<'_> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.surface.is_none() {
            match pollster::block_on(Surface::new(
                &self.instance,
                event_loop,
                &self.options.settings,
            )) {
                Ok(surface) => {
                    let size = surface.window.inner_size();
                    self.engine
                        .host
                        .set_interface_window(size.width, size.height);
                    surface.window.request_redraw();
                    let surface = Rc::new(RefCell::new(surface));
                    let progress_surface = Rc::downgrade(&surface);
                    let instance = self.instance.clone();
                    self.engine.host.set_progress_presenter(move |progress| {
                        if let Some(surface) = progress_surface.upgrade() {
                            surface.borrow_mut().render_progress(&instance, progress)?;
                        }
                        Ok(())
                    });
                    self.surface = Some(surface);
                }
                Err(error) => self.fail(event_loop, error),
            }
        }
    }
    fn suspended(&mut self, _: &ActiveEventLoop) {
        // Keep the Engine/CPU world; surface/device resources rebuild on resume.
        self.engine.control_input(ControlInput::FocusLost);
        self.surface = None;
    }
    fn device_event(&mut self, _: &ActiveEventLoop, _: DeviceId, event: DeviceEvent) {
        if self.focused
            && self.surface.is_some()
            && let DeviceEvent::MouseMotion { delta: (x, y) } = event
        {
            self.engine
                .control_input(ControlInput::MouseMotion { x, y });
        }
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        let Some(surface) = self.surface.clone() else {
            return;
        };
        if surface.borrow().window.id() != id {
            return;
        }
        let result = match event {
            WindowEvent::CloseRequested => self.close(event_loop),
            WindowEvent::Resized(size) => {
                self.engine
                    .host
                    .set_interface_window(size.width, size.height);
                surface.borrow_mut().resize(size)
            }
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if focused {
                    let mut surface = surface.borrow_mut();
                    surface.occluded = false;
                    surface.window.request_redraw();
                } else {
                    self.engine.control_input(ControlInput::FocusLost);
                }
                Ok(())
            }
            WindowEvent::KeyboardInput { event, .. } if self.focused => {
                if let Some(input) = input::keyboard(event) {
                    self.engine.control_input(input);
                }
                Ok(())
            }
            WindowEvent::MouseInput { state, button, .. } if self.focused => {
                if let Some(input) = input::button(button, state.is_pressed()) {
                    self.engine.control_input(input);
                }
                Ok(())
            }
            WindowEvent::MouseWheel { delta, .. } if self.focused => {
                self.engine.mouse_wheel(input::wheel(delta))
            }
            WindowEvent::Occluded(occluded) => {
                let mut surface = surface.borrow_mut();
                surface.occluded = occluded;
                if !occluded {
                    surface.window.request_redraw();
                }
                Ok(())
            }
            WindowEvent::RedrawRequested => self.step(event_loop),
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.closing {
            return;
        }
        if self.surface.is_none() {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        if !self.focused && !self.options.settings.run_in_background {
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(50),
            ));
            return;
        }
        let now = Instant::now();
        if now < self.next_frame {
            event_loop.set_control_flow(ControlFlow::WaitUntil(self.next_frame));
            return;
        }
        let hidden = self.surface.as_ref().is_some_and(|s| {
            let surface = s.borrow();
            !surface.configured || surface.occluded
        });
        if hidden {
            if let Err(error) = self.step(event_loop) {
                self.fail(event_loop, error);
                return;
            }
            // Avoid a busy wait when presentation supplies no pacing.
            event_loop.set_control_flow(ControlFlow::WaitUntil(
                Instant::now() + Duration::from_millis(16),
            ));
        } else {
            self.surface
                .as_ref()
                .unwrap()
                .borrow()
                .window
                .request_redraw();
            event_loop.set_control_flow(ControlFlow::Wait);
        }
    }
}
pub fn run(engine: &mut Engine, models: Models, options: Options) -> Result<()> {
    ensure!(
        (1..=16384).contains(&options.settings.width)
            && (1..=16384).contains(&options.settings.height),
        "window dimensions must be 1..16384"
    );
    ensure!(
        options.frames.is_none_or(|limit| limit > 0),
        "window frame limit must be positive"
    );
    let instance = storm_render::create_instance(options.backends)?;
    let event_loop = EventLoop::new().context("create engine event loop")?;
    let mut timer = Timer::default();
    if let Some(delta) = options.fixed_delta {
        timer.set_delta(delta);
    }
    let now = Instant::now();
    let mut app = Application {
        engine,
        models,
        instance,
        options,
        surface: None,
        timer,
        last_frame: now,
        next_frame: now,
        elapsed_ms: 0,
        focused: true,
        closing: false,
        presented: 0,
        failure: None,
    };
    event_loop.run_app(&mut app).context("run engine window")?;
    app.failure.map_or(Ok(()), Err)
}
