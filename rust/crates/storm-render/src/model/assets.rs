use super::{
    ModelVertex, RigMesh, RiggingConfig, RiggingStats, SailPatch, SailShape, SailWind, rigging,
    sails,
};
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
mod particle_project;
mod presentation;

pub struct Mesh {
    pub ship_node: Option<String>,
    pub dynamic: Option<super::DynamicMesh>,
    pub vertices: Vec<ModelVertex>,
    pub indices: Vec<u32>,
    pub texture: usize,
    pub sail: Option<SailPatch>,
    pub rigging: Option<RigMesh>,
}

impl Mesh {
    pub fn sail(patch: SailPatch, texture: usize) -> Result<Self> {
        let ship_node = patch.origin.as_ref().map(|o| o.node.clone());
        Ok(Self {
            vertices: patch.vertices(SailWind::default(), 0.0)?,
            indices: patch.indices(),
            texture,
            sail: Some(patch),
            rigging: None,
            ship_node,
            dynamic: None,
        })
    }
}

pub struct ModelAsset {
    /// Original root GM bounds, before attaching masts, sails and other nodes.
    pub root_box_size: Option<[f32; 3]>,
    pub sail_damage: Option<super::sail_damage::SailDamageMaterial>,
    pub ship: super::ShipAsset,
    pub ship_state: super::ShipState,
    pub meshes: Vec<Mesh>,
    pub textures: Vec<Texture>,
    pub center: Vec3,
    pub radius: f32,
    pub geometry_files: usize,
    pub sails: usize,
    pub rigging_stats: RiggingStats,
}

