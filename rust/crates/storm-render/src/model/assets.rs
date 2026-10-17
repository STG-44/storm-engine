use super::ModelVertex;
use anyhow::{Context, Result, bail, ensure};
use glam::{Mat4, Vec3};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File},
    io::Read,
    path::{Component, Path, PathBuf},
};
use storm_formats::{
    TxHeader,
    geometry::Geometry,
    texture::{Mip, Texture},
};

pub struct Mesh {
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub texture: usize,
}

pub struct ModelAsset {
    pub meshes: Vec<Mesh>,
    pub textures: Vec<Texture>,
    pub center: Vec3,
    pub radius: f32,
    pub geometry_files: usize,
    pub sails: usize,
}

#[derive(Default)]
pub struct LoadOptions {
    pub game_dir: Option<PathBuf>,
    /// Relative to RESOURCE/Textures. Default ship preview selects Hull1.
    pub texture_dir: Option<PathBuf>,
    pub untextured: bool,
    pub single_mesh: bool,
}

pub fn read_asset_bytes(path: &Path) -> Result<Vec<u8>> {
    const LIMIT: u64 = 256 * 1024 * 1024;
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    ensure!(
        file.metadata()?.len() <= LIMIT,
        "asset exceeds the 256 MiB input limit: {}",
        path.display()
    );
    let mut bytes = Vec::new();
    file.take(LIMIT + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() as u64 <= LIMIT,
        "asset grew beyond its input limit"
    );
    Ok(bytes)
}

fn logical(path: &Path) -> Result<String> {
    ensure!(!path.is_absolute(), "asset reference must be relative");
    let mut parts = Vec::new();
    for component in path.components() {
        let Component::Normal(part) = component else {
            bail!("asset reference contains traversal");
        };
        let text = part.to_str().context("asset reference is not Unicode")?;
        ensure!(
            !text.contains(':') && !text.contains('\\'),
            "asset reference contains an invalid separator"
        );
        parts.push(text.to_lowercase());
    }
    // Native Windows paths use backslashes; the resource index uses '/' on every OS.
    Ok(parts.join("/"))
}

fn find_child(root: &Path, name: &str) -> Result<PathBuf> {
    logical(Path::new(name))?;
    ensure!(
        !name.contains(['/', '\\']),
        "geometry label must name a sibling file"
    );
    let matches = fs::read_dir(root)?
        .filter_map(|r| r.ok())
        .filter(|e| e.file_name().to_string_lossy().to_lowercase() == name.to_lowercase())
        .map(|e| e.path())
        .collect::<Vec<_>>();
    ensure!(
        matches.len() == 1,
        "missing or ambiguous model file: {}",
        root.join(name).display()
    );
    let result = matches[0].canonicalize()?;
    ensure!(
        result.starts_with(root.canonicalize()?),
        "model reference escapes its directory"
    );
    Ok(result)
}

struct TextureFiles {
    root: PathBuf,
    paths: BTreeMap<String, PathBuf>,
    prefixes: Vec<String>,
}
impl TextureFiles {
    fn new(game: &Path, model: &Path, override_dir: Option<&Path>) -> Result<Self> {
        let resource = if game
            .file_name()
            .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case("resource"))
        {
            game.canonicalize()?
        } else {
            find_child(game, "RESOURCE")?
        };
        let root = find_child(&resource, "Textures")?;
        let mut prefixes = Vec::new();
        if let Some(dir) = override_dir {
            prefixes.push(logical(dir)?);
        }
        if let Ok(relative) = model
            .parent()
            .context("model has no directory")?
            .strip_prefix(find_child(&resource, "MODELS")?)
        {
            let path = logical(relative)?;
            prefixes.push(format!("{path}/hull1"));
            prefixes.push(path);
        }
        prefixes.extend(["ships".to_owned(), String::new()]);
        let mut paths = BTreeMap::new();
        let mut dirs = vec![root.clone()];
        while let Some(dir) = dirs.pop() {
            for e in fs::read_dir(dir)? {
                let e = e?;
                let t = e.file_type()?;
                if t.is_dir() {
                    dirs.push(e.path());
                } else if t.is_file()
                    && e.path()
                        .extension()
                        .is_some_and(|v| v.eq_ignore_ascii_case("tx"))
                {
                    let path = e.path();
                    let key = logical(path.strip_prefix(&root)?)?;
                    ensure!(
                        paths.insert(key, path).is_none(),
                        "case-colliding texture paths"
                    );
                }
            }
        }
        Ok(Self {
            root,
            paths,
            prefixes,
        })
    }
    fn find(&self, name: &str) -> Result<PathBuf> {
        let name = name.replace('\\', "/");
        let mut name = logical(Path::new(&name))?;
        if !name.ends_with(".tx") {
            name.push_str(".tx");
        }
        for prefix in &self.prefixes {
            let candidate = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if let Some(path) = self.paths.get(&candidate) {
                return Ok(path.clone());
            }
        }
        let matches = self
            .paths
            .iter()
            .filter(|(key, _)| key.rsplit('/').next() == Some(name.as_str()))
            .map(|(_, p)| p)
            .collect::<Vec<_>>();
        if matches.len() == 1 {
            return Ok(matches[0].clone());
        }
        bail!(
            "missing or ambiguous texture {name:?} under {}; select --texture-dir",
            self.root.display()
        )
    }
}

