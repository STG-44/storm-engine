# Original sail rope constraints

`storm_sim::sail_rope` retains SetTurnLimits and the rope-constrained branch of
TurnSail. Turning triangles search the original .01-radian steps independently
for each side. A failed search retains its preceding angle. All original guards,
strict comparisons and rolled-state priority are retained. Checked search input
is bounded to -16..16 radians; this includes the native .6 default.

The numeric MSG_SAIL_ROPE_TIE path retains the 1000 squared-distance threshold,
strict nearest-anchor order, top-anchor exclusion from rotation records, two
record slots, duplicate registration behavior, r1/r2 and the quad b construction.
Triangle b is unused by the original and is initialized explicitly by the port.
The endpoint query retains ROPE::GetEndPoint's first number/model match, negative
number direction, unchanged output on a miss, matrix point multiplication and
MulToInv transpose products, including non-orthonormal inputs. Invalid i32::MIN
negation is rejected; the null-output guard still returns without reading it.

TurnSail first updates its node, then queries endpoints. Quad points use the
original CVECTOR double-reciprocal division; triangles update x/z with float
division and preserve y. Records run in order and SetGeometry runs once after
the changed points. Active rolling blocks receive the new saved geometry;
fully rolled sails retain their original tie 2/3 endpoint assignments. The node
port observes the original position/rotation/global-update call order. Checked
non-finite constraint results retain motion and node; external endpoint-query
observations remain counted. The viewer holds such turns and reports them.

## Live integration

`--native-sail-ropes` requires `--native-sail-rolls`. Motion feeds the actual
changed hard points/geometry and sphere to cloth; goWave publishes its tied
points back to the next frame and to visible rope endpoints. Full, normal and
furled sets, requests/reversals, damage rebuilding, pause, fractional
presentation and independent owner release use the existing fixed-step fleet.
The default viewer and the preceding diagnostic reports retain their behavior.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --native-sail-rolls --native-sail-ropes --fleet-sail-setting 1 \
  --time 1.5 --fleet-window --paused
~~~

Replace `--fleet-window --paused` with `--particle-fleet-report sail-ropes-state.json`
for a GPU-free run, or add `--headless sail-ropes.png` for a frame. Supply original
resources through `--model PATH --game-dir PATH`. The generated scene explicitly
adds six noncommercial fal locators and a triangular yard; earlier generated
scenarios are unchanged. `native_sail_ropes` contains full cloth/index data;
`native_sail_drive.motion.bodies[].rope_constraints` records hard points,
current/saved geometry, ties, limits, sphere, records and turn/rebuild/query counts.

The service adapter uses paired rope/fal locators in viewer order, static node
frames and the existing yard-delta constructor. Fal numbers include native +1000;
duplicate numbers retain the first pair and are counted. Native label traversal,
numeric-group collisions, registration messages/pointer ownership, rope removal
and complete matrix accumulation are not established by this adapter. Stay/vant
preview meshes do not become native rotation records. Parent GM box widths feed
SetTurnLimits; the generated fixture supplies explicit zero widths. The endpoint
registry is separate per ship. Missing bound endpoints fail explicitly. Native
entity service lifetime and full scene scheduling remain pending.

The [cloth](SAIL_WAVES.md), [rolling](SAIL_ROLLS.md) and [free-cloth](SAIL_FREE.md)
provider limits remain: fixed initial phases/wind/table/depths, native RNG/INI /
weather and limit points, automatic camera LOD, heraldry material, original sea,
mast physics/group migration and full Mount/Execute are separate work.

## Native reference

`reference/sail_rope/generate.py` compiles unedited SetTurnLimits, TurnSail,
SetGeometry, DoRollingStep, GetEndPoint and its CMatrix point/MulToInv methods,
plus the retained MSG_SAIL_ROPE_TIE loop. UBSan and disabled contraction are used.
Node matrices are observed calls, endpoint matrices are supplied, and the
geometry rebuild counter is wrapper instrumentation outside the retained body.
Unused uninitialized triangle b values are excluded, as are NaN payload equality
and native registration/matrix lifetimes.

78 profiles cover all six kinds, registration guards/duplicates, rolled and
active transitions, varied limits, zero/negative search angles and moving,
rotated, mirrored and scaled/sheared endpoint frames. 546 endpoint queries and
900 complete finite snapshots match float bits, including every represented
geometry vector, current/saved state, hard/tie points, records, sphere, limits
and node call. Five profiles change a limit. Six native zero-denominator outcomes
verify atomic rejection. Integration checks geometry/cloth/rope feedback, damage,
reversals, partitioned presentation and owner release; GPU checks change only
the live rope endpoints and restore the preceding image. Commercial assets,
captures and local evidence stay outside Git.
