use crate::{GameDirectory, ScriptEncoding};
use anyhow::Result;
use storm_core::{AttributeId, AttributeStore};

pub(super) const MSG_INTERFACE_LOADOPTIONS: i32 = 45053;
const OPTIONS_FILE_BYTES: usize = 256 * 1024;

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
