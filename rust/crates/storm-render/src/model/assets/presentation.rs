//! Original crew resources and fireplace particle materials stay external.
use super::*;
use crate::model::{CrewPatch, DynamicMesh, ParticlePatch, Patrol, Skeleton};
use std::sync::Arc;
use storm_formats::{
    animation::{Animation, Clip},
    waypoints::DeckGraph,
};

impl ModelAsset {
    /// Generated crew/particle/damage fixture; no commercial data.
    pub fn ship_demo() -> Result<Self> {
        use crate::model::{FirePoint, RigMesh, SailOrigin, ShipNode};
        use glam::Quat;
        use storm_formats::geometry::{Label, Skin, Vertex};
        use storm_formats::waypoints::DeckPoint;
        let mut asset = Self::rigging_demo()?;
        asset.ship.nodes = vec![
            ShipNode {
                name: "fixture".into(),
                parent: String::new(),
                pivot: Vec3::ZERO,
            },
            ShipNode {
                name: "mast1".into(),
                parent: "fixture".into(),
                pivot: Vec3::new(2.5, 1.0, 0.0),
            },
            ShipNode {
                name: "mast2".into(),
                parent: "fixture".into(),
                pivot: Vec3::new(-2.0, 1.0, 0.0),
            },
            ShipNode {
                name: "mast201".into(),
                parent: "fixture".into(),
                pivot: Vec3::new(-2.0, 6.0, 0.0),
            },
            ShipNode {
                name: "rey_b2".into(),
                parent: "mast2".into(),
                pivot: Vec3::new(-2.0, 1.0, 0.0),
            },
            ShipNode {
                name: "shatter1".into(),
                parent: "fixture".into(),
                pivot: Vec3::new(0.0, 0.5, 4.5),
            },
        ];
        asset.ship.fires = vec![FirePoint {
            node: "fixture".into(),
            name: "fire1".into(),
            position: Vec3::new(-1.0, 1.0, 0.0),
        }];
        asset.ship.deck_center = Some(Vec3::Y);
        for mesh in &mut asset.meshes {
            if let Some(patch) = &mut mesh.sail {
                let node = if patch.is_triangle() {
                    "mast1"
                } else {
                    "rey_b2"
                };
                patch.origin = Some(SailOrigin {
                    node: node.into(),
                    parent: if patch.is_triangle() {
                        "fixture"
                    } else {
                        "mast2"
                    }
                    .into(),
                    group: "sails1".into(),
                });
                mesh.ship_node = Some(node.into());
            }
            if let Some(RigMesh::Flag(f)) = &mut mesh.rigging
                && let Some(o) = &mut f.origin
            {
                o.node = if o.special { "mast1" } else { "mast2" }.into();
                mesh.ship_node = Some(o.node.clone());
            }
        }
        asset
            .meshes
            .retain(|m| !matches!(m.rigging, Some(RigMesh::Ropes(_))));
        let mut collector = rigging::Collector::default();
        let mut add = |node: &str, group: &str, name: &str, p: Vec3| {
            collector.add(
                &[Label {
                    group: group.into(),
                    name: name.into(),
                    matrix: Mat4::from_translation(p).to_cols_array(),
                }],
                Mat4::IDENTITY,
                node,
            )
        };
        add("fixture", "hull", "ropeb1", Vec3::new(-4.0, 1.0, 1.0));
        add("mast2", "mast", "ropee1", Vec3::new(-2.0, 9.0, 0.0));
        add("fixture", "hull", "ropeb2", Vec3::new(4.0, 1.0, 1.0));
        add("mast1", "mast", "ropee2", Vec3::new(2.5, 7.0, 0.0));
        add("mast2", "vants", "vant1u", Vec3::new(-2.0, 8.0, 0.0));
        add("fixture", "vants", "vant1l", Vec3::new(-3.0, 1.0, 1.0));
        add("fixture", "vants", "vant1r", Vec3::new(-3.0, 1.0, -1.0));
        let (ropes, _, _) = collector.build(&[], &RiggingConfig::default())?;
        let mut builder = Builder {
            asset,
            files: None,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            single: false,
            rigging: collector,
            render_bytes: 0,
        };
        builder.add_rig_mesh(RigMesh::Ropes(ropes), 1)?;
        fn box_data(center: Vec3, size: Vec3) -> (Vec<ModelVertex>, Vec<u32>) {
            let mut vertices = Vec::new();
            let mut indices = Vec::new();
            for (normal, a, b) in [
                (Vec3::X, Vec3::Y, Vec3::Z),
                (-Vec3::X, Vec3::Z, Vec3::Y),
                (Vec3::Y, Vec3::Z, Vec3::X),
                (-Vec3::Y, Vec3::X, Vec3::Z),
                (Vec3::Z, Vec3::X, Vec3::Y),
                (-Vec3::Z, Vec3::Y, Vec3::X),
            ] {
                let offset = vertices.len() as u32;
                for (u, v) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                    vertices.push(ModelVertex {
                        position: (center + (normal + a * u + b * v) * size * 0.5).to_array(),
                        normal: normal.to_array(),
                        uv: [(u + 1.0) * 0.5, (v + 1.0) * 0.5],
                        color: [0.55, 0.35, 0.16, 1.0],
                    });
                }
                indices.extend([
                    offset,
                    offset + 1,
                    offset + 2,
                    offset,
                    offset + 2,
                    offset + 3,
                ]);
            }
            (vertices, indices)
        }
        for (node, center, size) in [
            ("mast2", Vec3::new(-2.0, 5.0, 0.0), Vec3::new(0.2, 8.0, 0.2)),
            ("mast1", Vec3::new(2.5, 4.0, 0.0), Vec3::new(0.2, 6.0, 0.2)),
        ] {
            let (vertices, indices) = box_data(center, size);
            builder.asset.meshes.push(Mesh {
                vertices,
                indices,
                texture: 1,
                sail: None,
                rigging: None,
                dynamic: None,
                ship_node: Some(node.into()),
            });
        }
        let (vertices, _) = box_data(Vec3::new(0.0, 0.5, 0.0), Vec3::new(8.0, 1.0, 9.0));
        for face in 0..6 {
            builder.asset.meshes.push(Mesh {
                vertices: vertices[face * 4..face * 4 + 4].to_vec(),
                indices: vec![0, 1, 2, 0, 2, 3],
                texture: 1,
                sail: None,
                rigging: None,
                dynamic: None,
                ship_node: Some(if face == 4 { "shatter1" } else { "fixture" }.into()),
            });
        }
        let frames = 4;
        let rotations = [
            vec![Quat::IDENTITY; frames],
            vec![
                Quat::IDENTITY,
                Quat::from_rotation_x(0.7),
                Quat::from_rotation_x(-0.7),
                Quat::IDENTITY,
            ],
        ]
        .into_iter()
        .flatten()
        .map(|q| q.to_array())
        .collect();
        let animation = Animation {
            frames,
            fps: 3.0,
            parents: vec![None, Some(0)],
            bind_positions: vec![[0.0, 0.9, 0.0], [0.0, -0.2, 0.0]],
            root_positions: vec![[0.0, 0.9, 0.0]; frames],
            rotations,
            order: vec![0, 1],
        };
        let skeleton = Arc::new(Skeleton::new(animation, Clip { first: 0, last: 3 }));
        let mut source = Vec::new();
        let mut indices = Vec::new();
        for (center, size, bone) in [
            (Vec3::new(0.0, 1.0, 0.0), Vec3::new(0.4, 0.65, 0.3), 0),
            (Vec3::new(0.0, 1.5, 0.0), Vec3::splat(0.28), 0),
            (Vec3::new(-0.12, 0.38, 0.0), Vec3::new(0.15, 0.7, 0.2), 1),
            (Vec3::new(0.12, 0.38, 0.0), Vec3::new(0.15, 0.7, 0.2), 0),
            (Vec3::new(-0.3, 1.0, 0.0), Vec3::new(0.15, 0.65, 0.18), 0),
            (Vec3::new(0.3, 1.0, 0.0), Vec3::new(0.15, 0.65, 0.18), 1),
        ] {
            let (vertices, triangles) = box_data(center, size);
            let offset = source.len() as u32;
            indices.extend(triangles.into_iter().map(|i| i + offset));
            source.extend(vertices.into_iter().map(|v| Vertex {
                position: v.position,
                normal: v.normal,
                color: [1.0; 4],
                uv: v.uv,
                skin: Some(Skin {
                    bones: [bone, bone],
                    weight: 1.0,
                }),
            }));
        }
        let source = Arc::new(source);
        let graph = DeckGraph {
            missing_links: 0,
            points: [
                [-2.0, 1.0, -3.0],
                [2.0, 1.0, -3.0],
                [2.0, 1.0, 3.0],
                [-2.0, 1.0, 3.0],
            ]
            .into_iter()
            .map(|position| DeckPoint { position, kind: 0 })
            .collect(),
            links: vec![[0, 1], [1, 2], [2, 3], [3, 0]],
        };
        for id in 0..3 {
            builder.add_dynamic(
                DynamicMesh::Crew(CrewPatch {
                    skeleton: skeleton.clone(),
                    source: source.clone(),
                    route: Arc::new(Patrol::from_graph(&graph, id)?),
                    phase: id as f32 * 2.173,
                }),
                indices.clone(),
                1,
            )?;
        }
        builder.asset.ship.crew_count = 3;
        builder.asset.ship.waypoint_counts = Some([4, 4]);
        builder.add_fire_effects(&[])?;
        Ok(builder.asset)
    }
}

