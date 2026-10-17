use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Read,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde_json::{Value, json};
use storm_formats::{GmHeader, TxHeader};

#[derive(Parser)]
#[command(about = "Inspect original Storm GM/TX metadata without running the game")]
struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Inspect one .gm or .tx file's header and minimum table length.
    Inspect { file: PathBuf },
    /// Check GM/TX headers recursively, skipping symbolic links.
    Scan { directory: PathBuf },
}

enum Asset {
    Model(GmHeader),
    Texture(TxHeader),
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
}

fn read_asset(path: &Path) -> Result<Asset> {
    let mut file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let length = file.metadata()?.len();
    let mut header = [0; GmHeader::SIZE];
    let size = length.min(header.len() as u64) as usize;
    file.read_exact(&mut header[..size])?;
    Ok(match extension(path).as_str() {
        "gm" => Asset::Model(GmHeader::read(&header[..size], length)?),
        "tx" => Asset::Texture(TxHeader::read(&header[..size], length)?),
        _ => bail!("expected a .gm or .tx file"),
    })
}

fn inspect(path: &Path) -> Result<Value> {
    let metadata = match read_asset(path)? {
        Asset::Model(h) => json!({
            "kind": "gm", "version": "1.05", "flags": h.flags,
            "name_bytes": h.name_bytes, "names": h.names,
            "textures": h.textures, "materials": h.materials,
            "lights": h.lights, "labels": h.labels, "objects": h.objects,
            "triangles": h.triangles, "vertex_buffers": h.vertex_buffers,
            "bounds_size": h.bounds_size, "bounds_center": h.bounds_center,
            "radius": h.radius, "minimum_file_bytes": h.minimum_file_len(),
        }),
        Asset::Texture(h) => json!({
            "kind": "tx", "flags": h.flags, "width": h.width, "height": h.height,
            "mips": h.mips, "format": h.format_name(), "format_code": h.format,
            "mip0_bytes": h.mip0_bytes, "minimum_file_bytes": h.minimum_file_len(),
            "cubemap": h.flags & TxHeader::CUBEMAP != 0,
            "palette": h.flags & TxHeader::PALETTE != 0,
        }),
    };
    Ok(json!({"file": path.display().to_string(), "metadata": metadata}))
}

fn asset_paths(root: &Path) -> Result<Vec<PathBuf>> {
    let mut pending = vec![root.to_owned()];
    let mut files = Vec::new();
    while let Some(dir) = pending.pop() {
        for entry in fs::read_dir(&dir).with_context(|| format!("list {}", dir.display()))? {
            let entry = entry?;
            let kind = entry.file_type()?;
            let path = entry.path();
            if kind.is_dir() {
                pending.push(path);
            } else if kind.is_file() && matches!(extension(&path).as_str(), "gm" | "tx") {
                files.push(path);
            }
        }
    }
    files.sort();
    Ok(files)
}

fn scan(root: &Path) -> Result<(Value, bool)> {
    let mut models = 0_u64;
    let mut textures = 0_u64;
    let mut triangles = 0_u64;
    let mut formats = BTreeMap::<String, u64>::new();
    let mut failures = Vec::new();
    for path in asset_paths(root)? {
        match read_asset(&path) {
            Ok(Asset::Model(h)) => {
                models += 1;
                triangles += u64::from(h.triangles);
            }
            Ok(Asset::Texture(h)) => {
                textures += 1;
                let name = if h.format_name() == "unknown" {
                    format!("unknown:0x{:08x}", h.format)
                } else {
                    h.format_name().to_owned()
                };
                *formats.entry(name).or_default() += 1;
            }
            Err(error) => failures.push(json!({
                "file": path.strip_prefix(root).unwrap_or(&path).display().to_string(),
                "error": error.to_string(),
            })),
        }
    }
    let passed = failures.is_empty();
    Ok((
        json!({
            "directory": root.display().to_string(),
            "validation": "headers and minimum table lengths only",
            "models": models, "textures": textures, "triangles": triangles,
            "texture_formats": formats, "failures": failures,
        }),
        passed,
    ))
}

fn main() -> Result<()> {
    let (result, passed) = match Args::parse().command {
        Command::Inspect { file } => (inspect(&file)?, true),
        Command::Scan { directory } => scan(&directory)?,
    };
    println!("{}", serde_json::to_string_pretty(&result)?);
    if !passed {
        bail!("one or more asset headers failed validation");
    }
    Ok(())
}
