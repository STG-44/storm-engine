# Original sail geometry and fixed vertex grids

`storm_sim::sail_geometry` retains `SAILONE::SetGeometry`, `FillVertex`, and
`SetTexGrid`. The 17-row / 13-column quad grid has 221 vertices; the triangular
grid has 153. The original hard-point order, arithmetic order, normal origin,
explicit final anchor overwrites and all three UV sets are retained. Corner
overwrites do not recompute normals, matching the source. CVECTOR normalization
sums in float before a double sqrt/reciprocal; division by a float uses a double
reciprocal. The Rust implementation uses pinned `libm` square roots.

SetGeometry updates a rolling block's saved geometry when one exists, leaving
the current nine scaled vectors alone and copying its `dopV` back to the current
geometry. Triangle fields not assigned by the method retain their preceding
values. All ten vectors are preserved, including the extra vector beyond the
legacy `v[9]` view. Non-finite input/results fail before committing geometry.
Zero-length normals return zero as in CVECTOR; degenerate reference inputs are
not silently given replacement normals.

## Integration and static inspection

Fleet rolling states now initialize their real geometry coefficients through
SetGeometry instead of carrying zero-valued ports. This does not yet connect
the normal animated preview to native wave vertex generation or hole-triggered
geometry rebuild scheduling.

The explicit `--native-sail-geometry` batch option displays the original static
full grid and exports every local position, normal and all three UV sets as
float bits. It requires `--native-sail-motion` and an initial full sail setting
of 1, and conflicts with sail targets and the interactive fleet window. This
keeps the inspection separate from the existing rolling preview: FillVertex
alone is not the original per-frame rolling/wave renderer.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --native-sail-geometry --fleet-sail-setting 1 --time 1.5 \
  --particle-fleet-report sail-geometry-state.json
~~~

Add `--headless native-sail-grid.png` and a native backend for a frame, or
`--model PATH --game-dir PATH` for external original resources. Local hard-point
math happens before applying the stored node frame; normal conversion uses its
inverse transpose. The renderer reuses the current vertex/index buffers and
the existing sail damage indices/material. It consumes the first native UV set;
heraldry's separate material/UV binding remains pending. Yard/root transforms
still use the documented viewer adapters.

The static depth provider uses the no-limit-point SetSail expressions with
explicit half-valued random samples. It does not claim native RNG, limit-point
loading, wind response or complete SetSail initialization. Holes use the original
quad-flex multiplier; the native triangular SetGeometry path ignores that count.
This inspection replaces preview flutter with a fixed construction, while hull
movement and the diagnostic sea remain active. Ordinary scenarios keep their
previous animated cloth and report schemas.

## Native reference

`reference/sail_geometry/generate.py` compiles the four unedited C++ methods
(including DoRollingStep for the second snapshot), full CVECTOR/SAILVERTEX and
SAILGEOMETRY/SAILROLLING declarations under UBSan, with contraction disabled.
48 distinct profiles compare 96 complete geometry/sphere/rolling snapshots and
20,128 complete vertices as float bits. Coverage includes all six kind values,
translated/skewed/degenerate hard points, preserved nonzero geometry, active
saved targets, several hole/flex values, different heraldry scales and scaled
rolling state. Depths and surrounding object fields are explicit inputs; no RNG,
native wave/LOD, node matrices or renderer execution is represented by the oracle.

CPU integration tests cover local construction before parent transforms,
attachment points, hole-dependent geometry, all UV channels and unchanged drive.
The GPU test switches the persistent scene into/out of static construction and
checks that preview wind cannot move it, with exact restoration of the old mode.

GoVWave/GoTWave, DoSRollSail/DoTRollSail, native rolling topology/LOD, the original
wind table/RNG/INI, limit points, rope constraints, native matrix accumulation,
full entity scheduling and complete gameplay remain pending. Commercial assets,
captures and audit evidence stay outside Git.
