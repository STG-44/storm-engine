use anyhow::{Context, Result};
use clap::{Parser, ValueEnum};
use std::path::PathBuf;
use storm_runtime::{GameDirectory, NativeHost, ScriptEncoding, Startup};

#[derive(Clone, Copy, ValueEnum)]
enum Encoding {
    Utf8,
    Windows1251,
}
#[derive(Parser)]
#[command(about = "Storm engine runtime foundation: configured source startup and headless frames")]
struct Arguments {
    /// Installed game directory containing engine.ini and original PROGRAM.
    #[arg(long)]
    game_dir: PathBuf,
    /// Select the original source text encoding explicitly.
    #[arg(long, value_enum)]
    script_encoding: Encoding,
    /// Headless frame deltas in milliseconds. Native window/services are pending.
    #[arg(long, value_delimiter = ',', default_value = "16")]
    frame_ms: Vec<u32>,
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
    let mut engine = startup.engine(&game, encoding, host)?;
    eprintln!(
        "Loaded {} Storm source files; complete game services remain pending",
        engine.sources().len()
    );
    let frames = (|| -> Result<()> {
        for (index, delta_ms) in arguments.frame_ms.into_iter().enumerate() {
            let frame = engine
                .frame(delta_ms)
                .with_context(|| format!("engine frame {index}"))?;
            if !frame.continued {
                break;
            }
        }
        Ok(())
    })();
    let shutdown = engine.shutdown();
    frames.and(shutdown)
}
