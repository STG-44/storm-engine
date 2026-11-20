//! Original renderer texture path levels and logical TX requests.
use anyhow::{Result, ensure};

#[derive(Clone, Default)]
pub struct TexturePaths {
    levels: [String; 4],
}

pub struct TextureCandidate {
    /// Renderer identity before adding the implicit extension or resource root.
    pub key: String,
    /// File name relative to RESOURCE/Textures.
    pub file: String,
}

impl TexturePaths {
    pub fn set(&mut self, level: i32, prefix: &str) -> Result<bool> {
        let Some(slot) = usize::try_from(level)
            .ok()
            .and_then(|i| self.levels.get_mut(i))
        else {
            return Ok(false);
        };
        let prefix = normalize(prefix)?;
        *slot = prefix;
        Ok(true)
    }

    pub fn candidates(&self, name: &str) -> Result<Vec<TextureCandidate>> {
        let name = normalize(name)?;
        ensure!(
            !name.is_empty() && !name.ends_with('/') && name.rsplit('/').next() != Some("."),
            "native texture must name a file"
        );
        let split = name.rfind('/').map_or(0, |i| i + 1);
        let (directory, basename) = name.split_at(split);
        self.levels
            .iter()
            .rev()
            .map(String::as_str)
            .filter(|prefix| !prefix.is_empty())
            .chain(std::iter::once(""))
            .map(|prefix| {
                let key = normalize(&format!("{directory}{prefix}{basename}"))?;
                let mut file = key
                    .split('/')
                    .filter(|part| !part.is_empty() && *part != ".")
                    .collect::<Vec<_>>()
                    .join("/");
                if let Some(relative) = file.strip_prefix("resource/textures/") {
                    file = relative.to_owned();
                }
                if !file.ends_with(".tx") {
                    file.push_str(".tx");
                }
                Ok(TextureCandidate { key, file })
            })
            .collect()
    }
}

fn normalize(path: &str) -> Result<String> {
    ensure!(
        path.len() < 256 && !path.bytes().any(|b| b.is_ascii_control() || b == b':'),
        "invalid native texture path"
    );
    let normalized = path.replace('\\', "/").to_ascii_lowercase();
    ensure!(
        !normalized.starts_with('/') && !normalized.split('/').any(|part| part == ".."),
        "native texture path escapes its root"
    );
    Ok(normalized)
}
