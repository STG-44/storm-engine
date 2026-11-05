//! ParticleManager project selection and its bounded native INI subset.
use crate::{FormatError, particles::Text};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub fn project_key(name: &str) -> Result<String, FormatError> {
    crate::particles::resource_key(name, "prj")
}

/// DX9 texture references are rooted at RESOURCE/Textures and append .tx.
/// SetProjectTexture derives the normal request from the color reference's stem.
pub fn texture_keys(name: &str) -> Result<(String, String), FormatError> {
    let normalized = name.replace('\\', "/");
    let key = if normalized
        .to_ascii_lowercase()
        .starts_with("resource/textures/")
    {
        &normalized["resource/textures/".len()..]
    } else {
        &normalized
    };
    let checked = crate::particles::resource_key(key, "tx")?;
    let color = if key.to_ascii_lowercase().ends_with(".tx") {
        key.to_ascii_lowercase()
    } else {
        checked
    };
    let stem = std::path::Path::new(key)
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or(FormatError("invalid particle atlas name"))?;
    let normal = crate::particles::resource_key(&format!("{stem}nm"), "tx")?;
    Ok((color, normal))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Project {
    pub texture: Text,
    /// Original values, in System_0000..System_9998 order up to the first gap.
    pub systems: Vec<Text>,
}
type Keys = BTreeMap<Vec<u8>, Option<Vec<u8>>>;
fn trim(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|v| !matches!(v, b' ' | b'\t'))
        .unwrap_or(bytes.len());
    let end = bytes
        .iter()
        .rposition(|v| !matches!(v, b' ' | b'\t'))
        .map_or(start, |i| i + 1);
    &bytes[start..end]
}
impl Project {
    pub fn read(bytes: &[u8]) -> Result<Self, FormatError> {
        if bytes.len() > 1_048_576 {
            return Err(FormatError("particle project exceeds 1 MiB"));
        }
        let mut sections: Vec<(Vec<u8>, Keys)> = vec![(vec![], Keys::new())];
        for (number, line) in bytes.split(|v| matches!(v, b'\r' | b'\n' | 0)).enumerate() {
            if number >= 32768 {
                return Err(FormatError("particle project exceeds line budget"));
            }
            let start = line
                .iter()
                .position(|v| !matches!(v, b' ' | b'\t'))
                .unwrap_or(line.len());
            let line = &line[start..];
            if line.is_empty() || line[0] == b';' {
                continue;
            }
            if line[0] == b'[' {
                if let Some(end) = line.iter().position(|v| *v == b']') {
                    sections.push((line[1..end].to_ascii_lowercase(), Keys::new()));
                }
                continue;
            }
            let (key, value) = if let Some(equal) = line.iter().position(|v| *v == b'=') {
                let value = trim(&line[equal + 1..]);
                (
                    trim(&line[..equal]).to_ascii_lowercase(),
                    (!value.is_empty()).then(|| value.to_vec()),
                )
            } else {
                (line.to_ascii_lowercase(), None)
            };
            sections.last_mut().unwrap().1.entry(key).or_insert(value);
        }
        let get = |section: &[u8], key: &[u8]| -> Result<Option<Text>, FormatError> {
            let value = sections
                .iter()
                .find(|(s, _)| s == section)
                .and_then(|(_, keys)| keys.get(key))
                .and_then(Option::as_ref);
            if value.is_some_and(|v| v.len() >= 8192) {
                return Err(FormatError(
                    "particle project value exceeds native 8192-byte buffer",
                ));
            }
            Ok(value.map(|v| Text(v.clone())))
        };
        let texture = get(b"textures", b"maintexture")?.unwrap_or_else(|| Text(b"none".to_vec()));
        let mut systems = vec![];
        for i in 0..9999 {
            let Some(value) = get(b"manager", format!("system_{i:04}").as_bytes())? else {
                break;
            };
            systems.push(value);
        }
        Ok(Self { texture, systems })
    }
    pub fn report(&self) -> Value {
        json!({"kind":"particle_project","texture":self.texture.display(),"texture_bytes":self.texture.0,
            "systems":self.systems.iter().map(|s|s.display()).collect::<Vec<_>>(),
            "system_bytes":self.systems.iter().map(|s|&s.0).collect::<Vec<_>>()})
    }
}

#[cfg(test)]
mod tests;
