# Camera-driven sail detail

`storm_sim::sail_lod` retains the complete GetGrid arithmetic: transform the
local sphere center through pMatWorld, retain rc, subtract the camera and compute
`sqrtf(dx*dx + dy*dy + dz*dz) * tanf(perspective * .5f)`. Original farGrid
thresholds and strict comparisons select the grid. The result reports whether
rows/columns changed. Rolled 5x13 takes precedence over free 5x4; both bypass
camera inputs and retain rc. Negative/zero finite perspective values retain
native numeric behavior. Non-finite attached inputs/results reject atomically.

## Viewer integration

`--native-sail-lod` requires native rolls and works with rope/free diagnostics.
The actual GPU camera supplies its eye and 45-degree vertical field of view.
An opt-in pinned orbit calculation is shared by CPU and GPU; preceding modes
retain their camera arithmetic. The matrix provider composes the stored sail
frame, yard/fall delta and displayed root pose in column order. Original camera
entities and full native matrix accumulation remain pending.

The fleet keeps its canonical near-grid simulation. Before each attached wave
step it saves immutable state/vertices and elapsed milliseconds. Presentation
replays that same step through GoVWave/GoTWave on the camera-selected grid and
publishes the resulting vertices and FillIndex ranges. It replaces neither
canonical SailPnt nor canonical rope outputs. Rolled/free presentation uses the
already evaluated fixed-grid frame. Initial presentation before a wave step can
use the initialized full vertex buffer; ordinary CLI/window setup evaluates
frame zero first.

This separation lets zoom/orbit and fractional presentation work on pause
without advancing physics, phases, flex or rope ownership. Returning to the same
grid/physics frame reproduces identical local vertices. A grid/frame cache
avoids repeated replay. Invisible released owners retain their presentation
state; live owners update independently. Normals/all UVs and unmodified vertices
retain their preceding values. Persistent GPU index/material ranges restore.

This is a presentation adapter around the original numeric methods. Native
SAIL::Execute chooses GetGrid before wind/turning; that complete service order,
camera-dependent simulation samples and registration lifetimes are not supplied
by this viewer. Canonical simulation remains independent of camera movement.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --native-sail-rolls --native-sail-ropes --native-sail-lod \
  --sail-lod-distance 800 --time 1.5 --fleet-window --paused
~~~

Scroll/drag changes the paused presentation; W/S, pause and restart retain their
existing roles. `--sail-lod-distance` optionally overrides initial camera distance
in world units (0.1..1,000,000). Without it the usual fleet camera is used. Replace
window/paused arguments with `--particle-fleet-report sail-lod-state.json` for a
GPU-free report; add `--headless sail-lod.png` for a frame. Headless reporting and
rendering use the same camera. Original resources remain external through
`--model PATH --game-dir PATH`.

`native_sail_lod` retains canonical cloth fields and adds `presentation_lod`:
camera/matrix bits, projected distance, retained world center, selected grid,
change count, source frame, sampled vertices/points/ropes/box/phase/flex and
complete index allocation/ranges. Prior modes and their reports are unchanged.

## Reference and checks

`reference/sail_lod/generate.py` compiles unedited complete GetGrid, farGrid,
CVECTOR and CMatrix point multiplication under UBSan with contraction disabled.
96 profiles / 1,536 stateful snapshots compare float bits for camera distance,
rc retention, grids and changed flags. Identity, rotated, reflected, scaled,
sheared, degenerate and non-affine input matrices, varied perspectives and
rolling/free transitions are represented. Input matrices and camera construction
are explicit ports. The preceding 108 threshold-boundary selection snapshots
and 525 full index profiles remain in the suite.

Integration checks near/far/near replay, repeated paused presentation, unchanged
motion/rope state, all three sets, free/rolled priority, partitioned stepping and
independent owner release. A renderer check uses exactly the GPU eye/FOV. The GPU
test switches near/coarse indices and restores both native and ordinary buffers.
Commercial assets, captures and local audit data remain outside Git.

Native RNG/INI/weather, limit points, heraldry, sea, mast physics/group migration,
complete rope service and full entity/camera scheduling remain separate work.


## Four-platform evidence, 2026-11-01

Code `46764b02844f430d07448b4d18b82bc52787aad1` passes the [PR run](https://github.com/STG-44/storm-engine/actions/runs/41292490310)
on Linux, Windows, macOS Apple Silicon and macOS Intel: formatting,
warnings-denied Clippy, builds, 385 CPU tests (135 interpreter) and 31 scenarios.
Four ZIP digests and all 124 complete JSON reports match local output, including
camera/matrix/distance bits, selected grids, sampled vertices/state and complete
index allocations/ranges. All preceding 30 reports remain unchanged. Actual
host triples and counts are checked from logs.

Linux passes 23 software Vulkan tests and fifteen fleet-window frames, including
three paused camera-detail frames. Local Metal passes 23 render tests and three
paused new-mode window frames. Seven generated/original captures cover close /
far, normal/furled and owner release; original close/far frames were visually
inspected. Three-frame presentation does not establish manual scroll/keyboard
testing. Hosted Windows/macOS physical GPU/window coverage remains open.

All 40 original roots / 368 sails pass camera-selected live-owner grids and
normal/UV retention, while hidden owners remain unselected. Canonical geometry,
rope feedback, parameters, metadata and motion/owner routing also pass. Across
274 fire locators the survivor retains 548 systems / 822 finite world rows /
12,243 living particles. 271 sails retain rotation records and 714 tied slots;
no held turns. Boat has zero sail drive; rowing remains pending. The complete
GetGrid oracle regenerates byte-identically: 96 profiles / 1,536 snapshots.

Pause/partition tests preserve canonical motion, phase/flex and rope points;
near/far/near replay reproduces identical local vertices and persistent buffers.
This validates the numeric methods and presentation adapter. Canonical near-grid
simulation remains independent of camera movement; native Execute/camera/entity
integration is still pending. Commercial resources, captures and local evidence
remain outside Git; earlier whole-profile and manual damage audits were not repeated.