impl Builder {
    pub(super) fn add_presentation(&mut self, stem: &str, options: &LoadOptions) -> Result<()> {
        if let Some(files) = &self.files {
            let resource = files
                .root
                .parent()
                .context("missing resource root")?
                .to_path_buf();
            let models = find_child(&resource, "MODELS")?;
            let graph_path = find_child(&models, "Ships")
                .and_then(|p| find_child(&p, "SAILORSPOINTS"))
                .and_then(|p| find_child(&p, &format!("{stem}.ini")));
            let count = options.crew.unwrap_or(5);
            ensure!(count <= 50, "crew preview limit is 50");
            if count > 0 {
                match graph_path {
                    Ok(path) => self.add_crew(&resource, &models, &path, count)?,
                    Err(error) if options.crew.is_some() => {
                        return Err(error.context("requested crew has no original waypoint file"));
                    }
                    Err(_) => {}
                }
            }
        } else {
            ensure!(
                options.crew.is_none_or(|n| n == 0),
                "original crew requires --game-dir"
            );
        }
        self.add_fire_effects(&options.particle_profiles)?;
        Ok(())
    }
    fn add_crew(
        &mut self,
        resource: &Path,
        models: &Path,
        path: &Path,
        count: usize,
    ) -> Result<()> {
        let graph = DeckGraph::read(&String::from_utf8_lossy(&read_asset_bytes(path)?))?;
        let normal = graph
            .points
            .iter()
            .filter(|p| p.kind == 0)
            .map(|p| Vec3::from_array(p.position) * Vec3::new(1.0, 1.0, -1.0))
            .collect::<Vec<_>>();
        ensure!(!normal.is_empty(), "crew graph has no deck points");
        self.asset.ship.deck_center =
            Some(normal.iter().copied().sum::<Vec3>() / normal.len() as f32);
        self.asset.ship.waypoint_counts = Some([graph.points.len(), graph.links.len()]);
        self.asset.ship.waypoint_missing_links = graph.missing_links;
        if graph.missing_links > 0 {
            eprintln!(
                "Original waypoint file is missing {} trailing links; use only recorded edges",
                graph.missing_links
            );
        }
        self.asset.ship.crew_source = Some(path.display().to_string());
        let animations = find_child(resource, "animation")?;
        let animation =
            Animation::read(&read_asset_bytes(&find_child(&animations, "Lo_Man.an")?)?)?;
        let clips =
            String::from_utf8_lossy(&read_asset_bytes(&find_child(&animations, "Lo_Man.ani")?)?)
                .into_owned();
        let walk = Clip::from_ini(&clips, "walk", animation.frames)?;
        let skeleton = Arc::new(Skeleton::new(animation, walk));
        let directory = find_child(models, "Lowcharacters")?;
        let names = [
            "Lo_man_1.gm",
            "Lo_man_2.gm",
            "Lo_man_3.gm",
            "Lo_man_kamzol_1.gm",
            "Lo_man_kamzol_2.gm",
            "Lo_man_kamzol_3.gm",
        ];
        let mut variants = Vec::new();
        for name in names.into_iter().take(count.min(6)) {
            let geometry = Geometry::read(&read_asset_bytes(&find_child(&directory, name)?)?)?;
            let mut meshes = Vec::new();
            for o in &geometry.objects {
                if o.flags & 1 == 0 || o.triangle_count == 0 {
                    continue;
                }
                let source = geometry.buffers[o.buffer]
                    [o.first_vertex..o.first_vertex + o.vertex_count]
                    .to_vec();
                ensure!(
                    source.iter().all(|v| v.skin.is_some_and(|s| s
                        .bones
                        .into_iter()
                        .all(|i| (i as usize) < skeleton.animation.parents.len()))),
                    "crew skin references an invalid bone"
                );
                let indices = geometry.triangles
                    [o.first_triangle..o.first_triangle + o.triangle_count]
                    .iter()
                    .flatten()
                    .map(|&i| u32::from(i))
                    .collect::<Vec<_>>();
                let texture = geometry.materials[o.material]
                    .texture
                    .map(|i| self.texture(&geometry.textures[i]))
                    .transpose()?
                    .unwrap_or(0);
                meshes.push((Arc::new(source), indices, texture));
            }
            ensure!(!meshes.is_empty(), "empty crew model");
            variants.push(meshes);
        }
        for id in 0..count {
            let route = Arc::new(Patrol::from_graph(&graph, id)?);
            for (source, indices, texture) in &variants[id % variants.len()] {
                let patch = CrewPatch {
                    skeleton: skeleton.clone(),
                    source: source.clone(),
                    route: route.clone(),
                    phase: id as f32 * 2.173,
                };
                self.add_dynamic(DynamicMesh::Crew(patch), indices.clone(), *texture)?;
            }
        }
        self.asset.ship.crew_count = count;
        Ok(())
    }
    pub(super) fn add_dynamic(
        &mut self,
        dynamic: DynamicMesh,
        indices: Vec<u32>,
        texture: usize,
    ) -> Result<()> {
        self.reserve(
            dynamic.vertex_count() * std::mem::size_of::<ModelVertex>() + indices.len() * 4,
        )?;
        let mut vertices = vec![bytemuck::Zeroable::zeroed(); dynamic.vertex_count()];
        dynamic.deform(
            &mut vertices,
            0.0,
            SailWind::default(),
            Vec3::new(20.0, 15.0, 30.0),
            &self.asset.ship,
            &self.asset.ship_state,
        )?;
        self.asset.meshes.push(Mesh {
            vertices,
            indices,
            texture,
            sail: None,
            rigging: None,
            ship_node: None,
            dynamic: Some(dynamic),
        });
        Ok(())
    }
    fn add_xps_fire_effects(&mut self, additional: &[String]) -> Result<bool> {
        let Some(files) = &self.files else {
            return Ok(false);
        };
        let resource = files.root.parent().context("missing resource root")?;
        let has_particles = std::fs::read_dir(resource)?
            .collect::<std::io::Result<Vec<_>>>()?
            .iter()
            .any(|e| {
                e.file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("Particles")
            });
        if !has_particles {
            ensure!(
                additional.is_empty(),
                "requested XPS profiles require RESOURCE/Particles"
            );
            return Ok(false);
        }
        let directory = find_child(resource, "Particles")?;
        let points = Arc::new(self.asset.ship.fires.clone());
        let texture = self.texture("particles_list.tga")?;
        ensure!(
            additional.len() <= 30,
            "particle cache exceeds 32 selected profiles"
        );
        let mut selected = BTreeSet::new();
        for (name, smoke, preview) in [
            ("Ship_Fire.xps", false, true),
            ("Ship_Smoke.xps", true, true),
        ]
        .into_iter()
        .chain(additional.iter().map(|name| (name.as_str(), false, false)))
        {
            let key = storm_formats::particles::profile_key(name)?;
            if !selected.insert(key.clone()) {
                continue;
            }
            let mut path = directory.clone();
            for part in key.split('/') {
                path = find_child(&path, part)?;
            }
            let profile = storm_formats::particles::Profile::read(&read_asset_bytes(&path)?)?;
            for emitter in &profile.emitters {
                for particle in &emitter.particles {
                    ensure!(
                        particle.kind == 1,
                        "XPS ship preview does not support model particles"
                    );
                    let mut style = crate::model::ParticleStyle::read(
                        if preview { name.into() } else { key.clone() },
                        &emitter.fields,
                        &particle.fields,
                    )?;
                    style.preview = preview;
                    let style = Arc::new(style);
                    let lifetime = style.lifetime.maximum();
                    self.asset.ship.fire_tail_seconds =
                        self.asset.ship.fire_tail_seconds.max(lifetime);
                    let patch = ParticlePatch {
                        points: points.clone(),
                        smoke,
                        per_point: style.count,
                        lifetime,
                        style: Some(style),
                    };
                    let indices = patch.indices();
                    self.add_dynamic(DynamicMesh::Particles(patch), indices, texture)?;
                }
            }
        }
        self.asset.ship.particle_profiles = Some(selected);
        Ok(true)
    }
    fn add_fire_effects(&mut self, additional: &[String]) -> Result<()> {
        ensure!(
            self.files.is_some() || additional.is_empty(),
            "requested XPS profiles require original resources"
        );
        if self.asset.ship.fires.is_empty() {
            ensure!(
                additional.is_empty(),
                "requested particle profiles require fireplace locators"
            );
            return Ok(());
        }
        if self.add_xps_fire_effects(additional)? {
            return Ok(());
        }
        let mut fire = "fire3.tga".to_owned();
        let mut smoke = "smoke9.tga".to_owned();
        if let Some(files) = &self.files {
            let resource = files.root.parent().context("missing resource root")?;
            if let Ok(path) = find_child(resource, "INI")
                .and_then(|p| find_child(&p, "particles"))
                .and_then(|p| find_child(&p, "ship_fire.ini"))
            {
                let bytes = read_asset_bytes(&path)?;
                let text = String::from_utf8_lossy(&bytes);
                let mut section = "";
                for line in text.lines() {
                    let line = line.split(';').next().unwrap_or("").trim();
                    if line.starts_with('[') && line.ends_with(']') {
                        section = &line[1..line.len() - 1];
                    }
                    if let Some((k, v)) = line.split_once('=')
                        && k.trim().eq_ignore_ascii_case("texture")
                    {
                        match section {
                            "ship_fire" => fire = v.trim().into(),
                            "ship_smoke" => smoke = v.trim().into(),
                            _ => {}
                        }
                    }
                }
            }
        }
        let points = Arc::new(self.asset.ship.fires.clone());
        for (name, smoke, per_point, lifetime) in [(fire, false, 24, 2.4), (smoke, true, 20, 7.0)] {
            let texture = if self.files.is_some() {
                self.texture(&name)?
            } else {
                self.radial_texture(!smoke)?
            };
            let patch = ParticlePatch {
                points: points.clone(),
                smoke,
                per_point,
                lifetime,
                style: None,
            };
            let indices = patch.indices();
            self.add_dynamic(DynamicMesh::Particles(patch), indices, texture)?;
        }
        Ok(())
    }
    fn radial_texture(&mut self, fire: bool) -> Result<usize> {
        let mut rgba = Vec::new();
        for y in 0..32 {
            for x in 0..32 {
                let distance =
                    ((x as f32 - 15.5).powi(2) + (y as f32 - 15.5).powi(2)).sqrt() / 15.5;
                rgba.extend(if fire {
                    [
                        255,
                        120,
                        20,
                        ((1.0 - distance).max(0.0).powi(2) * 255.0) as u8,
                    ]
                } else {
                    [230, 230, 230, ((1.0 - distance).max(0.0) * 255.0) as u8]
                });
            }
        }
        self.reserve(rgba.len())?;
        let id = self.asset.textures.len();
        self.asset.textures.push(Texture {
            header: TxHeader {
                flags: 0,
                width: 32,
                height: 32,
                mips: 1,
                format: 21,
                mip0_bytes: 32 * 32 * 4,
            },
            mips: vec![Mip {
                width: 32,
                height: 32,
                rgba,
            }],
        });
        Ok(id)
    }
}
