# Rigging compaction and buffer ownership

This library increment retains deletion/layout behavior from
SAIL::DeleteSailGroup and ROPE/FLAG/VANT::DoSTORM_DELETE. It also adds a wgpu
buffer owner that consumes the resulting resource intents. It is not yet wired
to the fleet's group services or native geometry fills. Existing scenes retain
their preceding buffers and all 36 diagnostic modes keep their report shape.

## Retained metadata and resource decisions

The scalar layout records group membership, item identity/owner, deletion flags,
vertex/index offsets and counts. Group tags let the caller retain its other
attributes; complete original GROUPDATA and geometry structures are separate
ports. Surviving records preserve the original scan order and group remapping.

| Service | Retained layout rule | Native buffer action |
| --- | --- | --- |
| SAIL | Rebuild nonempty groups in group order, scanning the old sail list; assign packed sVert and shared sIndx 54. | Always release the vertex buffer; recreate/fill it if vertices remain. The shared index buffer remains owned. |
| ROPE | Remove deleted ropes in list order, then rebuild group indices and packed vertex/index offsets. | Recreate buffers only when total vertex count changes; this condition is retained even when other counts differ. |
| FLAG | Remove deleted/zero-size flags and empty groups; preserve surviving flag-list order and update group/vertex/index positions. | Release both when empty; recreate when item/group counts change, request triangle/texture updates and retain verticesNeedUpdate. |
| VANT | Scan each group's membership order, compact survivors and remap group/item positions. | Release both when empty; otherwise recreate only when item/group counts change. |

Pending flags clear after compaction, last-item counters and bUse follow the
remaining item count. Zero-time Execute may perform pending cleanup. Invalid
owners/membership, repeated item identities, offset/byte overflow and excessive
allocations are checked before metadata commits. These are bounded Rust safety
rules, not support for native invalid-pointer or allocation-failure outcomes.
Native pointer lists and raw array deallocation are represented with owned Vec
storage. The supported reference inputs avoid inconsistent ownership/aliasing.

## Wgpu buffer port

`model::rig_buffers::RigBuffers` owns separate vertex/index handle spaces and
implements release/create/write intents. Native vertex strides remain 48 bytes
for SAIL and 20 for ROPE/FLAG/VANT. Native index sizes remain six bytes per
triangle. Physical GPU allocations/transfers pad a final u16 tail to four-byte
alignment without changing the logical native size or index units.

GeometryFill supplies the FillVertex/SetTexGrid/SetVertexes/SetIndex/triangle /
texture writes. The port validates the whole handle/allocation/fill plan before
changing its GPU storage; invalid fill ranges leave the preceding buffers
available. Device loss and GPU allocation failure are outside that transaction.
Intents are a consumable queue, not an unbounded production event history.

The current GPU probe supplies diagnostic payloads and reads actual GPU buffers
back. It checks each service's survivor bytes, removal of old handles, final
u16 padding, complete-owner removal and retained shared sail indices. Invalid
payloads also preserve the old GPU contents. This is a storage/readback test;
original native geometry fill and drawing through these buffers remain pending.

## Native reference

The oracle compiles all four unedited complete compaction methods and original
release macros under UBSan. Minimal group/item structures retain the fields
used by these methods; pointer tags, renderer allocations, locks, geometry and
texture hooks are explicit probes. Symbolic format/usage constants describe
buffer intents rather than a complete DX9 renderer. There are 256 profiles and
768 stateful before/after snapshots, including empty groups/lists, individual
deletions, null FLAG entries, zero-size flags, repeated cleanup, shared indices
and failed sail-buffer locks. All selected metadata and intents must match.

This extends [ship cleanup](SHIP_CLEANUP.md). Original service startup/scheduling,
full group payloads, geometry fills, live fleet buffer wiring and rendering must
still be integrated. Original Storm scripts remain compatibility inputs; their
language and syntax are preserved while the engine/runtime/API move to Rust.
The whole-engine estimate remains about 20% (roughly 15–25%).


## Four-platform evidence, 2026-11-02

Code `ecdba85c3bc31cfac0c881a3387f1f00956e4a1c` passes [PR run 41373941089](https://github.com/STG-44/storm-engine/actions/runs/41373941089)
on Windows x86_64, Linux x86_64, macOS Apple Silicon and macOS Intel. Full logs
verify formatting, warnings-denied Clippy, native builds, target triples and
413 CPU tests (135 interpreter) on every host. All four original ZIP SHA256
digests are verified and all 144 complete reports over the existing 36 scenarios
match local values without rounding or tolerances. All 36 preceding reports
remain unchanged; no new scene mode is introduced by this library increment.

Linux additionally passes 25 software Vulkan tests and the existing 30 fleet
window frames. Local Metal passes 25 GPU tests. The new allocation test reads
actual GPU buffers back for each service after survivor compaction and final
owner removal, checks native u16 transfer padding and retains the shared sail
index buffer. The diagnostic payload producer is explicit. Native geometry
fills and drawing these buffers through live fleet services remain pending.
The 256-profile / 768-snapshot C++ fixture regenerates byte-identically.

This storage/layout increment does not establish original geometry, service
startup/scheduling, complete Mount or gameplay. Windows/macOS physical GPU
acceptance remains pending. Resources and local reports remain outside Git.
The whole-engine estimate remains about 20% (roughly 15–25%).
