//! Static GM 1.05 render data. Packed records are read without casts or alignment assumptions.
use crate::{FormatError, GmHeader, count, word};

#[derive(Debug, Clone)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub skin: Option<Skin>,
}

#[derive(Debug, Clone, Copy)]
pub struct Skin {
    pub bones: [u8; 2],
    pub weight: f32,
}

#[derive(Debug)]
pub struct Material {
    pub name: String,
    pub diffuse: f32,
    pub texture: Option<usize>,
}

#[derive(Debug)]
pub struct Label {
    pub group: String,
    pub name: String,
    /// Row-vector C++ matrix bytes, equivalent to a column-major Rust matrix.
    pub matrix: [f32; 16],
}

#[derive(Debug)]
pub struct Object {
    pub name: String,
    pub flags: u32,
    pub material: usize,
    pub buffer: usize,
    pub first_vertex: usize,
    pub vertex_count: usize,
    pub first_triangle: usize,
    pub triangle_count: usize,
}

#[derive(Debug)]
pub struct Geometry {
    pub header: GmHeader,
    pub textures: Vec<String>,
    pub materials: Vec<Material>,
    pub labels: Vec<Label>,
    pub objects: Vec<Object>,
    pub buffers: Vec<Vec<Vertex>>,
    /// Indices are local to each object's vertex range, as in DrawIndexedPrimitive.
    pub triangles: Vec<[u16; 3]>,
}

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], FormatError> {
        let end = self
            .pos
            .checked_add(length)
            .ok_or(FormatError("GM offset overflow"))?;
        let data = self
            .bytes
            .get(self.pos..end)
            .ok_or(FormatError("truncated GM data"))?;
        self.pos = end;
        Ok(data)
    }
}

fn floats<const N: usize>(record: &[u8], start: usize) -> Result<[f32; N], FormatError> {
    let values = std::array::from_fn(|i| f32::from_bits(word(record, start + i)));
    if values.iter().any(|v| !v.is_finite()) {
        return Err(FormatError("non-finite GM vertex, material, or transform"));
    }
    Ok(values)
}

fn string(blob: &[u8], offset: u32) -> Result<String, FormatError> {
    if offset == u32::MAX {
        return Ok(String::new());
    }
    let tail = blob
        .get(offset as usize..)
        .ok_or(FormatError("GM name offset is out of bounds"))?;
    let end = tail
        .iter()
        .position(|&b| b == 0)
        .ok_or(FormatError("unterminated GM name"))?;
    let bytes = &tail[..end];
    Ok(match std::str::from_utf8(bytes) {
        Ok(name) => name.to_owned(),
        Err(_) => encoding_rs::WINDOWS_1251.decode(bytes).0.into_owned(),
    })
}

