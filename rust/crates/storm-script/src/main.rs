use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::path::{Path, PathBuf};
use storm_script::{DirectorySource, Limits, Runtime, preview::AttributeHost};

#[derive(Parser)]
#[command(about = "Execute a bounded Storm source-language subset without a GPU")]
struct Options {
    source: PathBuf,
    /// With this option, source is a virtual path relative to this script directory.
    #[arg(long)]
    script_root: Option<PathBuf>,
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
    ensure!(options.frame_ms.len() <= 128, "scheduler frame count limit");
    let source = PathBuf::from(
        options
            .source
            .to_str()
            .context("script path is not UTF-8")?
            .replace('\\', std::path::MAIN_SEPARATOR_STR),
    );
    let (root, entry) = if let Some(root) = &options.script_root {
        (root.as_path(), source.as_path())
    } else {
        (
            source
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
            Path::new(source.file_name().context("missing script file name")?),
        )
    };
    let mut loader = DirectorySource::new(root)?;
    let mut program = Runtime::compile_with_loader(
        entry.to_str().context("script path is not UTF-8")?,
        &mut loader,
    )
    .context("compile source-language subset")?;
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
    if program.sources().len() > 1 {
        report["script_sources"] = serde_json::json!(program.sources());
    }
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
