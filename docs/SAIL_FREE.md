# Original free sail cloth

`storm_sim::sail_free` retains DoSFreeSail and DoTFreeSail, including the original
normalization, MulToInvNorm column products, sample order and random draw order.
Quad sails write 20 scattered vertices and consume 16 draws; triangles write
15 and consume 10. Top anchors move directly to their targets without a draw.
Other samples, vertices, normals, all UVs, rope outputs, phases and wind flex
retain their previous values. goWave's sample-based box is recomputed afterward.
There is no delta-time multiplier or distance clamp: native overshoot is retained.

Inputs expose the matrix axes, wind, height, FreeSailAddition and raw random
draws with their RAND_MAX. A zero target distance can divide by zero in the
original even with valid inputs. The checked kernel reports `NativeNonFinite`
and leaves its state and vertices intact. The viewer holds the last finite frame
and reports held attempts and every consumed draw. Other input errors propagate.

## Live integration

`--native-sail-free` requires `--native-sail-rolls`. At `--fleet-free-at` seconds
(default .75), the first ship's chosen mast falls and its descendant sails gain
the native free flag. `--fleet-free-mast ID` selects a mast explicitly; otherwise
the mast with most attached sails is chosen, breaking ties by lower ID. A model
without sails is a no-op. The second ship remains independent.

As in MSG_SAIL_FREE_GROUP, entering free mode does not unroll or reset samples.
DoRollingStep stops for freed sails, retaining any saved transition. goWave and
GetGrid give rolled cloth precedence; other freed sails use the native 5-row /
4-column grid (48 triangle or 72 quad indices). Attached sails retain their
previous mode. Pause and presentation do not advance cloth; owner release
freezes that owner's state. The renderer restores its previous buffers and
material ranges when native cloth is removed.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --native-sail-rolls --native-sail-free --fleet-sail-setting 1 \
  --fleet-free-mast 2 --time 1.5 --fleet-window --paused
~~~

For a GPU-free report replace `--fleet-window --paused` with
`--particle-fleet-report sail-free-state.json`; add `--headless sail-free.png`
for a frame. Supply commercial resources separately through `--model PATH
--game-dir PATH`. `native_sail_free` contains complete vertices/index allocations,
samples, boxes, saved geometry, flags, axes, update/hold counts and draw counts.
Existing options and report schemas retain their previous behavior.

The diagnostic uses fixed raw draws 16383 / 32767, addition .2 + random * .2,
the preceding fleet pose, yard adapter and static node axes. The existing mast
fall prototype uses a pinned sine/cosine construction only in this opt-in mode
to make report comparisons reproducible. This is not native mast physics or
native group ownership migration.
The existing lost-mast drive adapter still suppresses propulsion from these
sails; it represents that loss through the drive's maximum hole count without
changing their visible damage mask. The [attached](SAIL_WAVES.md) and
[rolled](SAIL_ROLLS.md) provider limits still apply: native RNG/weather/INI,
full matrix/entity scheduling, rope binding/constraints and automatic camera
LOD remain pending. The default viewer is unchanged.

## Native reference

The next increment adds [rope constraints](SAIL_ROPES.md) through a separate flag.
The free-only diagnostic and its preceding reports retain their behavior.

`reference/sail_free/generate.py` reads the preceding oracle's scaffolding as
data and compiles unedited free methods, goWave, geometry/vertex initialization,
CVECTOR and MulToInvNorm under UBSan with contraction disabled. A queued raw-draw
provider replaces rand, with RAND_MAX supplied explicitly; this does not emulate
the platform RNG. Matrix storage is an explicit port to the original method.

72 profiles cover all six kinds, varied wind/height/additions, identity, rotated,
mirrored, scaled, sheared and zero axes, endpoint draws, both 32767 and 2147483647
maxima, and zero normalization. 180 finite frames / 37,740 complete vertices
match float bits, including every unchanged field and the complete sample/box
state. Another 36 native non-finite outcomes check rejection classification and
atomic retention, rather than comparing NaN payloads. Integration checks entry,
transition freeze, rolled priority, partitioned stepping, presentation, independent
owner release and held-frame accounting. GPU checks cover coarse hole ranges and
exact restoration. Resources, captures and local evidence remain outside Git.


## Four-platform evidence, 2026-11-01

Code `97cc7ed4af5cad600f832e007dfc71a68bc20c20` passes the [PR run](https://github.com/STG-44/storm-engine/actions/runs/41251687807)
on Linux, Windows, macOS Apple Silicon and macOS Intel: formatting,
warnings-denied Clippy, builds, 374 CPU tests (135 interpreter) and 29 scenarios.
Four archive digests and all 116 complete JSON reports match local output,
including free flags, axes, sample/vertex fields, saved motion and index ranges.
All preceding 28 reports remain unchanged; host triples/counts are checked in logs.

Linux passes 21 software Vulkan tests and nine fleet-window frames, including
three paused free-cloth frames. Local Metal passes 21 render tests and three
paused new-mode window frames. Six generated/original captures cover entry,
falling cloth and owner release; original before/falling frames were visually
inspected. Three-frame presentation does not establish manual keyboard testing.
Hosted Windows/macOS physical GPU/window coverage remains open.

All 40 original roots / 368 sails pass, with 132 sails entering free
mode on the selected first-ship masts and 0 held non-finite attempts.
Active free indices reference only written vertices; normals and all UVs retain
their preceding static values. Metadata, parameters, motion and owner routing
also pass. At three seconds, 274 fire locators retain 548 systems / 822 finite
world rows / 12,243 living particles under the surviving owner. Boat has zero
sail drive; rowing remains pending. The UBSan oracle regenerates byte-identically:
72 profiles, 180 finite frames / 37,740 complete vertices, 36 non-finite outcomes.

Native mast physics/group ownership migration, RNG/weather/INI, camera LOD,
rope binding/constraints and full matrix/entity scheduling remain pending.
Commercial resources, captures and local evidence stay outside Git. Earlier
whole-profile and manual damage audits were not repeated.