#[derive(Default)]
pub struct LoadOptions {
    /// "default" scans XPS names; otherwise a native project name or absolute PRJ path.
    pub particle_project: Option<PathBuf>,
    /// Additional profiles cached for native fire messages, relative to RESOURCE/Particles.
    pub particle_profiles: Vec<String>,
    /// None uses the original preview default (five) when a ship graph exists.
    pub crew: Option<usize>,
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
    /// Generated rigging fixture; no original game assets are bundled.
    pub fn rigging_demo() -> Result<Self> {
        use storm_formats::geometry::Label;
        let mut asset = Self::sail_demo()?;
        let mut collector = rigging::Collector::default();
        let labels = [
            ("hull", "ropeb1", Vec3::ZERO),
            ("mast", "ropee1", Vec3::new(0.0, 10.0, 0.0)),
            ("hull", "ropeb2", Vec3::new(4.0, 0.0, 0.0)),
            ("mast", "ropee2", Vec3::new(0.0, 9.0, 0.0)),
            ("vants", "vant1u", Vec3::new(0.0, 7.0, 0.0)),
            ("vants", "vant1l", Vec3::new(-2.0, 0.0, 1.0)),
            ("vants", "vant1r", Vec3::new(-2.0, 0.0, -1.0)),
            ("flag1", "f1", Vec3::new(0.0, 10.0, 0.0)),
            ("flag1", "f2", Vec3::new(3.0, 10.0, 0.0)),
            ("flag1", "f3", Vec3::new(3.0, 9.0, 0.0)),
            ("flag1", "f4", Vec3::new(0.0, 9.0, 0.0)),
            ("sflag2", "f1", Vec3::new(-4.0, 8.0, 0.0)),
            ("sflag2", "f2", Vec3::new(-1.0, 7.6, 0.0)),
            ("sflag2", "f3", Vec3::new(-4.0, 7.2, 0.0)),
        ]
        .map(|(group, name, p)| Label {
            group: group.into(),
            name: name.into(),
            matrix: Mat4::from_translation(p).to_cols_array(),
        });
        collector.add(&labels, Mat4::IDENTITY, "fixture");
        let config = RiggingConfig::default();
        let (ropes, flags, stats) = collector.build(&[], &config)?;
        asset.rigging_stats = stats;
        asset.textures.push(Texture {
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
        });
        // Each cell has a distinct color; this fixture contains no game artwork.
        let rgba = (0..128)
            .flat_map(|y| {
                (0..64).flat_map(move |x| {
                    let base = [
                        [240u8, 40, 40],
                        [40, 220, 40],
                        [40, 40, 240],
                        [230, 210, 30],
                    ][x / 16];
                    let scale = 1.0 - (y / 16) as f32 * 0.07;
                    [
                        (base[0] as f32 * scale) as u8,
                        (base[1] as f32 * scale) as u8,
                        (base[2] as f32 * scale) as u8,
                        255,
                    ]
                })
            })
            .collect();
        asset.textures.push(Texture {
            header: TxHeader {
                flags: 0,
                width: 64,
                height: 128,
                mips: 1,
                format: 21,
                mip0_bytes: 64 * 128 * 4,
            },
            mips: vec![Mip {
                width: 64,
                height: 128,
                rgba,
            }],
        });
        let mut append = |rigging: RigMesh, texture| -> Result<()> {
            let vertices =
                rigging.vertices(SailWind::default(), 0.0, storm_sim::RigState::uniform(1.0))?;
            let indices = rigging.indices();
            asset.meshes.push(Mesh {
                vertices,
                indices,
                texture,
                sail: None,
                ship_node: rigging.node().map(str::to_owned),
                dynamic: None,
                rigging: Some(rigging),
            });
            Ok(())
        };
        append(RigMesh::Ropes(ropes), 1)?;
        for mut flag in flags {
            flag.set_texture_size(64, 128)?;
            append(RigMesh::Flag(flag), 2)?;
        }
        asset.center = Vec3::new(0.0, 5.0, 0.0);
        asset.radius = 8.0;
        Ok(asset)
    }
    /// Redistributable procedural fixture for inspecting both sail topologies without a game.
    pub fn sail_demo() -> Result<Self> {
        let quad = SailPatch::new(SailShape::Quad([
            Vec3::new(-4.0, 7.0, 0.0),
            Vec3::new(-0.5, 7.0, 0.0),
            Vec3::new(-0.5, 1.0, 0.0),
            Vec3::new(-4.0, 1.0, 0.0),
        ]))?;
        let mut triangle = SailPatch::new(SailShape::Triangle([
            Vec3::new(1.0, 7.0, 0.0),
            Vec3::new(1.0, 1.0, 0.0),
            Vec3::new(5.0, 1.0, 0.0),
        ]))?;
        triangle.tier = storm_sim::SailTier::FullOnly;
        let rgba = (0..64)
            .flat_map(|i| {
                if (i / 8 + i % 8) % 2 == 0 {
                    [235, 220, 185, 255]
                } else {
                    [160, 180, 195, 255]
                }
            })
            .collect();
        Ok(Self {
            sail_damage: Some(super::sail_damage::SailDamageMaterial::fixture()),
            root_box_size: None,
            ship: Default::default(),
            ship_state: Default::default(),
            meshes: vec![Mesh::sail(quad, 0)?, Mesh::sail(triangle, 0)?],
            textures: vec![Texture {
                header: TxHeader {
                    flags: 0,
                    width: 8,
                    height: 8,
                    mips: 1,
                    format: 21,
                    mip0_bytes: 256,
                },
                mips: vec![Mip {
                    width: 8,
                    height: 8,
                    rgba,
                }],
            }],
            center: Vec3::new(0.5, 4.0, 0.0),
            radius: 5.5,
            geometry_files: 0,
            sails: 2,
            rigging_stats: RiggingStats::default(),
        })
    }

