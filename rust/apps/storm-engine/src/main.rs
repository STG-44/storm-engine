use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use storm_runtime::model::{ModelScenes, Models};
use storm_runtime::{GameDirectory, NativeHost, ScriptEncoding, Startup};
mod window;

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
#[command(about = "Storm runtime: configured source, native window and static model rendering")]
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
    /// Render the final script-created static models to PNG. Full game rendering is pending.
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
        let mut elapsed_ms = 0u64;
        for (index, delta_ms) in arguments.frame_ms.into_iter().enumerate() {
            let frame = engine
                .frame(delta_ms)
                .with_context(|| format!("engine frame {index}"))?;
            if !frame.continued {
                break;
            }
            elapsed_ms = elapsed_ms
                .checked_add(u64::from(delta_ms))
                .context("engine display clock overflow")?;
        }
        if let Some(path) = arguments.headless {
            let instance = storm_render::create_instance(arguments.backend.flags())?;
            let gpu = pollster::block_on(storm_render::Gpu::request(&instance, None))?;
            let mut scenes = ModelScenes::default();
            let draws = models.take_draws()?;
            let live = models.live_ids()?;
            scenes.prepare(&gpu, &draws, &live)?;
            scenes
                .offscreen(
                    &gpu,
                    arguments.width.unwrap_or(960),
                    arguments.height.unwrap_or(540),
                    elapsed_ms as f32 * 0.001,
                )?
                .save_png(&path)?;
            scenes.prune(&live);
            eprintln!(
                "Rendered {} script-created static models to {}",
                draws.len(),
                path.display()
            );
        }
        Ok(())
    })();
    let shutdown = engine.shutdown();
    frames.and(shutdown)
}
