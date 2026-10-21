//! Locator-built preview ropes/shrouds and wind-driven flags; no tension solver.
use super::{ModelVertex, SailPatch, SailWind};
use anyhow::{Result, ensure};
use glam::{Mat4, Vec3};
use std::collections::BTreeMap;
use storm_formats::geometry::Label;
use storm_sim::{FlagCell, FlagSelection, FlagTexture, RigState};
#[cfg(test)]
mod tests;

const SIDES: usize = 6;
const FLAG_SEGMENTS: usize = 12;

#[derive(Clone, Debug)]
pub struct RiggingConfig {
    pub rope_texture: String,
    pub flag_texture: String,
    pub rope_width: f32,
    pub stay_width: f32,
    pub shroud_width: f32,
    pub segment_length: f32,
    pub shroud_ropes: usize,
    pub flag_columns: u32,
    pub flag_rows: u32,
}
impl Default for RiggingConfig {
    fn default() -> Self {
        Self {
            rope_texture: "rope.tga".into(),
            flag_texture: "flagall.tga".into(),
            rope_width: 0.04,
            stay_width: 0.12,
            shroud_width: 0.08,
            segment_length: 2.0,
            shroud_ropes: 5,
            flag_columns: 4,
            flag_rows: 8,
        }
    }
}
impl RiggingConfig {
    pub fn from_ini(text: &str) -> Result<Self> {
        let mut c = Self::default();
        let mut section = String::new();
        for line in text.lines() {
            let line = line.split(';').next().unwrap_or("").trim();
            if line.starts_with('[') && line.ends_with(']') {
                section = line[1..line.len() - 1].to_ascii_lowercase();
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().to_ascii_lowercase();
            let value = value.trim();
            match (section.as_str(), key.as_str()) {
                ("ropes", "texturename") => c.rope_texture = value.into(),
                ("ropes", "fwidth") => c.rope_width = value.parse()?,
                ("ropes", "fstay_width") => c.stay_width = value.parse()?,
                ("ropes", "fseg_length") => c.segment_length = value.parse()?,
                ("vants", "fwidth") => c.shroud_width = value.parse()?,
                ("vants", "fropequant") => c.shroud_ropes = value.parse()?,
                ("flags", "texturename") => c.flag_texture = value.into(),
                ("flags", "texturecount") => {
                    c.flag_columns = value.parse()?;
                    c.flag_rows = 1;
                }
                ("flags", "texturecountcolumn") => c.flag_columns = value.parse()?,
                ("flags", "texturecountrow") => c.flag_rows = value.parse()?,
                _ => {}
            }
        }
        ensure!(
            [c.rope_width, c.stay_width, c.shroud_width]
                .iter()
                .all(|v| v.is_finite() && (0.001..=1.0).contains(v)),
            "invalid rigging widths"
        );
        ensure!(
            c.segment_length.is_finite() && (0.1..=20.0).contains(&c.segment_length),
            "invalid rope segment length"
        );
        ensure!(
            (2..=16).contains(&c.shroud_ropes)
                && (1..=64).contains(&c.flag_columns)
                && (1..=64).contains(&c.flag_rows),
            "invalid rigging grid"
        );
        Ok(c)
    }
}

#[derive(Clone, Copy, Default, Debug)]
pub struct RiggingStats {
    pub ropes: usize,
    pub shrouds: usize,
    pub flags: usize,
    pub unmatched_ropes: usize,
    pub incomplete_shrouds: usize,
    pub incomplete_flags: usize,
    pub invalid_lengths: usize,
    pub duplicate_points: usize,
}

#[derive(Default)]
pub(super) struct Collector {
    labels: Vec<PlacedLabel>,
}
struct PlacedLabel {
    node: String,
    group: String,
    name: String,
    position: Vec3,
}
impl Collector {
    pub fn add(&mut self, labels: &[Label], world: Mat4, node: &str) {
        self.labels.extend(
            labels
                .iter()
                .filter(|l| {
                    l.name.starts_with("rope")
                        || l.name.starts_with("fal")
                        || l.name.starts_with("stay")
                        || l.name.starts_with("vant")
                        || l.name.starts_with("vanx")
                        || l.name.starts_with("vanz")
                        || l.group.starts_with("flag")
                        || l.group.starts_with("sflag")
                })
                .map(|l| PlacedLabel {
                    node: node.into(),
                    group: l.group.clone(),
                    name: l.name.clone(),
                    position: world
                        .transform_point3(Mat4::from_cols_array(&l.matrix).w_axis.truncate()),
                }),
        );
    }
    pub fn build(
        &self,
        patches: &[SailPatch],
        config: &RiggingConfig,
    ) -> Result<(Vec<RopeLine>, Vec<FlagPatch>, RiggingStats)> {
        ensure!(self.labels.len() <= 50_000, "too many rigging locators");
        let mut ropes = BTreeMap::<(String, u32), [Option<&PlacedLabel>; 2]>::new();
        let mut shrouds = BTreeMap::<(String, u32), [Option<Vec3>; 3]>::new();
        let mut shroud_nodes = BTreeMap::new();
        let mut flags = BTreeMap::<(String, String), [Option<Vec3>; 4]>::new();
        let mut duplicate_points = 0;
        for l in &self.labels {
            for prefix in ["rope", "fal", "stay"] {
                if let Some(tail) = l.name.strip_prefix(prefix) {
                    let Some((&end, id)) = tail.as_bytes().split_first() else {
                        continue;
                    };
                    let Ok(id) = std::str::from_utf8(id)?.parse::<u32>() else {
                        continue;
                    };
                    let index = match end {
                        b'b' => 0,
                        b'e' => 1,
                        _ => continue,
                    };
                    let entry = ropes.entry((prefix.into(), id)).or_insert([None; 2]);
                    duplicate_points += usize::from(entry[index].is_some());
                    entry[index] = Some(l);
                }
            }
            for prefix in ["vant", "vanx", "vanz"] {
                if let Some(tail) = l.name.strip_prefix(prefix) {
                    let Some((&end, id)) = tail.as_bytes().split_last() else {
                        continue;
                    };
                    let Ok(id) = std::str::from_utf8(id)?.parse::<u32>() else {
                        continue;
                    };
                    let index = match end {
                        b'u' => 0,
                        b'l' => 1,
                        b'r' => 2,
                        _ => continue,
                    };
                    let entry = shrouds.entry((prefix.into(), id)).or_insert([None; 3]);
                    duplicate_points += usize::from(entry[index].is_some());
                    entry[index] = Some(l.position);
                    if index == 0 {
                        shroud_nodes.insert((prefix.to_owned(), id), l.node.clone());
                    }
                }
            }
            if (l.group.starts_with("flag") || l.group.starts_with("sflag"))
                && matches!(l.name.as_bytes(), [b'f', b'1'..=b'4'])
            {
                let index = (l.name.as_bytes()[1] - b'1') as usize;
                let entry = flags
                    .entry((l.node.clone(), l.group.clone()))
                    .or_insert([None; 4]);
                duplicate_points += usize::from(entry[index].is_some());
                entry[index] = Some(l.position);
            }
        }
        let mut stats = RiggingStats {
            duplicate_points,
            ..Default::default()
        };
        let mut lines = Vec::new();
        let mut cloth = Vec::new();
        for ((kind, _), ends) in ropes {
            let [Some(a), Some(b)] = ends else {
                stats.unmatched_ropes += 1;
                continue;
            };
            let bind = |l: &PlacedLabel| RopeEnd {
                node: Some(l.node.clone()),
                position: l.position,
                sail: patches
                    .iter()
                    .find(|p| {
                        p.origin
                            .as_ref()
                            .is_some_and(|o| o.node == l.node && o.group == l.group)
                    })
                    .cloned(),
            };
            if let Some(line) = RopeLine::new(
                bind(a),
                bind(b),
                if kind == "stay" {
                    config.stay_width
                } else {
                    config.rope_width
                },
                kind == "rope",
                config.segment_length,
            ) {
                lines.push(line);
                stats.ropes += 1;
            } else {
                stats.invalid_lengths += 1;
            }
        }
        for (key, points) in shrouds {
            let [Some(top), Some(left), Some(right)] = points else {
                stats.incomplete_shrouds += 1;
                continue;
            };
            stats.shrouds += 1;
            for i in 0..config.shroud_ropes {
                let base = left.lerp(right, i as f32 / (config.shroud_ropes - 1) as f32);
                if let Some(mut line) =
                    RopeLine::fixed(base, top, config.shroud_width, config.segment_length)
                {
                    line.cut_node = shroud_nodes.get(&key).cloned();
                    lines.push(line);
                }
            }
            let height = top.distance((left + right) * 0.5);
            for i in 1..(height / 0.4).ceil().clamp(2.0, 256.0) as usize {
                let t = i as f32 * 0.4 / height;
                if t >= 0.96 {
                    break;
                }
                if let Some(mut line) = RopeLine::fixed(
                    left.lerp(top, t),
                    right.lerp(top, t),
                    config.rope_width,
                    config.segment_length,
                ) {
                    line.cut_node = shroud_nodes.get(&key).cloned();
                    lines.push(line);
                }
            }
            ensure!(lines.len() <= 20_000, "too many rigging lines");
        }
        for ((node, group), points) in flags {
            ensure!(cloth.len() < 512, "too many flags");
            if let [Some(a), Some(b), Some(c), d] = points {
                // FLAG::SetAdd attaches quad f1/f4; f2/f3 are the free edge.
                let (bottom, tip, lower) = d.map_or((c, b, None), |d| (d, b, Some(c)));
                match FlagPatch::new(a, bottom, tip, lower, config.flag_columns, config.flag_rows) {
                    Ok(mut flag) => {
                        flag.origin = Some(FlagOrigin {
                            node,
                            special: group.starts_with("sflag"),
                            group,
                        });
                        cloth.push(flag);
                        stats.flags += 1;
                    }
                    Err(_) => stats.incomplete_flags += 1,
                }
            } else {
                stats.incomplete_flags += 1;
            }
        }
        Ok((lines, cloth, stats))
    }
}

#[derive(Clone, Debug)]
pub struct RopeEnd {
    pub node: Option<String>,
    pub position: Vec3,
    pub sail: Option<SailPatch>,
}
impl RopeEnd {
    fn at(&self, rig: RigState) -> Vec3 {
        self.sail.as_ref().map_or(self.position, |s| {
            s.gather_anchor(self.position, rig.deployed(s.tier) as f32)
        })
    }
}
#[derive(Clone, Debug)]
pub struct RopeLine {
    cut_node: Option<String>,
    start: RopeEnd,
    end: RopeEnd,
    radius: f32,
    sag: bool,
    segments: usize,
    length: f32,
}
impl RopeLine {
    fn new(
        start: RopeEnd,
        end: RopeEnd,
        width: f32,
        sag: bool,
        segment_length: f32,
    ) -> Option<Self> {
        let length = start.position.distance(end.position);
        if !length.is_finite() || !(0.1..=200.0).contains(&length) {
            return None;
        }
        Some(Self {
            cut_node: None,
            start,
            end,
            radius: width * 0.5,
            sag,
            segments: (length / segment_length).ceil().clamp(2.0, 128.0) as usize,
            length,
        })
    }
    fn fixed(a: Vec3, b: Vec3, width: f32, segment: f32) -> Option<Self> {
        Self::new(
            RopeEnd {
                node: None,
                position: a,
                sail: None,
            },
            RopeEnd {
                node: None,
                position: b,
                sail: None,
            },
            width,
            false,
            segment,
        )
    }
    fn vertex_count(&self) -> usize {
        (self.segments + 1) * SIDES
    }
    fn deform(&self, out: &mut [ModelVertex], rig: RigState, wind: SailWind, seconds: f32) {
        let start = self.start.at(rig);
        let end = self.end.at(rig);
        let delta = end - start;
        let direction = if delta.length_squared() > 1e-8 {
            delta.normalize()
        } else {
            Vec3::Y
        };
        let mut side = direction.cross(Vec3::Y).normalize_or_zero();
        if side.length_squared() < 0.5 {
            side = Vec3::X;
        }
        let other = direction.cross(side).normalize_or_zero();
        let sag = if self.sag {
            self.length.min(30.0) * 0.015
        } else {
            0.0
        };
        let flutter = if self.sag {
            0.03 * wind.speed / (8.0 + wind.speed)
        } else {
            0.0
        };
        for ring in 0..=self.segments {
            let t = ring as f32 / self.segments as f32;
            let envelope = 4.0 * t * (1.0 - t);
            let center = start + delta * t - Vec3::Y * (sag * envelope)
                + side * (flutter * envelope * (seconds * 1.7 - t * 6.0).sin());
            for s in 0..SIDES {
                let angle = std::f32::consts::TAU * s as f32 / SIDES as f32;
                let normal = side * angle.cos() + other * angle.sin();
                out[ring * SIDES + s] = ModelVertex {
                    position: (center + normal * self.radius).to_array(),
                    normal: normal.to_array(),
                    uv: [s as f32 / SIDES as f32, t * self.length / 0.2],
                    color: [0.55, 0.46, 0.32, 1.0],
                };
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct FlagOrigin {
    pub node: String,
    pub group: String,
    pub special: bool,
}

#[derive(Clone, Debug)]
pub struct FlagPatch {
    pub origin: Option<FlagOrigin>,
    top: Vec3,
    bottom: Vec3,
    tip: Vec3,
    lower: Option<Vec3>,
    columns: u32,
    rows: u32,
    texture: FlagTexture,
    inset: [f32; 2],
}
impl FlagPatch {
    pub fn new(
        top: Vec3,
        bottom: Vec3,
        tip: Vec3,
        lower: Option<Vec3>,
        columns: u32,
        rows: u32,
    ) -> Result<Self> {
        ensure!(
            [top, bottom, tip].iter().all(|p| p.is_finite()) && lower.is_none_or(|p| p.is_finite()),
            "non-finite flag"
        );
        ensure!(
            top.distance(bottom) >= 0.02 && tip.distance(top) >= 0.05 && columns > 0 && rows > 0,
            "degenerate flag"
        );
        Ok(Self {
            origin: None,
            top,
            bottom,
            tip,
            lower,
            columns,
            rows,
            texture: FlagTexture::Cell(FlagCell::default()),
            inset: [0.0; 2],
        })
    }
    pub fn texture(&self) -> FlagTexture {
        self.texture
    }
    pub fn atlas_size(&self) -> [u32; 2] {
        [self.columns, self.rows]
    }
    pub fn is_visible(&self) -> bool {
        self.texture != FlagTexture::Hidden
    }
    pub fn set_texture_size(&mut self, width: u32, height: u32) -> Result<()> {
        ensure!(
            width >= self.columns && height >= self.rows,
            "flag atlas has fewer pixels than cells"
        );
        self.inset = [0.5 / width as f32, 0.5 / height as f32];
        Ok(())
    }
    fn check_texture(&self, texture: FlagTexture) -> Result<()> {
        if let FlagTexture::Cell(cell) = texture {
            ensure!(
                cell.column < self.columns && cell.row < self.rows,
                "flag cell ({}, {}) is outside {}x{} atlas",
                cell.column,
                cell.row,
                self.columns,
                self.rows
            );
        }
        Ok(())
    }
    fn vertex_count(&self) -> usize {
        if self.lower.is_some() {
            2 * (FLAG_SEGMENTS + 1)
        } else {
            2 * FLAG_SEGMENTS + 1
        }
    }
    fn indices(&self) -> Vec<u32> {
        let mut indices = Vec::new();
        for i in 0..FLAG_SEGMENTS {
            let a = (i * 2) as u32;
            indices.extend([a, a + 2, a + 1]);
            if i + 1 < FLAG_SEGMENTS || self.lower.is_some() {
                indices.extend([a + 1, a + 2, a + 3]);
            }
        }
        indices
    }
    fn deform(&self, out: &mut [ModelVertex], wind: SailWind, seconds: f32) {
        let cell = match self.texture {
            FlagTexture::Cell(cell) => cell,
            FlagTexture::Hidden => FlagCell::default(),
        };
        let rest_vector = self
            .lower
            .map_or(self.tip - (self.top + self.bottom) * 0.5, |lower| {
                ((self.tip - self.top) + (lower - self.bottom)) * 0.5
            });
        let rest = rest_vector.normalize_or_zero();
        let air = Vec3::new(wind.direction.sin(), 0.0, wind.direction.cos());
        let strength = wind.speed / (8.0 + wind.speed);
        let flow = (rest * (1.0 - strength) + air * strength).normalize_or_zero();
        let flow = if flow.length_squared() < 0.5 {
            air
        } else {
            flow
        };
        let axis = (self.bottom - self.top).normalize();
        let mut normal = flow.cross(axis).normalize_or_zero();
        if normal.length_squared() < 0.5 {
            normal = Vec3::X;
        }
        let width = rest_vector.length();
        let count = out.len();
        for (i, vertex) in out.iter_mut().enumerate() {
            let tip = self.lower.is_none() && i == count - 1;
            let u = if tip {
                1.0
            } else {
                (i / 2) as f32 / FLAG_SEGMENTS as f32
            };
            let v = if tip { 0.5 } else { (i % 2) as f32 };
            let across = if self.lower.is_some() {
                v
            } else {
                0.5 + (v - 0.5) * (1.0 - u)
            };
            let head = self.top.lerp(self.bottom, across);
            let wave = (seconds * (2.0 + wind.speed * 0.08) - u * 8.0 + v * 1.2).sin();
            let position = head + flow * (width * u)
                - Vec3::Y * (width * 0.2 * (1.0 - strength) * u * u)
                + normal * (width * 0.12 * strength * u * wave);
            *vertex = ModelVertex {
                position: position.to_array(),
                normal: [0.0; 3],
                uv: [
                    cell.column as f32 / self.columns as f32
                        + self.inset[0]
                        + u * (1.0 / self.columns as f32 - 2.0 * self.inset[0]),
                    cell.row as f32 / self.rows as f32
                        + self.inset[1]
                        + across * (1.0 / self.rows as f32 - 2.0 * self.inset[1]),
                ],
                color: [1.0; 4],
            };
        }
        for tri in self.indices().chunks_exact(3) {
            let [a, b, c] =
                [tri[0], tri[1], tri[2]].map(|i| Vec3::from_array(out[i as usize].position));
            let n = (b - a).cross(c - a);
            for &i in tri {
                out[i as usize].normal = (Vec3::from_array(out[i as usize].normal) + n).to_array();
            }
        }
        for v in out {
            v.normal = Vec3::from_array(v.normal).normalize_or_zero().to_array();
        }
    }
}

/// Validate the complete update before changing any flag, including mixed atlases.
pub(super) fn select_flags<'a>(
    meshes: impl Iterator<Item = &'a mut RigMesh>,
    selection: FlagSelection,
) -> Result<()> {
    let patches = meshes
        .filter_map(|m| match m {
            RigMesh::Flag(f) => Some(f),
            _ => None,
        })
        .collect::<Vec<_>>();
    let textures = patches
        .iter()
        .map(|p| {
            let t = selection.texture(p.origin.as_ref().is_some_and(|o| o.special));
            p.check_texture(t)?;
            Ok(t)
        })
        .collect::<Result<Vec<_>>>()?;
    for (patch, texture) in patches.into_iter().zip(textures) {
        patch.texture = texture;
    }
    Ok(())
}

#[derive(Clone, Debug)]
pub enum RigMesh {
    Ropes(Vec<RopeLine>),
    Flag(FlagPatch),
}
impl RigMesh {
    pub(super) fn follow_damage(
        &self,
        out: &mut [ModelVertex],
        asset: &super::ShipAsset,
        state: &super::ShipState,
        seconds: f32,
    ) {
        if let Self::Ropes(lines) = self {
            let mut offset = 0;
            for line in lines {
                let vertices = &mut out[offset..offset + line.vertex_count()];
                offset += line.vertex_count();
                let fallen = |node: &Option<String>| {
                    node.as_deref()
                        .and_then(|n| asset.fallen_mast(n, state))
                        .filter(|(_, t, _)| seconds >= *t)
                        .map(|(id, _, _)| id)
                };
                let a = fallen(&line.start.node);
                let b = fallen(&line.end.node);
                let cut = line.cut_node.as_deref().is_some_and(|n| {
                    asset
                        .fallen_mast(n, state)
                        .is_some_and(|(_, t, _)| seconds >= t)
                }) || [&line.start.node, &line.end.node].into_iter().any(|n| {
                    n.as_deref()
                        .is_some_and(|n| asset.hidden(n, state, seconds))
                });
                if cut || a != b {
                    for v in vertices {
                        v.position = [0.0; 3];
                        v.normal = [0.0, 1.0, 0.0];
                    }
                } else if a.is_some()
                    && let Some(matrix) = line
                        .start
                        .node
                        .as_deref()
                        .and_then(|n| asset.transform(n, state, seconds))
                {
                    for v in vertices {
                        v.position = matrix
                            .transform_point3(Vec3::from_array(v.position))
                            .to_array();
                        v.normal = matrix
                            .transform_vector3(Vec3::from_array(v.normal))
                            .normalize_or_zero()
                            .to_array();
                    }
                }
            }
        }
    }
    pub fn node(&self) -> Option<&str> {
        match self {
            Self::Flag(f) => f.origin.as_ref().map(|o| o.node.as_str()),
            _ => None,
        }
    }
    pub fn index_count(&self) -> usize {
        match self {
            Self::Ropes(lines) => lines.iter().map(|l| l.segments * SIDES * 6).sum(),
            Self::Flag(f) => (FLAG_SEGMENTS * 2 - usize::from(f.lower.is_none())) * 3,
        }
    }
    pub fn vertex_count(&self) -> usize {
        match self {
            Self::Ropes(lines) => lines.iter().map(RopeLine::vertex_count).sum(),
            Self::Flag(f) => f.vertex_count(),
        }
    }
    pub fn indices(&self) -> Vec<u32> {
        match self {
            Self::Flag(f) => f.indices(),
            Self::Ropes(lines) => {
                let mut out = Vec::new();
                let mut offset = 0u32;
                for l in lines {
                    for row in 0..l.segments {
                        for side in 0..SIDES {
                            let a = offset + (row * SIDES + side) as u32;
                            let b = offset + (row * SIDES + (side + 1) % SIDES) as u32;
                            out.extend([
                                a,
                                b,
                                a + SIDES as u32,
                                b,
                                b + SIDES as u32,
                                a + SIDES as u32,
                            ]);
                        }
                    }
                    offset += l.vertex_count() as u32;
                }
                out
            }
        }
    }
    pub fn vertices(
        &self,
        wind: SailWind,
        seconds: f32,
        rig: RigState,
    ) -> Result<Vec<ModelVertex>> {
        ensure!(
            self.vertex_count() <= 2_000_000,
            "rigging vertex budget exceeded"
        );
        let mut out = vec![bytemuck::Zeroable::zeroed(); self.vertex_count()];
        self.deform(&mut out, wind, seconds, rig)?;
        Ok(out)
    }
    pub fn deform(
        &self,
        out: &mut [ModelVertex],
        wind: SailWind,
        seconds: f32,
        rig: RigState,
    ) -> Result<()> {
        wind.validate()?;
        rig.validate()?;
        ensure!(
            seconds.is_finite() && out.len() == self.vertex_count(),
            "invalid rigging frame"
        );
        match self {
            Self::Flag(f) => f.deform(out, wind, seconds),
            Self::Ropes(lines) => {
                let mut offset = 0;
                for line in lines {
                    let count = line.vertex_count();
                    line.deform(&mut out[offset..offset + count], rig, wind, seconds);
                    offset += count;
                }
            }
        }
        ensure!(
            out.iter().all(|v| Vec3::from_array(v.position).is_finite()
                && Vec3::from_array(v.normal).is_finite()),
            "rigging overflow"
        );
        Ok(())
    }
}
