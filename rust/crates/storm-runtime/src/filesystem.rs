//! Explicit, case-insensitive access to one installed game directory.
use anyhow::{Context, Result, ensure};
use std::{
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone)]
pub struct GameDirectory {
    root: PathBuf,
}
impl GameDirectory {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        let root = root
            .as_ref()
            .canonicalize()
            .context("open game directory")?;
        ensure!(root.is_dir(), "game directory is not a directory");
        Ok(Self { root })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn resolve(&self, name: &str) -> Result<PathBuf> {
        ensure!(
            !name.is_empty()
                && name.len() <= 1024
                && name.is_ascii()
                && !name.bytes().any(|b| b.is_ascii_control() || b == b':'),
            "invalid game-relative path: {name:?}"
        );
        let normalized = name.replace('\\', "/");
        ensure!(!normalized.starts_with('/'), "absolute game path: {name}");
        let mut current = self.root.clone();
        for part in normalized.split('/') {
            ensure!(
                !part.is_empty() && part != "..",
                "invalid game path component: {name}"
            );
            if part == "." {
                continue;
            }
            let mut found = None;
            for (count, entry) in std::fs::read_dir(&current)
                .with_context(|| format!("read game path {name}"))?
                .enumerate()
            {
                ensure!(count < 65536, "game directory entry budget: {name}");
                let entry = entry?;
                if entry
                    .file_name()
                    .to_str()
                    .is_some_and(|s| s.eq_ignore_ascii_case(part))
                {
                    ensure!(found.is_none(), "ambiguous game path: {name}");
                    found = Some(entry.path());
                }
            }
            current = found
                .with_context(|| format!("game path not found: {name}"))?
                .canonicalize()
                .with_context(|| format!("resolve game path {name}"))?;
            ensure!(
                current.starts_with(&self.root),
                "game path escapes root: {name}"
            );
        }
        Ok(current)
    }
    pub fn directory(&self, name: &str) -> Result<Self> {
        let path = self.resolve(name)?;
        ensure!(path.is_dir(), "game path is not a directory: {name}");
        Ok(Self { root: path })
    }
    pub fn read(&self, name: &str, limit: usize) -> Result<Vec<u8>> {
        let path = self.resolve(name)?;
        let file = File::open(&path).with_context(|| format!("open game file {name}"))?;
        let metadata = file.metadata()?;
        ensure!(
            metadata.is_file() && metadata.len() <= limit as u64,
            "game file is not regular or exceeds {limit} bytes: {name}"
        );
        let mut bytes = Vec::new();
        file.take((limit as u64).saturating_add(1))
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= limit, "game file byte budget: {name}");
        Ok(bytes)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ScriptEncoding {
    Utf8,
    Windows1251,
}
impl ScriptEncoding {
    pub fn decode(self, bytes: &[u8]) -> Result<String> {
        // UTF-8 BOM is meaningful independently of the configured legacy page.
        let text = if let Some(text) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
            std::str::from_utf8(text)
                .context("UTF-8 BOM script")?
                .to_owned()
        } else {
            match self {
                Self::Utf8 => std::str::from_utf8(bytes)
                    .context("UTF-8 script; select its original code page")?
                    .to_owned(),
                Self::Windows1251 => {
                    let (text, errors) =
                        encoding_rs::WINDOWS_1251.decode_without_bom_handling(bytes);
                    ensure!(!errors, "invalid Windows-1251 script text");
                    text.into_owned()
                }
            }
        };
        ensure!(!text.contains('\0'), "NUL in script text");
        Ok(text)
    }
}

pub struct GameSource {
    pub directory: GameDirectory,
    pub encoding: ScriptEncoding,
}
impl storm_script::SourceLoader for GameSource {
    fn load(&mut self, path: &str) -> Result<String> {
        self.encoding
            .decode(&self.directory.read(path, 262144)?)
            .with_context(|| format!("decode script {path}"))
    }
}
