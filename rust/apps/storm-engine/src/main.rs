use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use storm_render::Gpu;
use storm_runtime::model::Models;
use storm_runtime::{GameDirectory, NativeHost, ScriptEncoding, Startup};
mod input;
mod scene;
mod window;
use scene::{SceneFrame, SceneRenderer};

fn headless_renderer(backends: wgpu::Backends) -> Result<(Gpu, SceneRenderer)> {
    let instance = storm_render::create_instance(backends)?;
    let gpu = pollster::block_on(Gpu::request(&instance, None))?;
    let scene = SceneRenderer::new(&gpu, wgpu::TextureFormat::Rgba8UnormSrgb);
    Ok((gpu, scene))
}

#[derive(Clone, Copy, ValueEnum)]
enum Encoding {
    Utf8,
    Windows1251,
}
#[derive(Clone, Copy, ValueEnum)]
enum Backend {
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
#[command(about = "Storm runtime: configured source, native window and model rendering")]
struct Arguments {
    /// Installed game directory containing engine.ini and original PROGRAM.
    #[arg(long)]
    game_dir: PathBuf,
    /// Select the original source text encoding explicitly.
    #[arg(long, value_enum)]
    script_encoding: Encoding,
    /// Explicit headless frame deltas in milliseconds; window mode uses TIMER.
    #[arg(long, value_delimiter = ',', default_value = "16")]
    frame_ms: Vec<u32>,
    /// Render the final scene or loading screen to PNG.
    #[arg(long, conflicts_with = "window")]
    headless: Option<PathBuf>,
    /// Run the same configured engine and scene in a native desktop window.
    #[arg(long, conflicts_with = "frame_ms")]
    window: bool,
    /// Stop after this many presented window frames (smoke check).
    #[arg(long, requires = "window")]
    window_frames: Option<u64>,
    /// Set native TIMER fixed delta in window mode; <=0 disables it.
    #[arg(long, requires = "window", allow_hyphen_values = true)]
    window_fixed_delta: Option<i32>,
    #[arg(long, value_enum, default_value = "auto")]
    backend: Backend,
    #[arg(long)]
    width: Option<u32>,
    #[arg(long)]
    height: Option<u32>,
}
fn main() -> Result<()> {
    let arguments = Arguments::parse();
    let game = GameDirectory::open(arguments.game_dir)?;
    let startup = Startup::load(&game)?;
    let encoding = match arguments.script_encoding {
        Encoding::Utf8 => ScriptEncoding::Utf8,
        Encoding::Windows1251 => ScriptEncoding::Windows1251,
    };
    let mut host = NativeHost::new()?;
    host.set_trace_sink(|text| {
        println!("{text}");
        Ok(())
    });
    let models = Models::register(&mut host, game.clone())?;
    let mut engine = startup.engine(&game, encoding, host)?;
    engine.register_service(models.clone())?;
    eprintln!(
        "Loaded {} Storm source files; complete game services remain pending",
        engine.sources().len()
    );
    let frames = (|| -> Result<()> {
        if arguments.window {
            let mut settings = storm_runtime::WindowSettings::load(&game)?;
            if let Some(width) = arguments.width {
                settings.width = width;
            }
            if let Some(height) = arguments.height {
                settings.height = height;
            }
            return window::run(
                &mut engine,
                models.clone(),
                window::Options {
                    settings,
                    frames: arguments.window_frames,
                    fixed_delta: arguments.window_fixed_delta,
                    backends: arguments.backend.flags(),
                },
            );
        }
        let size = [
            arguments.width.unwrap_or(960),
            arguments.height.unwrap_or(540),
        ];
        if arguments.headless.is_some() {
            engine.host.set_interface_window(size[0], size[1]);
        }
        let mut elapsed_ms = 0u64;
        let mut headless: Option<(Gpu, SceneRenderer)> = None;
        for (index, delta_ms) in arguments.frame_ms.into_iter().enumerate() {
            let next_elapsed = elapsed_ms
                .checked_add(u64::from(delta_ms))
                .context("engine display clock overflow")?;
            let frame = if arguments.headless.is_some() {
                engine.frame_with_screenshot(delta_ms, &mut |host| {
                    if headless.is_none() {
                        headless = Some(headless_renderer(arguments.backend.flags())?);
                    }
                    let (gpu, renderer) = headless.as_mut().expect("headless renderer initialized");
                    renderer
                        .screenshot(gpu, size, next_elapsed as f32 * 0.001, host, &models)
                        .map(Some)
                })
            } else {
                engine.frame(delta_ms)
            }
            .with_context(|| format!("engine frame {index}"))?;
            if !frame.continued {
                break;
            }
            elapsed_ms = next_elapsed;
            if arguments.headless.is_some() && !frame.skipped {
                let fades = engine.host.fade_draws();
                let fades = fades.as_deref().unwrap_or_default();
                if fades.iter().any(|draw| draw.frozen_frame.is_some()) {
                    if headless.is_none() {
                        headless = Some(headless_renderer(arguments.backend.flags())?);
                    }
                    let (gpu, renderer) = headless.as_mut().expect("headless renderer initialized");
                    if renderer.needs_capture(fades) {
                        let draws = models.draws()?;
                        let live = models.live_ids()?;
                        renderer.capture(
                            gpu,
                            size,
                            &SceneFrame {
                                draws: &draws,
                                live: &live,
                                seconds: elapsed_ms as f32 * 0.001,
                                pictures: engine.host.loading_draw(),
                                fades,
                                correction: engine.host.color_correction(),
                                sea: engine.host.sea_draw(),
                                camera: engine.host.authored_camera(),
                                interface: engine.host.interface_draws(),
                                coast_foam: engine.host.coast_foam_draws(),
                                rain: engine.host.rain_draws(),
                                sun: engine.host.sun_draws(),
                                astronomy: engine.host.astronomy_draws(),
                                sky: engine.host.sky_draw(),
                            },
                        )?;
                    }
                }
            }
        }
        if let Some(path) = arguments.headless {
            let (gpu, mut renderer) = match headless {
                Some(renderer) => renderer,
                None => headless_renderer(arguments.backend.flags())?,
            };
            let draws = models.take_draws()?;
            let live = models.live_ids()?;
            let progress = engine.host.loading_progress();
            let fades = engine.host.fade_draws();
            let scene = SceneFrame {
                draws: &draws,
                live: &live,
                seconds: elapsed_ms as f32 * 0.001,
                pictures: engine.host.loading_draw(),
                fades: fades.as_deref().unwrap_or_default(),
                correction: engine.host.color_correction(),
                sea: engine.host.sea_draw(),
                camera: engine.host.authored_camera(),
                interface: engine.host.interface_draws(),
                coast_foam: engine.host.coast_foam_draws(),
                rain: engine.host.rain_draws(),
                sun: engine.host.sun_draws(),
                astronomy: engine.host.astronomy_draws(),
                sky: engine.host.sky_draw(),
            };
            let frame = storm_render::render_offscreen_with(
                &gpu,
                size[0],
                size[1],
                scene.seconds,
                |encoder, view| {
                    if let Some(active) = &progress {
                        renderer.encode_progress(
                            &gpu,
                            encoder,
                            view,
                            size,
                            active,
                            scene.correction,
                        )
                    } else {
                        renderer.encode(&gpu, encoder, Some(view), size, &scene)
                    }
                },
            )?;
            frame.save_png(&path)?;
            eprintln!(
                "Rendered {} script-created model draws{}{} to {}",
                draws.len(),
                if progress.is_none() && scene.sea.is_some() {
                    " and the sea"
                } else {
                    ""
                },
                if progress.is_some() {
                    " and a loading indicator"
                } else if scene.pictures.is_some() {
                    " and loading pictures"
                } else {
                    ""
                },
                path.display()
            );
        }
        Ok(())
    })();
    let shutdown = engine.shutdown();
    frames.and(shutdown)
}