impl ModelAsset {
    pub fn load(path: &Path, options: &LoadOptions) -> Result<Self> {
        let path = path
            .canonicalize()
            .with_context(|| format!("model {}", path.display()))?;
        let white = Texture {
            header: TxHeader {
                flags: 0,
                width: 1,
                height: 1,
                mips: 1,
                format: 21,
                mip0_bytes: 4,
            },
            mips: vec![Mip {
                width: 1,
                height: 1,
                rgba: vec![255; 4],
            }],
        };
        let files = if options.untextured {
            None
        } else {
            Some(TextureFiles::new(
                options
                    .game_dir
                    .as_deref()
                    .context("textured models require --game-dir (or use --untextured)")?,
                &path,
                options.texture_dir.as_deref(),
            )?)
        };
        let mut builder = Builder {
            asset: Self {
                meshes: Vec::new(),
                textures: vec![white],
                center: Vec3::ZERO,
                radius: 1.0,
                geometry_files: 0,
                sails: 0,
            },
            files,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            single: options.single_mesh,
            render_bytes: 4,
        };
        let stem = path
            .file_stem()
            .context("model has no name")?
            .to_string_lossy()
            .into_owned();
        builder.node(&path, &stem, Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)), 0)?;
        ensure!(
            !builder.asset.meshes.is_empty(),
            "model contains no visible static geometry"
        );
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in builder.asset.meshes.iter().flat_map(|m| &m.vertices) {
            let p = Vec3::from_array(vertex.position);
            ensure!(
                p.is_finite(),
                "model transform produced a non-finite vertex"
            );
            min = min.min(p);
            max = max.max(p);
        }
        builder.asset.center = (min + max) * 0.5;
        builder.asset.radius = ((max - min) * 0.5).length().max(0.1);
        ensure!(
            builder.asset.center.is_finite() && builder.asset.radius.is_finite(),
            "model bounds overflow"
        );
        Ok(builder.asset)
    }
}

