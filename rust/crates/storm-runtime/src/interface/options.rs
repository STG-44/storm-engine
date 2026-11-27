use crate::{GameDirectory, ScriptEncoding};
use anyhow::{Context, Result, ensure};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};
use storm_core::{AttributeId, AttributeStore};

pub(super) const MSG_INTERFACE_LOADOPTIONS: i32 = 45053;
pub(super) const MSG_INTERFACE_SAVEOPTIONS: i32 = 45052;
const OPTIONS_FILE_BYTES: usize = 256 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

/// Returns true for a present, zero-byte file, which fires evntOptionsBreak.
pub(super) fn load(
    game: &GameDirectory,
    path: &str,
    attributes: &mut AttributeStore,
    root: AttributeId,
) -> Result<bool> {
    if path.is_empty() || root == AttributeId::INVALID {
        return Ok(false);
    }
    let Some(bytes) = game.read_optional(path, OPTIONS_FILE_BYTES)? else {
        return Ok(false);
    };
    if bytes.is_empty() {
        return Ok(true);
    }
    let encoding = if std::str::from_utf8(&bytes).is_ok() {
        ScriptEncoding::Utf8
    } else {
        ScriptEncoding::Windows1251
    };
    let text = encoding.decode(&bytes)?.replace('\r', "");
    let mut rest = text.as_str();
    // The original parser skips leading spaces, tabs and newlines, then
    // searches for '=' and keeps the value up to the next newline.
    while let Some((name, value)) = rest.trim_start_matches([' ', '\t', '\n']).split_once('=') {
        let (value, next) = value.split_once('\n').unwrap_or((value, ""));
        let node = attributes.create_path(root, name)?;
        attributes.set_value(node, Some(value))?;
        rest = next;
    }
    Ok(false)
}

pub(super) fn save(
    game: &GameDirectory,
    name: &str,
    attributes: &AttributeStore,
    root: AttributeId,
) -> Result<()> {
    if name.is_empty() || root == AttributeId::INVALID {
        return Ok(());
    }
    let mut bytes = String::new();
    serialize(attributes, root, &mut String::new(), &mut bytes)?;
    let normalized = name.replace('\\', "/");
    let path = match game.resolve_optional(name)? {
        Some(path) => path,
        None => {
            let (parent, leaf) = normalized
                .rsplit_once('/')
                .unwrap_or(("", normalized.as_str()));
            // Resolve the parent separately. The target may be the first save.
            ensure!(
                !leaf.is_empty() && leaf != "." && leaf != "..",
                "invalid options filename"
            );
            let parent = if parent.is_empty() {
                game.root().to_owned()
            } else {
                game.resolve(parent)?
            };
            parent.join(leaf)
        }
    };
    let parent = path.parent().context("resolve options folder")?;
    ensure!(
        parent.starts_with(game.root()) && (!path.exists() || path.is_file()),
        "invalid options destination"
    );
    let temporary = parent.join(format!(
        ".storm-options-{}-{}.tmp",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = file
        .write_all(bytes.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    let result = result.and_then(|()| fs::rename(&temporary, &path));
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.with_context(|| format!("save options {name}"))
}

fn serialize(
    attributes: &AttributeStore,
    node: AttributeId,
    prefix: &mut String,
    output: &mut String,
) -> Result<()> {
    let entry = attributes.get(node).context("missing options attribute")?;
    for &child in entry.children() {
        let name = attributes
            .name(child)
            .context("missing options attribute name")?;
        let length = prefix.len();
        if length > 0 {
            prefix.push('.');
        }
        prefix.push_str(name);
        if let Some(value) = attributes
            .value(child, None)
            .filter(|value| !value.is_empty())
        {
            ensure!(
                output.len() + prefix.len() + value.len() + 2 <= OPTIONS_FILE_BYTES,
                "options file byte budget"
            );
            if !output.is_empty() {
                output.push('\n');
            }
            output.push_str(prefix);
            output.push('=');
            output.push_str(value);
        }
        serialize(attributes, child, prefix, output)?;
        prefix.truncate(length);
    }
    Ok(())
}
