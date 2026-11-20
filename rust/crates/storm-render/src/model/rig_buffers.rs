//! Wgpu storage for retained rigging buffer intents. Geometry fills are explicit.
use crate::Gpu;
use anyhow::{Result, ensure};
use std::collections::BTreeMap;
use storm_sim::rig_compaction::{BufferEvent, Layout};
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum BufferKind {
    Vertex,
    Index,
}
#[derive(Clone, Debug)]
pub struct Write {
    pub kind: BufferKind,
    pub id: i32,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
/// Native geometry/texture hooks. The producer owns the vertex/index format.
pub trait GeometryFill {
    fn fill(&mut self, event: &BufferEvent, layout: &Layout) -> Result<Vec<Write>>;
}
struct Allocation {
    buffer: wgpu::Buffer,
    logical: u64,
    allocated: u64,
}
#[derive(Default)]
pub struct RigBuffers {
    allocations: BTreeMap<(BufferKind, i32), Allocation>,
}
enum Operation {
    Release(BufferKind, i32),
    Create(BufferKind, i32, u64),
    Write(Write),
}
fn aligned(bytes: u64) -> Result<u64> {
    bytes
        .checked_add(3)
        .map(|n| (n / 4 * 4).max(4))
        .ok_or_else(|| anyhow::anyhow!("rigging buffer size overflow"))
}
impl RigBuffers {
    pub fn count(&self) -> usize {
        self.allocations.len()
    }
    pub fn logical_bytes(&self) -> u64 {
        self.allocations.values().map(|v| v.logical).sum()
    }
    pub fn allocated_bytes(&self) -> u64 {
        self.allocations.values().map(|v| v.allocated).sum()
    }
    pub fn buffer(&self, kind: BufferKind, id: i32) -> Option<&wgpu::Buffer> {
        self.allocations.get(&(kind, id)).map(|a| &a.buffer)
    }
    fn allocation(gpu: &Gpu, kind: BufferKind, logical: u64) -> Result<Allocation> {
        let allocated = aligned(logical)?;
        ensure!(
            allocated <= gpu.device.limits().max_buffer_size,
            "rigging GPU allocation budget"
        );
        let usage = match kind {
            BufferKind::Vertex => wgpu::BufferUsages::VERTEX,
            BufferKind::Index => wgpu::BufferUsages::INDEX,
        };
        let buffer = gpu.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Retained rigging buffer"),
            size: allocated,
            usage: usage | wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Ok(Allocation {
            buffer,
            logical,
            allocated,
        })
    }
    /// Imports an existing handle; physical storage includes transfer padding.
    pub fn seed(&mut self, gpu: &Gpu, kind: BufferKind, id: i32, bytes: &[u8]) -> Result<()> {
        ensure!(
            id >= 0 && !self.allocations.contains_key(&(kind, id)),
            "duplicate rigging buffer handle"
        );
        let allocation = Self::allocation(gpu, kind, bytes.len() as u64)?;
        if !bytes.is_empty() {
            let mut padded = bytes.to_vec();
            padded.resize(aligned(bytes.len() as u64)? as usize, 0);
            gpu.queue.write_buffer(&allocation.buffer, 0, &padded);
        }
        self.allocations.insert((kind, id), allocation);
        Ok(())
    }
    pub fn apply(
        &mut self,
        gpu: &Gpu,
        layout: &Layout,
        events: &[BufferEvent],
        fill: &mut impl GeometryFill,
    ) -> Result<()> {
        // Validate all input and fills before touching GPU storage. Device loss
        // and allocation failure are outside this checked metadata transaction.
        let mut sizes = self
            .allocations
            .iter()
            .map(|(k, v)| (*k, v.logical))
            .collect::<BTreeMap<_, _>>();
        let mut plan = Vec::new();
        for event in events {
            let resource = match *event {
                BufferEvent::ReleaseVertex(id) => Some((BufferKind::Vertex, id, None)),
                BufferEvent::ReleaseIndex(id) => Some((BufferKind::Index, id, None)),
                BufferEvent::CreateVertex { id, bytes, .. } => {
                    Some((BufferKind::Vertex, id, Some(u64::from(bytes))))
                }
                BufferEvent::CreateIndex { id, bytes } => {
                    Some((BufferKind::Index, id, Some(u64::from(bytes))))
                }
                _ => None,
            };
            if let Some((kind, id, bytes)) = resource {
                ensure!(id >= 0, "invalid rigging handle");
                if let Some(bytes) = bytes {
                    ensure!(
                        !sizes.contains_key(&(kind, id)),
                        "reused live rigging handle"
                    );
                    ensure!(
                        aligned(bytes)? <= gpu.device.limits().max_buffer_size,
                        "rigging GPU allocation budget"
                    );
                    sizes.insert((kind, id), bytes);
                    plan.push(Operation::Create(kind, id, bytes));
                } else {
                    ensure!(
                        sizes.remove(&(kind, id)).is_some(),
                        "release of unknown rigging buffer"
                    );
                    plan.push(Operation::Release(kind, id));
                }
            }
            if let BufferEvent::Lock(id) | BufferEvent::Unlock(id) = *event {
                ensure!(
                    sizes.contains_key(&(BufferKind::Vertex, id)),
                    "lock of unknown rigging buffer"
                );
            }
            if matches!(
                event,
                BufferEvent::FillSail { .. }
                    | BufferEvent::TextureSail { .. }
                    | BufferEvent::SetVertices
                    | BufferEvent::SetIndices
                    | BufferEvent::SetTriangles
                    | BufferEvent::SetTexture
            ) {
                for write in fill.fill(event, layout)? {
                    let size = *sizes
                        .get(&(write.kind, write.id))
                        .ok_or_else(|| anyhow::anyhow!("fill of unknown rigging buffer"))?;
                    ensure!(
                        write.offset.is_multiple_of(4)
                            && write
                                .offset
                                .checked_add(write.bytes.len() as u64)
                                .is_some_and(|n| n <= size),
                        "rigging fill range"
                    );
                    ensure!(
                        write.bytes.len().is_multiple_of(4)
                            || write.offset + write.bytes.len() as u64 == size,
                        "unaligned interior rigging fill"
                    );
                    plan.push(Operation::Write(write));
                }
            }
        }
        for op in plan {
            match op {
                Operation::Release(kind, id) => {
                    self.allocations.remove(&(kind, id));
                }
                Operation::Create(kind, id, bytes) => {
                    self.allocations
                        .insert((kind, id), Self::allocation(gpu, kind, bytes)?);
                }
                Operation::Write(write) => {
                    if write.bytes.is_empty() {
                        continue;
                    }
                    let allocation = &self.allocations[&(write.kind, write.id)];
                    let mut bytes = write.bytes;
                    let padded = aligned(bytes.len() as u64)? as usize;
                    bytes.resize(padded, 0);
                    gpu.queue
                        .write_buffer(&allocation.buffer, write.offset, &bytes);
                }
            }
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests;
