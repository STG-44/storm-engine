//! On-demand billboard meshes reuse the already selected project atlas.
use super::*;
use std::{collections::BTreeSet, sync::Arc};

fn keys<'a>(meshes: impl Iterator<Item = &'a DynamicMesh>) -> BTreeSet<String> {
    meshes
        .filter_map(|d| match d {
            DynamicMesh::Particles(p) => p
                .style
                .as_ref()
                .and_then(|s| particle_profile_key(&s.source).ok()),
            _ => None,
        })
        .collect()
}
fn pending(
    ship: &ShipAsset,
    state: &ShipState,
    mut known: BTreeSet<String>,
) -> Result<Vec<ParticlePatch>> {
    let Some(library) = &ship.particle_library else {
        return Ok(Vec::new());
    };
    let mut patches = Vec::new();
    let points = Arc::new(ship.fires.clone());
    for e in state.native_fire.iter().flatten() {
        let Some(key) = &e.profile else {
            continue;
        };
        if !known.insert(key.clone()) {
            continue;
        }
        let definition = library
            .definition(key)?
            .ok_or_else(|| anyhow::anyhow!("particle profile not in project"))?;
        for style in &definition.styles {
            patches.push(ParticlePatch {
                points: points.clone(),
                smoke: e.smoke,
                per_point: style.count,
                lifetime: style.lifetime.maximum(),
                style: Some(Arc::new(style.clone())),
            });
        }
    }
    Ok(patches)
}
impl ModelAsset {
    /// Add requested profiles before CPU reports; the shared library never shares replay state.
    pub fn sync_particle_profiles(&mut self) -> Result<()> {
        let patches = pending(
            &self.ship,
            &self.ship_state,
            keys(self.meshes.iter().filter_map(|m| m.dynamic.as_ref())),
        )?;
        if patches.is_empty() {
            return Ok(());
        }
        let texture = self
            .ship
            .particle_texture
            .ok_or_else(|| anyhow::anyhow!("missing project atlas binding"))?;
        ensure!(
            texture < self.textures.len() && self.meshes.len() + patches.len() <= 4096,
            "particle draw budget or atlas binding invalid"
        );
        for patch in patches {
            self.meshes.push(Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
                texture,
                dynamic: Some(DynamicMesh::Particles(patch)),
                sail: None,
                rigging: None,
                ship_node: None,
            });
        }
        Ok(())
    }
}
impl ModelScene {
    pub(super) fn sync_particle_profiles(&mut self, gpu: &Gpu) -> Result<()> {
        let patches = pending(
            &self.ship,
            &self.ship_state,
            keys(
                self.meshes
                    .iter()
                    .filter_map(|m| m.dynamic.as_ref().map(|(d, _)| d)),
            ),
        )?;
        if patches.is_empty() {
            return Ok(());
        }
        let texture = self
            .ship
            .particle_texture
            .ok_or_else(|| anyhow::anyhow!("missing project atlas binding"))?;
        ensure!(
            texture < self.textures.len() && self.meshes.len() + patches.len() <= 4096,
            "particle draw budget or atlas binding invalid"
        );
        for patch in patches {
            self.meshes.push(DrawMesh {
                damage_mask: 0,
                normal_count: 0,
                ship_node: None,
                dynamic: Some((DynamicMesh::Particles(patch), Vec::new())),
                vertices: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Lazy particle vertices"),
                    size: 4,
                    usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                indices: gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Lazy particle indices"),
                    size: 4,
                    usage: wgpu::BufferUsages::INDEX,
                    mapped_at_creation: false,
                }),
                count: 0,
                texture,
                sail: None,
                rigging: None,
            });
        }
        self.pose_bytes
            .resize(self.pose_stride as usize * (self.meshes.len() + 1), 0);
        self.pose_buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Expanded model poses"),
            size: self.pose_bytes.len() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.camera_group = model_camera_group(
            &gpu.device,
            &self.camera_layout,
            &self.camera_buffer,
            &self.pose_buffer,
            &self.wave_buffer,
        );
        Ok(())
    }
}