    pub fn triangular_sails(&self) -> usize {
        self.meshes
            .iter()
            .filter(|m| m.sail.as_ref().is_some_and(SailPatch::is_triangle))
            .count()
    }
    pub fn set_flags(&mut self, selection: storm_sim::FlagSelection) -> Result<()> {
        rigging::select_flags(
            self.meshes.iter_mut().filter_map(|m| m.rigging.as_mut()),
            selection,
        )?;
        for mesh in &mut self.meshes {
            if let Some(rigging @ RigMesh::Flag(_)) = &mesh.rigging {
                rigging.deform(
                    &mut mesh.vertices,
                    SailWind::default(),
                    0.0,
                    storm_sim::RigState::default(),
                )?;
            }
        }
        Ok(())
    }
    pub fn rig_layout(&self) -> storm_sim::RigLayout {
        self.rig_layout_at(&self.ship_state, 0.0)
    }
    pub fn rig_layout_at(&self, state: &super::ShipState, seconds: f32) -> storm_sim::RigLayout {
        let mut layout = storm_sim::RigLayout {
            normal_weight: 0.0,
            full_only_weight: 0.0,
            missing_weight: 0.0,
        };
        for (index, patch) in self
            .meshes
            .iter()
            .enumerate()
            .filter_map(|(i, m)| m.sail.as_ref().map(|p| (i, p)))
        {
            if patch.origin.as_ref().is_some_and(|o| {
                self.ship
                    .fallen_mast(&o.node, state)
                    .is_some_and(|(_, t, _)| seconds >= t)
            }) {
                layout.missing_weight += patch.weight;
                continue;
            }
            let intact = patch.intact_fraction(state.sail_holes.get(&index).copied().unwrap_or(0));
            layout.missing_weight += patch.weight * (1.0 - intact);
            match patch.tier {
                storm_sim::SailTier::Normal => layout.normal_weight += patch.weight * intact,
                storm_sim::SailTier::FullOnly => layout.full_only_weight += patch.weight * intact,
            }
        }
        layout
    }