impl Geometry {
    pub fn read(bytes: &[u8]) -> Result<Self, FormatError> {
        let header = GmHeader::read(bytes, bytes.len() as u64)?;
        let mut cursor = Cursor {
            bytes,
            pos: GmHeader::SIZE,
        };
        let names = cursor.take(header.name_bytes as usize)?;
        for offset in cursor.take(header.names as usize * 4)?.chunks_exact(4) {
            string(names, word(offset, 0))?;
        }
        let textures = cursor
            .take(header.textures as usize * 4)?
            .chunks_exact(4)
            .map(|r| string(names, word(r, 0)))
            .collect::<Result<Vec<_>, _>>()?;
        let mut materials = Vec::new();
        for r in cursor
            .take(header.materials as usize * 56)?
            .chunks_exact(56)
        {
            string(names, word(r, 0))?;
            let coefficients = floats::<4>(r, 2)?;
            let mut texture = None;
            for slot in 0..4 {
                let kind = word(r, 6 + slot);
                if kind > 2 {
                    return Err(FormatError("unknown GM material texture type"));
                }
                if kind != 0 {
                    let index = count(r, 10 + slot)? as usize;
                    if index >= textures.len() {
                        return Err(FormatError("GM material texture index out of bounds"));
                    }
                    if kind == 1 && texture.is_none() {
                        texture = Some(index);
                    }
                }
            }
            materials.push(Material {
                name: string(names, word(r, 1))?,
                diffuse: coefficients[0],
                texture,
            });
        }
        cursor.take(header.lights as usize * 80)?;
        let labels = cursor
            .take(header.labels as usize * 108)?
            .chunks_exact(108)
            .map(|r| {
                Ok(Label {
                    group: string(names, word(r, 0))?,
                    name: string(names, word(r, 1))?,
                    matrix: floats(r, 3)?,
                })
            })
            .collect::<Result<Vec<_>, FormatError>>()?;
        let mut objects = Vec::new();
        for r in cursor
            .take(header.objects as usize * 104)?
            .chunks_exact(104)
        {
            string(names, word(r, 0))?;
            floats::<4>(r, 3)?;
            objects.push(Object {
                name: string(names, word(r, 1))?,
                flags: word(r, 2),
                buffer: count(r, 7)? as usize,
                triangle_count: count(r, 8)? as usize,
                first_triangle: count(r, 9)? as usize,
                vertex_count: count(r, 10)? as usize,
                first_vertex: count(r, 11)? as usize,
                material: count(r, 12)? as usize,
            });
        }
        let triangles = cursor
            .take(header.triangles as usize * 6)?
            .chunks_exact(6)
            .map(|r| std::array::from_fn(|i| u16::from_le_bytes([r[i * 2], r[i * 2 + 1]])))
            .collect::<Vec<_>>();
        let descriptors = cursor.take(header.vertex_buffers as usize * 8)?;
        let mut buffers = Vec::new();
        for r in descriptors.chunks_exact(8) {
            let kind = count(r, 0)?;
            if kind > 4 {
                return Err(FormatError(
                    "animated GM vertex buffers are not supported yet",
                ));
            }
            let stride = if kind == 4 {
                44
            } else {
                36 + kind as usize * 8
            };
            let size = count(r, 1)? as usize;
            if !size.is_multiple_of(stride) {
                return Err(FormatError("GM vertex buffer has a partial vertex"));
            }
            let mut vertices = Vec::new();
            for vertex in cursor.take(size)?.chunks_exact(stride) {
                let animated = kind == 4;
                let normal = if animated { 5 } else { 3 };
                let color = word(vertex, if animated { 8 } else { 6 });
                let uv_offset = if animated { 9 } else { 7 };
                let skin = if animated {
                    let weight = floats::<1>(vertex, 3)?[0];
                    if !(0.0..=1.0).contains(&weight) {
                        return Err(FormatError("invalid skin weight"));
                    }
                    let ids = word(vertex, 4);
                    Some(Skin {
                        bones: [ids as u8, (ids >> 8) as u8],
                        weight,
                    })
                } else {
                    None
                };
                for uv in vertex[uv_offset * 4..].chunks_exact(8) {
                    floats::<2>(uv, 0)?;
                }
                vertices.push(Vertex {
                    position: floats(vertex, 0)?,
                    normal: floats(vertex, normal)?,
                    uv: floats(vertex, uv_offset)?,
                    skin,
                    color: [
                        ((color >> 16) & 255) as f32 / 255.0,
                        ((color >> 8) & 255) as f32 / 255.0,
                        (color & 255) as f32 / 255.0,
                        (color >> 24) as f32 / 255.0,
                    ],
                });
            }
            buffers.push(vertices);
        }
        for object in &objects {
            let buffer = buffers
                .get(object.buffer)
                .ok_or(FormatError("GM object buffer index out of bounds"))?;
            if object.material >= materials.len() {
                return Err(FormatError("GM object material index out of bounds"));
            }
            let end = object
                .first_vertex
                .checked_add(object.vertex_count)
                .ok_or(FormatError("GM vertex range overflow"))?;
            if end > buffer.len() {
                return Err(FormatError("GM object vertex range out of bounds"));
            }
            let end = object
                .first_triangle
                .checked_add(object.triangle_count)
                .ok_or(FormatError("GM triangle range overflow"))?;
            let indices = triangles
                .get(object.first_triangle..end)
                .ok_or(FormatError("GM object triangle range out of bounds"))?;
            if indices
                .iter()
                .flatten()
                .any(|&i| i as usize >= object.vertex_count)
            {
                return Err(FormatError(
                    "GM triangle references a vertex outside its object",
                ));
            }
        }
        // Collision is not exposed yet, but its declared byte ranges/indices must fit.
        if header.flags & 2 != 0 {
            let bsp = cursor.take(12)?;
            let nodes = count(bsp, 0)? as usize;
            let vertices = count(bsp, 1)? as usize;
            let faces = count(bsp, 2)? as usize;
            cursor.take(
                nodes
                    .checked_mul(24)
                    .ok_or(FormatError("GM BSP size overflow"))?,
            )?;
            cursor.take(
                vertices
                    .checked_mul(12)
                    .ok_or(FormatError("GM BSP size overflow"))?,
            )?;
            for face in cursor
                .take(
                    faces
                        .checked_mul(9)
                        .ok_or(FormatError("GM BSP size overflow"))?,
                )?
                .chunks_exact(3)
            {
                let index = u32::from_le_bytes([face[0], face[1], face[2], 0]);
                if index as usize >= vertices {
                    return Err(FormatError("GM BSP vertex index out of bounds"));
                }
            }
        }
        Ok(Self {
            header,
            textures,
            materials,
            labels,
            objects,
            buffers,
            triangles,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn cannon() -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../tools/modding-tool/Meshes/cannon_x.gm"),
        )
        .unwrap()
    }
    #[test]
    fn decodes_upstream_mesh_and_rejects_corrupt_references() {
        let bytes = cannon();
        let mesh = Geometry::read(&bytes).unwrap();
        assert_eq!(mesh.triangles.len(), 1728);
        assert!(!mesh.buffers[0].is_empty());
        assert!(!mesh.materials[0].name.is_empty());
        let h = &mesh.header;
        let object = 72
            + h.name_bytes as usize
            + h.names as usize * 4
            + h.textures as usize * 4
            + h.materials as usize * 56
            + h.lights as usize * 80
            + h.labels as usize * 108;
        let mut bad = bytes.clone();
        bad[object + 48..object + 52].copy_from_slice(&i32::MAX.to_le_bytes());
        assert!(Geometry::read(&bad).is_err());
        let mut bad = bytes.clone();
        let triangle = object + h.objects as usize * 104;
        bad[triangle..triangle + 2].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(Geometry::read(&bad).is_err());
        let descriptors = triangle + h.triangles as usize * 6;
        let vertex_end = descriptors
            + h.vertex_buffers as usize * 8
            + bytes[descriptors..descriptors + h.vertex_buffers as usize * 8]
                .chunks_exact(8)
                .map(|r| word(r, 1) as usize)
                .sum::<usize>();
        assert!(Geometry::read(&bytes[..vertex_end - 1]).is_err());
    }
}
