use super::*;
use std::{sync::mpsc, time::Duration};
use storm_sim::rig_compaction::{Buffers, Group, Item, Kind};
struct Fill {
    bad: bool,
}
impl GeometryFill for Fill {
    fn fill(&mut self, event: &BufferEvent, layout: &Layout) -> Result<Vec<Write>> {
        let vertex = |bytes| Write {
            kind: BufferKind::Vertex,
            id: layout.buffers.vertex,
            offset: 0,
            bytes,
        };
        let index = |bytes| Write {
            kind: BufferKind::Index,
            id: layout.buffers.index,
            offset: 0,
            bytes,
        };
        Ok(match *event {
            BufferEvent::FillSail { id, .. } => {
                vec![vertex(vec![id as u8; layout.vertices as usize * 48])]
            }
            BufferEvent::TextureSail { id, .. } => {
                vec![vertex(vec![0x80 + id as u8; layout.vertices as usize * 48])]
            }
            BufferEvent::SetVertices | BufferEvent::SetTexture => {
                vec![vertex(vec![
                    2;
                    layout.vertices as usize * 20
                        + usize::from(self.bad) * 4
                ])]
            }
            BufferEvent::SetIndices | BufferEvent::SetTriangles => {
                vec![index(bytemuck::cast_slice(&[0u16, 1, 2]).to_vec())]
            }
            _ => Vec::new(),
        })
    }
}
fn read(gpu: &Gpu, buffer: &wgpu::Buffer, bytes: usize) -> Vec<u8> {
    let size = aligned(bytes as u64).unwrap();
    let staging = gpu.device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    encoder.copy_buffer_to_buffer(buffer, 0, &staging, 0, size);
    gpu.queue.submit([encoder.finish()]);
    let (tx, rx) = mpsc::channel();
    staging
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    gpu.device
        .poll(wgpu::PollType::Wait {
            timeout: Some(Duration::from_secs(5)),
            submission_index: None,
        })
        .unwrap();
    rx.recv_timeout(Duration::from_secs(5)).unwrap().unwrap();
    let data = staging.slice(..).get_mapped_range().unwrap();
    let result = data[..bytes].to_vec();
    drop(data);
    staging.unmap();
    result
}
#[test]
#[ignore = "requires a native GPU or software Vulkan"]
fn native_rigging_reallocation_releases_old_owner_bytes_and_preserves_shared_sail_indices() {
    let instance = crate::create_instance(wgpu::Backends::PRIMARY).unwrap();
    let gpu = pollster::block_on(Gpu::request(&instance, None)).unwrap();
    for kind in [Kind::Sail, Kind::Rope, Kind::Flag, Kind::Vant] {
        let stride = if kind == Kind::Sail { 48 } else { 20 };
        let groups = (0..2)
            .map(|i| Group {
                owner: 100 + i as u64,
                deleted: i == 0,
                members: if kind == Kind::Flag {
                    Vec::new()
                } else {
                    vec![i]
                },
                span: matches!(kind, Kind::Rope | Kind::Vant).then_some([0, 3, 0, 1]),
            })
            .collect();
        let items = (0..2)
            .map(|i| {
                Some(Item {
                    id: i + 1,
                    group: i as usize,
                    deleted: false,
                    vertices: 3,
                    triangles: 1,
                    start_vertex: i * 3,
                    start_index: i * 3,
                })
            })
            .collect();
        let mut layout = Layout {
            kind,
            groups,
            items,
            vertices: 6,
            triangles: (kind != Kind::Sail).then_some(2),
            active: true,
            pending: true,
            last: 2,
            vertices_need_update: false,
            buffers: Buffers {
                vertex: 1,
                index: 2,
                next_id: 100,
                lock_succeeds: true,
                events: Vec::new(),
            },
        };
        let mut arena = RigBuffers::default();
        let initial = vec![1u8; 6 * stride];
        let indices = if kind == Kind::Sail {
            vec![0u16; 60]
        } else {
            vec![0, 1, 2, 3, 4, 5]
        };
        let index_bytes = bytemuck::cast_slice(&indices);
        arena.seed(&gpu, BufferKind::Vertex, 1, &initial).unwrap();
        arena.seed(&gpu, BufferKind::Index, 2, index_bytes).unwrap();
        layout.compact().unwrap();
        let events = layout.buffers.take_events();
        if kind != Kind::Sail {
            assert!(
                arena
                    .apply(&gpu, &layout, &events, &mut Fill { bad: true })
                    .is_err()
            );
            assert_eq!(arena.count(), 2);
            assert_eq!(
                read(
                    &gpu,
                    arena.buffer(BufferKind::Vertex, 1).unwrap(),
                    initial.len()
                ),
                initial
            );
        }
        arena
            .apply(&gpu, &layout, &events, &mut Fill { bad: false })
            .unwrap();
        assert!(arena.buffer(BufferKind::Vertex, 1).is_none());
        let expected = vec![if kind == Kind::Sail { 0x82 } else { 2 }; 3 * stride];
        assert_eq!(
            read(
                &gpu,
                arena
                    .buffer(BufferKind::Vertex, layout.buffers.vertex)
                    .unwrap(),
                expected.len()
            ),
            expected
        );
        if kind == Kind::Sail {
            assert_eq!(
                read(
                    &gpu,
                    arena.buffer(BufferKind::Index, 2).unwrap(),
                    index_bytes.len()
                ),
                index_bytes
            );
        } else {
            assert!(arena.buffer(BufferKind::Index, 2).is_none());
            assert_eq!(
                read(
                    &gpu,
                    arena
                        .buffer(BufferKind::Index, layout.buffers.index)
                        .unwrap(),
                    6
                ),
                bytemuck::cast_slice::<u16, u8>(&[0, 1, 2])
            );
            assert_eq!(arena.allocated_bytes() - arena.logical_bytes(), 2);
        }
        layout.groups[0].deleted = true;
        layout.pending = true;
        layout.compact().unwrap();
        let events = layout.buffers.take_events();
        arena
            .apply(&gpu, &layout, &events, &mut Fill { bad: false })
            .unwrap();
        assert_eq!(arena.count(), usize::from(kind == Kind::Sail));
    }
}
