# Original rolled sail cloth and index grids

`storm_sim::sail_roll` retains DoSRollSail and DoTRollSail on the native 5-row /
13-column rolled grid. Rectangular sails write 65 vertices; triangular sails
write their original 24 scattered vertices. Every other vertex, normal and UV
retains its previous value. Vertical phase advances as in goWave; horizontal
phase, SumWind/WindUp and SailPnt remain unchanged. The native box therefore
comes from the preceding SailPnt samples, including their initial zero values.
Rope outputs retain the original endpoint assignments.

`storm_sim::sail_topology` retains FillIndex's full allocation, normal head,
hole tail, unused middle and winding order for all supported grids. Rolled
triangles use 93 indices and ignore their hole mask; rolled quads use 288
indices and retain their hole material. Only written rolled vertices are drawn.
The renderer packs the normal and hole ranges into the existing buffer and
restores its original count/material ranges when returning to the old mode.

GetGrid's farGrid selection accepts projected distance as an explicit numeric
port. Its seven thresholds, equality boundaries, rolled/free priority and
reported grid changes are checked. Camera matrices, perspective distance and
automatic viewer LOD are not claimed by this selection port. The current
diagnostic chooses the near grid for attached/transitioning cloth and the
native fixed rolled grid after a roll completes.

## Live motion integration

`--native-sail-rolls` requires native sail motion and supports all three initial
sets (0, .5, 1), `--fleet-sail-target`, and `--fleet-window`. It conflicts with
the separate static/full-attached inspection options. It copies the actual
Motion geometry, saved transition and rolled flag before generating vertices.
Hole changes update the saved geometry before the next DoRollingStep. A newly
issued delta-zero request receives the native zero-ms DoRollingStep before
its first vertex update, keeping its coefficients coherent without advancing
time. Yard matrices and overall entity scheduling remain viewer adapters.

The window's W/S controls request the next of the original three sets. Native
rolling timing/reversals, attached waves, rolled positions and index ranges
remain linked. Pause and fractional presentation do not advance cloth. Restart
creates fresh state. A removed owner freezes independently, including when
its hole mask later changes. Raw native unrolling overshoot is preserved.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --native-sail-rolls --fleet-sail-setting 0.5 --fleet-window
~~~

For a GPU-free diagnostic, replace `--fleet-window` with `--time 3
--particle-fleet-report sail-rolls-state.json`; add `--headless sail-rolls.png`
for a frame. Original resources can be supplied separately with `--model PATH
--game-dir PATH`. The new `native_sail_rolls` report includes every local vertex,
all ten geometry vectors, the saved transition, samples/ropes/box, phase/flex
and complete original index allocation/ranges.

Initialization keeps the [attached-wave provider](SAIL_WAVES.md): zero initial
phase/flex, fixed wind base, pinned sine table, no registered ties, fixed-half
depth samples and no limit points. Native numeric rolled form tables are used.
Original weather/RNG/INI, free/falling cloth, constrained yards, rope binding,
heraldry's separate material/UV, full matrix accumulation and game scheduling
remain pending. The default viewer retains its previous cloth unless this
explicit option is enabled.

## Native reference

`reference/sail_roll/generate.py` compiles unedited rolled methods, goWave and
FillIndex under UBSan with contraction disabled and retained vector/vertex /
geometry declarations. The preceding oracle's scaffolding is read as data,
never executed. Free processors abort if selected. Timing instrumentation is
an excluded zero-valued port. GetGrid uses unedited selection/assignment slices
with projected distance supplied separately.

Sixty profiles / 180 complete frames / 37,740 complete vertices compare as float
bits, including all UVs/normals, untouched vertices, rope/sample points, boxes,
phase/flex, turning, default/nondefault form tables and wrapped increments.
525 index profiles compare every entry and range, including unused sentinel
values, all six kinds, supported LOD grids and varied masks. 108 selection
snapshots cover the threshold boundaries and changed flags. CPU integration
checks all three sets, completed/reversed requests, saved geometry after damage
and independent owner release; the GPU check exercises persistent buffers and
exact restoration. Commercial assets/captures/evidence stay outside Git.
