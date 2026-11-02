# Script-created static models in the shared engine

The shared runtime registers the original `MODELR` class and a bounded static
geometry path. A Storm script creates models, sends original messages and adds
them to Realize layers. These requests reach actual GPU buffers through the same
`storm-engine` host. Complete MODELR/NODER, animation, lighting/techniques,
collision and original game rendering remain pending.

## Supported native boundary

| Message | Supported behavior |
| --- | --- |
| 20500, MSG_MODEL_LOAD_GEO, `ls` | Resolve `resource/models/<name>.gm`, append the native extension, load the static hierarchy and return 1. |
| 20506, MSG_MODEL_SET_POSITION, `lffffffffffff` | Read position, Vx, Vy and Vz in that order; retain complete affine CMatrix bytes. |
| 20509, MSG_MODEL_SET_DIRPATH, `ls` | Change GEOMETRY's shared literal texture prefix for subsequent loads across instances. Loaded assets remain unchanged. |

References: `src/libs/model/src/model.cpp` (class/messages), `node.cpp`
(hierarchy) and `src/libs/geometry/src/geometry.cpp` (paths). The static loader
reuses GM/TX readers and generates no preview sails, ropes, flags, crew or fire.
Those belong to their own native services. Texture lookup concatenates prefix
and material name without ship-specific/basename fallbacks. Shadow/lighting
texture paths and unsupported messages fail explicitly.

Checked load failures fault the script call and preserve its preceding loaded
asset. The native failed-load branch instead returns zero and erases the model;
that outcome remains pending. Successful replacements release owned assets
safely. Animated GM, native lights/COL, full material/technique behavior,
release/restore, action messages and complete node mutation remain unfinished.

## Scheduling, rendering and ownership

Models keep independent transforms and revisions. Realize records immutable draw
snapshots in native layer/priority order; its service begins each draw queue.
Checked stage errors propagate to the frame owner. Deferred destruction removes
registry/asset ownership, while a prepared snapshot may keep an asset through
the current frame.

GPU scenes follow entity ID, revision and actual asset ownership, including fresh
engine sessions reusing IDs/revisions. Pruning releases old scenes; submitted
wgpu commands retain needed resources. The initial cache supports 16 distinct
draw owners and checks duplicates. General per-draw uniforms, native GEOMETRY
lookup/reference counts and complete resource sharing remain pending. Inactive
entries may be evicted and rebuilt.

Native roots are conjugated by the existing GM Z reflection and use
inverse-transpose normal directions. Previous preview paths retain their math.
Nonfinite/noninvertible matrices are checked errors. Static runtime draws clear
black and insert no preview ocean. The bounds-based orbit camera is an explicit
viewer adapter; original scripted cameras, sky/sea and lighting remain pending.

## Executable path

Newly written Storm source can use the original API:

```c
object model;
void Main()
{
    CreateEntity(&model, "MODELR");
    SendMessage(model, "ls", 20500, "cannon");
    LayerAddObject(1, model, 0);
}
```

Place source in the configured PROGRAM and GM/TX below RESOURCE, then run:

```sh
cargo run -p storm-engine --locked -- --game-dir /path/to/game \
  --script-encoding utf8 --frame-ms 16 --headless model.png
```

`--backend metal`, `vulkan` or `dx12` requires an available backend; `auto`
selects native backends. Native window and production sea/camera, controls/audio
and complete original program startup remain future work.

## Verification boundary

Tests extend the licensed upstream cannon GM with a child and incomplete cloth
locator, generating texture pixels/source. Static loading retains both geometry
nodes and ignores preview cloth; preview loading still diagnoses its incomplete
sail. Scripts change a global texture path/reload one owner, set a full matrix,
freeze a layer and delete an owner. Unsupported requests, failed replacement
and stage faults are checked.

GPU readback verifies real color/pose changes, exact same-device restoration,
invalid-request preservation, final empty rendering and fresh-session cache
identity. These are source-derived contract/integration tests, not a new full
native MODELR/NODER differential oracle or complete lighting comparison.
Resources and local captures remain outside Git. Full-engine coverage remains
about 20% (roughly 15–25%).