struct Builder {
    asset: ModelAsset,
    files: Option<TextureFiles>,
    cache: BTreeMap<PathBuf, usize>,
    active: BTreeSet<PathBuf>,
    single: bool,
    render_bytes: usize,
}
impl Builder {
    fn reserve(&mut self, bytes: usize) -> Result<()> {
        self.render_bytes = self
            .render_bytes
            .checked_add(bytes)
            .context("model size overflow")?;
        ensure!(
            self.render_bytes <= 512 * 1024 * 1024,
            "model exceeds the 512 MiB decoded scene limit"
        );
        Ok(())
    }
    fn texture(&mut self, name: &str) -> Result<usize> {
        let Some(files) = &self.files else {
            return Ok(0);
        };
        let path = files.find(name)?;
        if let Some(&id) = self.cache.get(&path) {
            return Ok(id);
        }
        let texture = Texture::read(&read_asset_bytes(&path)?)
            .with_context(|| format!("decode {}", path.display()))?;
        self.reserve(texture.mips.iter().map(|m| m.rgba.len()).sum())?;
        let id = self.asset.textures.len();
        self.asset.textures.push(texture);
        self.cache.insert(path, id);
        Ok(id)
    }
    fn node(&mut self, path: &Path, stem: &str, world: Mat4, depth: usize) -> Result<()> {
        ensure!(
            depth < 16 && self.asset.geometry_files < 256,
            "model hierarchy limit exceeded"
        );
        ensure!(
            self.active.insert(path.to_owned()),
            "cycle in model geometry hierarchy"
        );
        ensure!(
            world.is_finite() && world.determinant().abs() > 1e-8,
            "invalid geometry transform"
        );
        let model = Geometry::read(&read_asset_bytes(path)?)
            .with_context(|| format!("decode {}", path.display()))?;
        self.asset.geometry_files += 1;
        let normals = world.inverse().transpose();
        for object in &model.objects {
            if object.flags & 1 == 0 || object.triangle_count == 0 {
                continue;
            }
            let mat = &model.materials[object.material];
            let texture = match mat.texture {
                Some(i) => self.texture(&model.textures[i])?,
                None => 0,
            };
            self.reserve(
                object.vertex_count * std::mem::size_of::<ModelVertex>()
                    + object.triangle_count * 3 * std::mem::size_of::<u32>(),
            )?;
            let vertices = model.buffers[object.buffer]
                [object.first_vertex..object.first_vertex + object.vertex_count]
                .iter()
                .map(|v| ModelVertex {
                    position: world
                        .transform_point3(Vec3::from_array(v.position))
                        .to_array(),
                    normal: normals
                        .transform_vector3(Vec3::from_array(v.normal))
                        .normalize_or_zero()
                        .to_array(),
                    uv: v.uv,
                    // Lighting is recomputed in this preview; COL lighting is not loaded yet.
                    color: [1.0, 1.0, 1.0, 1.0],
                })
                .collect();
            let indices = model.triangles
                [object.first_triangle..object.first_triangle + object.triangle_count]
                .iter()
                .flatten()
                .map(|&i| u32::from(i))
                .collect();
            self.asset.meshes.push(Mesh {
                vertices,
                indices,
                texture,
            });
        }
        if !self.single {
            let mut sails = BTreeMap::<&str, [Option<Vec3>; 4]>::new();
            for label in &model.labels {
                if label.group.starts_with("sail")
                    && let Some(index) = ["sail1", "sail2", "sail3", "sail4"]
                        .iter()
                        .position(|&s| s == label.name)
                {
                    sails.entry(&label.group).or_insert([None; 4])[index] =
                        Some(world.transform_point3(
                            Mat4::from_cols_array(&label.matrix).w_axis.truncate(),
                        ));
                }
            }
            for corners in sails.values() {
                if let [Some(a), Some(b), Some(c), Some(d)] = *corners {
                    let texture = self.texture("parus_common.tga")?;
                    let mesh = sail_patch([a, b, c, d], texture);
                    self.reserve(
                        mesh.vertices.len() * std::mem::size_of::<ModelVertex>()
                            + mesh.indices.len() * std::mem::size_of::<u32>(),
                    )?;
                    self.asset.meshes.push(mesh);
                    self.asset.sails += 1;
                }
            }
            for label in &model.labels {
                if label.group == "geometry"
                    && !matches!(label.name.as_str(), "path" | "fonar_night")
                {
                    let name = format!("{stem}_{}.gm", label.name);
                    let child = find_child(path.parent().context("model has no parent")?, &name)?;
                    self.node(
                        &child,
                        stem,
                        world * Mat4::from_cols_array(&label.matrix),
                        depth + 1,
                    )?;
                }
            }
        }
        self.active.remove(path);
        Ok(())
    }
}

fn sail_patch(p: [Vec3; 4], texture: usize) -> Mesh {
    const N: u32 = 12;
    let normal = (p[1] - p[0]).cross(p[3] - p[0]).normalize_or_zero();
    let bulge = (p[1] - p[0]).length().min((p[3] - p[0]).length()) * 0.12;
    let position = |u: f32, v: f32| {
        p[0].lerp(p[1], u).lerp(p[3].lerp(p[2], u), v)
            + normal * (u * std::f32::consts::PI).sin() * (v * std::f32::consts::PI).sin() * bulge
    };
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for y in 0..=N {
        for x in 0..=N {
            let u = x as f32 / N as f32;
            let v = y as f32 / N as f32;
            let du = position(u + 0.001, v) - position(u - 0.001, v);
            let dv = position(u, v + 0.001) - position(u, v - 0.001);
            vertices.push(ModelVertex {
                position: position(u, v).to_array(),
                normal: du.cross(dv).normalize_or_zero().to_array(),
                uv: [u, v],
                color: [1.0, 0.97, 0.88, 1.0],
            });
            if x < N && y < N {
                let a = y * (N + 1) + x;
                indices.extend([a, a + 1, a + N + 1, a + 1, a + N + 2, a + N + 1]);
            }
        }
    }
    Mesh {
        vertices,
        indices,
        texture,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_cannot_escape_and_sail_indices_fit() {
        for path in ["../secret", "/absolute", "c:/drive", "a/../../b"] {
            assert!(logical(Path::new(path)).is_err());
        }
        let native = Path::new("Ships").join("Barkentine1").join("Hull1");
        assert_eq!(logical(&native).unwrap(), "ships/barkentine1/hull1");
        let mesh = sail_patch([Vec3::ZERO, Vec3::X, Vec3::X + Vec3::Y, Vec3::Y], 0);
        assert!(
            mesh.indices
                .iter()
                .all(|&i| (i as usize) < mesh.vertices.len())
        );
        assert!(
            mesh.vertices
                .iter()
                .all(|v| Vec3::from_array(v.normal).is_finite())
        );
    }
}
