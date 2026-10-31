# Ship crew and damage preview

The Rust preview now loads original deck routes, low-detail sailors, skeletal
walking animation, fireplace textures, mast ownership, hull sections and sail
damage. This is an interactive presentation and sailing prototype. Original
battle logic, sailor AI and the script VM have not been ported.

## Run and controls

```sh
cargo run -p storm-demo --locked -- --ship-demo --fire --sail-volley 3
cargo run -p storm-demo --locked -- --ship-demo --break-mast 2 --break-hull shatter1 --time 2
cargo run -p storm-demo --locked -- --sail-demo --sail-volley 4
```

For an installed game, use the original model command in [the run guide](../rust/README.md),
optionally adding `--crew 12 --deck-view`. Default crew is five when a matching
`RESOURCE/MODELS/Ships/SAILORSPOINTS/<ship>.ini` exists. `--crew 0` disables them;
explicitly requesting sailors without an original route is an error. The limit
is 50. Missing routes do not cause invented paths.

Physical keys work with any keyboard layout:

| Key | Preview action |
| --- | --- |
| C | Hide/show the sailors |
| B | Toggle flame/smoke at fireplace locators |
| M | Break the next mast/topmast |
| P | Detach the next original `shatter` hull section |
| X | Send a test cannonball segment through the centre of the camera view |
| 0 | Restore the preview ship, including all damaged sail sectors |
| Space | Pause/resume the shared scene clock; camera and controls remain usable |

`--fire`, repeatable `--break-mast ID` and `--break-hull NAME` initialize damage
at time zero. `--sail-volley N` sends deterministic short swept test shots into
N sectors of every deployed sail at time zero, using the same collision path as
X. N is 1..12 (triangular sails have ten sectors). Crossings of nearby overlapping
cloth can damage more sectors. This is a repeatable test, not an original cannon
or enemy AI implementation. `--rig-report report.json` exports metadata and masks
without creating a GPU device; `--headless frame.png --sail --state-json state.json`
also exports remaining drive area and sailing state.

## Original formats and behavior

References are the retained C++ source in this repository:

- `src/libs/geometry/src` and `src/libs/animation/src`: GM buffer kind 4 has
  two byte bone indices and one blend weight. AN stores float quaternions per
  bone/frame, parents, bind translations and root positions. The bounded reader
  rejects bad sizes, cycles, bone indices, nonfinite data and invalid clips.
- `src/libs/sailors/src/sailors_way_points.cpp`: SIZE / POINT_DATA / LINK_DATA.
  Point kinds 5..9 are mast routes. The new deck patrol uses the original links
  at 1.3 m/s, excluding mast climbing. It retains self/repeated edges. If an INI
  declares more trailing links than it contains, only recorded edges are used
  and the discrepancy is explicitly reported. Missing interior links still fail.
- Original `Lo_Man.an`, `[walk]` from `Lo_Man.ani`, and six `Lowcharacters/Lo_man*`
  models supply the walking sailors. CPU quaternion interpolation and two-bone
  skinning reuse GPU buffers. Ship pose transforms both crew and deck.
- `src/libs/ship/src/ship.cpp`, `src/libs/mast/src/mast.cpp` (MAST and HULL):
  mast numeric grouping (mast2 includes mast201 etc.) and node subtrees determine
  which yards/sails/flags fall. The fall is new kinematic motion about the
  original node pivot. Cross-mast ropes and shrouds are cut; ropes entirely on
  the detached mast follow it. Named hull sections are hidden after detachment.
- Original `fireplace` / `fireplaces` locators and texture names from
  `RESOURCE/INI/particles/ship_fire.ini` provide flame/smoke. New prewarmed
  deterministic billboard particles use camera-facing quads, alpha blending,
  depth testing and persistent vertex buffers. The complete INI/XPS particle
  interpreter has not been ported.