    pub fn load(path: &Path, options: &LoadOptions) -> Result<Self> {
        ensure!(
            (options.particle_profiles.is_empty() && options.particle_project.is_none())
                || (!options.single_mesh && !options.untextured && options.game_dir.is_some()),
            "particle profiles require a textured ship hierarchy and game directory"
        );
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
                sail_damage: None,
                root_box_size: None,
                ship: Default::default(),
                ship_state: Default::default(),
                meshes: Vec::new(),
                textures: vec![white],
                center: Vec3::ZERO,
                radius: 1.0,
                geometry_files: 0,
                sails: 0,
                rigging_stats: RiggingStats::default(),
            },
            files,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            single: options.single_mesh,
            rigging: rigging::Collector::default(),
            render_bytes: 4,
        };
        let stem = path
            .file_stem()
            .context("model has no name")?
            .to_string_lossy()
            .into_owned();
        builder.node(
            &path,
            &stem,
            &stem,
            "",
            Mat4::from_scale(Vec3::new(1.0, 1.0, -1.0)),
            0,
        )?;
        if !options.single_mesh {
            builder.add_rigging()?;
            if builder.asset.sails > 0 {
                if let Some(files) = &builder.files {
                    let path = files.find("ships/parus_hole.tga")?;
                    let texture = Texture::read(&read_asset_bytes(&path)?)?;
                    builder.reserve(texture.mips.iter().map(|m| m.rgba.len()).sum())?;
                    builder.asset.sail_damage = Some(super::sail_damage::SailDamageMaterial {
                        texture,
                        source: Some(path.display().to_string()),
                    });
                } else {
                    builder.asset.sail_damage =
                        Some(super::sail_damage::SailDamageMaterial::fixture());
                }
            }
        }
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
        if !options.single_mesh {
            builder.add_presentation(&stem, options)?;
        }
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
    rigging: rigging::Collector,
}
impl Builder {
    fn add_rigging(&mut self) -> Result<()> {
        let mut config = RiggingConfig::default();
        if let Some(files) = &self.files {
            let resource = files
                .root
                .parent()
                .context("texture root has no RESOURCE parent")?;
            if let Ok(ini_dir) = find_child(resource, "INI")
                && let Ok(ini) = find_child(&ini_dir, "rigging.ini")
            {
                let text = String::from_utf8_lossy(&read_asset_bytes(&ini)?).into_owned();
                config = RiggingConfig::from_ini(&text)?;
                let settings = super::sail_damage::SailDamageSettings::from_ini(&text)?;
                for patch in self.asset.meshes.iter_mut().filter_map(|m| m.sail.as_mut()) {
                    patch.damage_settings = settings;
                }
            }
        }
        let patches = self
            .asset
            .meshes
            .iter()
            .filter_map(|m| m.sail.clone())
            .collect::<Vec<_>>();
        let (ropes, flags, stats) = self.rigging.build(&patches, &config)?;
        self.asset.rigging_stats = stats;
        if !ropes.is_empty() {
            let texture = self.texture(&config.rope_texture)?;
            self.add_rig_mesh(RigMesh::Ropes(ropes), texture)?;
        }
        if !flags.is_empty() {
            let texture = self.texture(&config.flag_texture)?;
            for mut flag in flags {
                if self.files.is_some() {
                    let t = self.asset.textures[texture].header;
                    flag.set_texture_size(t.width, t.height)?;
                }
                self.add_rig_mesh(RigMesh::Flag(flag), texture)?;
            }
        }
        Ok(())
    }
    fn add_rig_mesh(&mut self, rigging: RigMesh, texture: usize) -> Result<()> {
        self.reserve(
            rigging.vertex_count() * std::mem::size_of::<ModelVertex>() + rigging.index_count() * 4,
        )?;
        let vertices =
            rigging.vertices(SailWind::default(), 0.0, storm_sim::RigState::uniform(1.0))?;
        let indices = rigging.indices();
        self.asset.meshes.push(Mesh {
            vertices,
            indices,
            texture,
            sail: None,
            ship_node: rigging.node().map(str::to_owned),
            dynamic: None,
            rigging: Some(rigging),
        });
        Ok(())
    }
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
        self.texture_path(path)
    }
    fn texture_path(&mut self, path: PathBuf) -> Result<usize> {
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
    fn node(
        &mut self,
        path: &Path,
        stem: &str,
        node: &str,
        parent: &str,
        world: Mat4,
        depth: usize,
    ) -> Result<()> {
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
        ensure!(
            model.buffers.iter().flatten().all(|v| v.skin.is_none()),
            "animated geometry requires the crew animation loader"
        );
        let center = Vec3::from_array(model.header.bounds_center);
        let size = Vec3::from_array(model.header.bounds_size);
        if depth == 0 {
            self.asset.root_box_size = Some(model.header.bounds_size);
        }
        let mut pivot = center - Vec3::Y * (size.y * 0.5);
        if super::part_number(node, "mast") == Some(1) {
            pivot.z -= size.z * 0.5;
        }
        self.asset.ship.nodes.push(super::ShipNode {
            name: node.into(),
            parent: parent.into(),
            pivot: world.transform_point3(pivot),
        });
        self.asset
            .ship
            .node_frames
            .insert(node.into(), world.to_cols_array());
        for label in &model.labels {
            if matches!(label.group.as_str(), "fireplace" | "fireplaces") {
                self.asset.ship.fires.push(super::FirePoint {
                    node: node.into(),
                    name: label.name.clone(),
                    position: world
                        .transform_point3(Mat4::from_cols_array(&label.matrix).w_axis.truncate()),
                });
            }
        }
        ensure!(self.asset.ship.fires.len() <= 256, "too many fire locators");
        self.asset.geometry_files += 1;
        self.rigging.add(&model.labels, world, node);
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
                sail: None,
                rigging: None,
                ship_node: Some(node.into()),
                dynamic: None,
            });
        }
        if !self.single {
            for patch in sails::from_labels(&model.labels, world, node, parent)
                .with_context(|| format!("sails in {}", path.display()))?
            {
                let texture = self.texture("parus_common.tga")?;
                let mesh = Mesh::sail(patch, texture)?;
                self.reserve(
                    mesh.vertices.len() * std::mem::size_of::<ModelVertex>()
                        + mesh.indices.len() * std::mem::size_of::<u32>(),
                )?;
                self.asset.meshes.push(mesh);
                self.asset.sails += 1;
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
                        &label.name,
                        node,
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn references_cannot_escape_and_native_separators_match() {
        for path in ["../secret", "/absolute", "c:/drive", "a/../../b"] {
            assert!(logical(Path::new(path)).is_err());
        }
        let native = Path::new("Ships").join("Barkentine1").join("Hull1");
        assert_eq!(logical(&native).unwrap(), "ships/barkentine1/hull1");
    }
    #[test]
    fn native_root_dimensions_are_retained_from_geometry_header() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../tools/modding-tool/Meshes/cannon_x.gm");
        let header = Geometry::read(&read_asset_bytes(&path).unwrap())
            .unwrap()
            .header;
        let asset = ModelAsset::load(
            &path,
            &LoadOptions {
                untextured: true,
                single_mesh: true,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(asset.root_box_size, Some(header.bounds_size));
        assert_eq!(ModelAsset::ship_demo().unwrap().root_box_size, None);
    }
}
