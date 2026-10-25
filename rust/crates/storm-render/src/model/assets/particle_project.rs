use super::*;
use crate::model::ParticleProjectInfo;
use storm_formats::particle_project::{Project, project_key, texture_keys};

/// Missing files are catalog entries, while ambiguity/escapes remain errors.
fn profile_exists(root: &Path, key: &str) -> Result<bool> {
    let mut path = root.to_path_buf();
    for part in key.split('/') {
        if !fs::read_dir(&path)?
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .any(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(part))
        {
            return Ok(false);
        }
        path = find_child(&path, part)?;
    }
    ensure!(path.is_file(), "particle profile is not a file");
    Ok(true)
}

#[cfg(test)]
mod tests;
impl Builder {
    pub(super) fn select_particle_project(
        &mut self,
        directory: &Path,
        selection: Option<&Path>,
    ) -> Result<(usize, Option<BTreeSet<String>>)> {
        let Some(selection) = selection else {
            return Ok((self.texture("particles_list.tga")?, None));
        };
        let (source, texture, declared) = if selection == Path::new("default") {
            let mut names = BTreeSet::new();
            for entry in fs::read_dir(directory)? {
                let entry = entry?;
                if entry.file_type()?.is_file()
                    && entry
                        .path()
                        .extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("xps"))
                {
                    let name = entry
                        .file_name()
                        .into_string()
                        .map_err(|_| anyhow::anyhow!("particle filename is not Unicode"))?;
                    let key = storm_formats::particles::profile_key(&name)?;
                    ensure!(names.insert(key), "case-colliding particle profile names");
                    ensure!(
                        names.len() <= 9999,
                        "particle project exceeds profile budget"
                    );
                }
            }
            (
                "default".into(),
                "particles_list.tga".into(),
                names.into_iter().collect::<Vec<_>>(),
            )
        } else {
            let path = if selection.is_absolute() {
                selection.canonicalize()?
            } else {
                let key = project_key(
                    selection
                        .to_str()
                        .context("particle project name is not Unicode")?,
                )?;
                let mut path = directory.to_path_buf();
                for part in key.split('/') {
                    path = find_child(&path, part)?;
                }
                path
            };
            ensure!(
                fs::metadata(&path)?.len() <= 1_048_576,
                "particle project exceeds 1 MiB"
            );
            let project = Project::read(&read_asset_bytes(&path)?)?;
            (
                selection.display().to_string(),
                project.texture.display(),
                project.systems.iter().map(|s| s.display()).collect(),
            )
        };
        let (color_key, normal_key) = texture_keys(&texture)?;
        let files = self
            .files
            .as_ref()
            .context("particle project requires original textures")?;
        let color = files.paths.get(&color_key).cloned().with_context(|| {
            format!(
                "project atlas not found: RESOURCE/Textures/{color_key} (requested {texture:?})"
            )
        })?;
        let normal = files.paths.get(&normal_key).cloned();
        let atlas = self.texture_path(color)?;
        let normal_loaded = if let Some(path) = normal {
            self.texture_path(path)?;
            true
        } else {
            false
        };
        let mut available = BTreeSet::new();
        let mut missing = BTreeSet::new();
        for name in &declared {
            let key = storm_formats::particles::profile_key(name)?;
            if profile_exists(directory, &key)? {
                available.insert(key);
            } else {
                missing.insert(key);
            }
        }
        self.asset.ship.particle_project = Some(ParticleProjectInfo {
            source,
            texture,
            color_key,
            normal_key,
            normal_loaded,
            declared,
            available: available.iter().cloned().collect(),
            missing: missing.into_iter().collect(),
        });
        Ok((atlas, Some(available)))
    }
}
