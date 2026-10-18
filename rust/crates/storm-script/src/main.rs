use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{fs::File, io::Read, path::PathBuf};
use storm_script::{Limits, Runtime, preview::AttributeHost};

#[derive(Parser)]
#[command(about = "Execute a bounded Storm source-language subset without a GPU")]
struct Options {
    source: PathBuf,
    #[arg(long, default_value = "main")]
    entry: String,
    #[arg(long)]
    report: Option<PathBuf>,
    #[arg(long, default_value_t = 100000)]
    instructions: u64,
    /// Advance up to 128 scheduler frames with explicit millisecond deltas.
    #[arg(long, value_delimiter = ',')]
    frame_ms: Vec<u32>,
}

fn main() -> Result<()> {
    let options = Options::parse();
    let mut source = String::new();
    File::open(&options.source)
        .context("open script source")?
        .take(262145)
        .read_to_string(&mut source)?;
    ensure!(source.len() <= 262144, "script source limit");
    ensure!(options.frame_ms.len() <= 128, "scheduler frame count limit");
    let mut program = Runtime::compile(&source).context("compile source-language subset")?;
    let mut host = AttributeHost::new()?;
    let result = program.call(
        &options.entry,
        &[],
        &mut host,
        Limits {
            instructions: options.instructions,
            ..Limits::default()
        },
    )?;
    let mut frames = Vec::new();
    for delta in &options.frame_ms {
        let delivered = program.advance_frame(
            *delta,
            &mut host,
            Limits {
                instructions: options.instructions,
                ..Limits::default()
            },
        )?;
        frames.push(serde_json::json!({"delta_ms":delta,"delivered_events":delivered}));
    }
    let mut report = host.report(&result);
    if !frames.is_empty() || program.posted_events() != 0 || program.completed_handlers() != 0 {
        report["script_events"] = serde_json::json!({"frames":frames,
            "completed_handlers":program.completed_handlers(),"posted_events":program.posted_events(),
            "pending_events":program.pending_events(),"retained_post_events":program.retained_post_events()});
    }
    let report = serde_json::to_string_pretty(&report)? + "\n";
    if let Some(path) = options.report {
        std::fs::write(path, &report)?;
    }
    print!("{report}");
    Ok(())
}
