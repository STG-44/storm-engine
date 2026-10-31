# Original attached sail waves

`storm_sim::sail_wave` retains the complete GoVWave and GoTWave bodies' arithmetic
and their attached path through goWave: wind-table phase advancement, quad
SumWind/WindUp clamps and the box calculation from SailPnt. Input vectors use
CVECTOR's double reciprocal for vector division. Normals and all three UV sets
stay unchanged, as in the original per-frame processors. The original LOD loops
retain skipped vertices and unwritten sample/rope points, including the source's
four-step start increment for the triangular row LOD 3.

The checked interface accepts the existing geometry/rolling transition,
depths, sphere, height, local/global wind, table, phase/flex state, row/column
counts and four rope ties. It returns complete vertex, sample, rope, box and
phase/flex state. Non-finite values, invalid grids/phases and unsafe native-int
conversions fail without modifying either state or vertices. The fully rolled
and free/falling processors are excluded rather than replaced with this path.

## Inspection provider and rendering

`--native-sail-waves` enables an attached full-grid batch diagnostic. It requires
native sail motion, settled full sails (setting 1), and conflicts with static
geometry inspection, sail targets and the interactive fleet window. Ship
movement and unconstrained yard decisions stay active. Local grids enter the
existing persistent buffers before the stored node frame and yard/root deltas.
Positions change; the initial native normals and UV sets remain intact. Only
the first UV set is bound to the current material.

The provider explicitly uses zero phase, zero SumWind, WindUp=false, maximum
SumWind = height * .02, no rope ties, global base .4, default numeric sail
parameters, and 60 pinned sine samples. Local wind comes from the existing
native-drive adapter. Depths retain the fixed-half/no-limit-point provider from
[geometry](SAIL_GEOMETRY.md). This is not native random initialization, INI,
weather, rope registration, matrix accumulation or full SetSail scheduling.

The diagnostic updates once per existing 60 Hz fleet step, including the initial
zero-ms step. Repeated/fractional presentation does not advance it. Releasing
one owner freezes its grid and state without stopping the other. Changing a
hole mask rebuilds geometry before the next wave update while retaining normals
and UVs. Rope outputs are tested in the numeric kernel but are not connected to
the viewer's rope service. The ordinary viewer and preceding report schemas
keep their previous behavior.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --native-sail-waves --fleet-sail-setting 1 --time 1.5 \
  --particle-fleet-report sail-waves-state.json --headless sail-waves.png
~~~

Add `--model PATH --game-dir PATH` for separately supplied original resources.
`native_sail_waves` exports all local vertex fields, sample/rope points,
box, phase/flex state and the exact provider table as float bits.

## Native reference and remaining scope

`reference/sail_wave/generate.py` compiles unedited GoVWave, GoTWave and the whole
goWave dispatcher, plus SetGeometry/FillVertex/SetTexGrid initialization, under
UBSan with contraction disabled and retained vector/vertex/geometry declarations.
Rolled/free stubs abort if selected. Sixty distinct profiles compare 180 complete
frames / 37,740 complete vertices, including normals and all UVs, phase/flex,
sample/rope points and boxes. Coverage includes all six kind values, all five
triangular row and four quad column LODs, holes, ties, active transition deltas,
negative/zero/over-one wind bases and phase wrapping. Table samples, local wind,
initial states and surrounding services are explicit inputs to this oracle.

The renderer currently uses the full grid. Native index LOD selection,
DoSRollSail/DoTRollSail, free/falling cloth, constrained yard turning, native RNG,
weather/INI/limit points, rope outputs and full entity scheduling remain pending.
Commercial resources, captures and local audit evidence stay outside Git.


## Four-platform evidence, 2026-10-31

Code `eb8bfa52502fb6bc7e472e97c9ac32fc29441e38` passes the [PR run](https://github.com/STG-44/storm-engine/actions/runs/41229441431)
on Linux, Windows, macOS Apple Silicon and macOS Intel: formatting,
warnings-denied Clippy, builds, 362 CPU tests (135 interpreter) and 27 scenarios.
Four archive digests and all 108 complete JSON reports are verified against
local output. Every new wave vertex field, phase/flex value, sample/rope point
and box matches. The preceding 26 reports remain unchanged. Host triples and
actual test counts are checked from logs.

Linux passes 19 explicit software Vulkan tests and three existing motion-viewer
frames. Metal passes 19 render tests and four attached-wave captures: generated,
original at two times, and original after owner release. The original frame was
visually inspected. This diagnostic excludes interactive windows; local native
window presentation was not repeated, and hosted macOS/Windows physical GPU and
window coverage remains open.

All 40 original roots / 368 sails pass attached-grid and finite-value checks;
normals and every UV channel exactly retain their prior static-grid values.
Metadata, type/character, motion and independent owner lifetime checks also pass.
At three seconds, 274 fire locators retain 548 systems / 822 finite world rows /
12,243 living particles under the surviving owner. Boat has zero sail drive;
rowing is pending. The native oracle compares 60 profiles / 180 complete frames /
37,740 complete vertices under UBSan; regeneration is byte-identical.

These results cover attached cloth with the explicit providers above. Native
rolled/free processors, index LOD selection, rope bindings/constraints, native
RNG/weather/INI/limit points and full matrix/entity scheduling remain pending.
Resources, captures and local evidence stay outside Git; earlier whole-profile
and manual damage audits were not repeated.
