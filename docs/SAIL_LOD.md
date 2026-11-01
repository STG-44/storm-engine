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
