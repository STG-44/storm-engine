use anyhow::{Context, Result, ensure};
use clap::Parser;
use std::{fs::File, io::Read, path::PathBuf};
use storm_script::{Limits, Program, preview::AttributeHost};

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
}

fn main() -> Result<()> {
    let options = Options::parse();
    let mut source = String::new();
    File::open(&options.source)
        .context("open script source")?
        .take(262145)
        .read_to_string(&mut source)?;
    ensure!(source.len() <= 262144, "script source limit");
    let program = Program::compile(&source).context("compile source-language subset")?;
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
    let report = serde_json::to_string_pretty(&host.report(&result))? + "\n";
    if let Some(path) = options.report {
        std::fs::write(path, &report)?;
    }
    print!("{report}");
    Ok(())
}