- `src/libs/rigging/src/sailone.cpp`, SSailTrace / TSailTrace / SetAllHole:
  quad sails use twelve sectors (3 x 4), triangular sails ten (four rows).
  `sail.cpp::GetSailSpeed` reduces drive by hole count / sector count with
  dependency 1.0. The prototype retains that sector count and loss rule.

## Persistent sail holes and sailing

The initial procedural circles were rejected during visual review. They have
been replaced by the original damage material and drawing/collision rules;
see [original sail damage](ORIGINAL_SAIL_DAMAGE.md) for source correspondence,
reproducible C++ comparison and current integration limits.

A swept segment uses the original fifteen/twenty-point coarse collision grid
and records a 10/12-bit mask. Repeated hits still collide with damaged sectors,
as in the original. High-resolution triangles are partitioned into intact and
damaged draw ranges, with original `SetTexGrid` coordinates and wrap/mirror-U
samplers for `Ships/parus_hole.tga.tx`. Repair restores the mask and image.

Damaged and missing sail weights enter `RigLayout.missing_weight`. Surviving
sails keep the original total denominator; losing a mast cannot normalize the
remaining sails back to full thrust. Hole damage and mast loss are not counted
twice. Live controls update the existing sailing session and renderer together.

## Validation and remaining scope

Local checks include malformed AN/GM/routes, real original resources, route
edges and speed, skeleton interpolation, mast groups, cut rigging, alpha
billboards, deformed cloth intersections, persistence and reduced drive. GPU
tests exercise one persistent scene, damage/repair, cloth flutter/furling and
crew visibility. Public CI adds generated fixtures only; commercial resources
and their frames remain outside Git.

The whole fleet audit checks full textured decoding, original route counts,
sail classification and test shots. It does not establish gameplay parity.
The audit separately reports ships without matching deck paths, fireplace
locators or detachable hull pieces.

Still pending: original sailor orders, climbing, combat/reloading and avoidance;
full detached-mast rigid-body motion/collision; projectile meshes, gravity,
cannon firing/reload and hull occlusion; hull HP/decal selection, fire spread,
extinguishing and damage over time; sound; per-ship original balance and scripts;
collision, boarding, sinking and saved damage state. Current ships use the same
approximate sailing configuration. These limits must remain visible when
planning the game entity and script runtime integration.


The local 2026-10-17 audit passes all 40 roots (776 ship GM nodes, 368 sails).
37 ships have matching deck networks and load five sailors each; Boat,
Flyingdutchman_l1 and Polacre_q1 have no matching network at the original ship
path and show no crew. Sloop1 declares 36 links but contains 34. Three other
networks contain original self-links, which remain valid. All sails receive
three test-sector hits through the deformed collision path. Separate local
checks pass 56 CPU tests and 13 Metal GPU tests. A flat-water 40-second generated
fixture reaches 2.744 knots intact and 2.193 knots after six damaged sectors;
remaining drive area is 72.964%. Those figures describe the prototype, not game
balance. The Rust workflow repeats CPU checks on four platforms and GPU/native
fixture checks through software Vulkan on Linux.


A subsequent full-fleet Metal pass renders all 40 ships at 640 x 360, time 1,
with original textures, flags, full sails and default crew where available.
All cells of its labelled contact sheet were visually reviewed. One view per
ship does not cover all poses/damage combinations. Twelve models retain locator
warnings (duplicates, unmatched rope ends or rejected lengths); no incomplete
flag/shroud groups were reported. Missing geometry is not fabricated.

[CI run 39050610912](https://github.com/STG-44/storm-engine/actions/runs/39050610912)
passes all four jobs for code `dd3e12a3`: 56 CPU tests, formatting, Clippy and
native builds on Ubuntu, Windows, macOS ARM and macOS Intel. Linux also passes
13 software Vulkan GPU tests, generated crew/damage PNGs, native windows and an
asserted intact/holed sailing comparison. The downloaded Vulkan damage artifact
was inspected; its sailing results agree with local Metal. Original game
resources and captures are excluded from Git/CI. Physical Windows and Linux GPU
checks with original resources remain pending.
