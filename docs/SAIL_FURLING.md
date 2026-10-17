# Sail gathering and deployment

The sailing prototype now gathers and unfurls both four-corner and triangular
sails. W/S request 10% more/less exposed sail. The rig moves at 0.25 of full
area per second: four seconds for a complete change, or 0.4 s for one key step.
The title displays actual deployment and the requested target. Commands can
reverse a manoeuvre midway; Space holds the rig along with hull, waves and
cloth animation. Controls while paused change the target for resuming.

The CPU model uses actual deployment for drive and leeway throughout the
manoeuvre. Setting a zero target does not instantly remove thrust. Once fully
gathered, the sails provide no drive; water resistance gradually slows the boat.
This is prescribed motion and approximate sailing, not original rigging parity.

## Run and replay

`--sail-setting` specifies the initial fraction, with the sails already in place.
It also works in static ship previews and the resource-free two-sail fixture:

```sh
cargo run -p storm-demo --locked -- --sail-demo --sail-setting 0.25
```

`--sail-target` optionally requests a different fraction when sailing starts:

```sh
cargo run -p storm-demo --locked -- \
  --model /path/to/game/RESOURCE/MODELS/Ships/Barkentine1/barkentine1.gm \
  --game-dir /path/to/game --sail --sail-setting 1 --sail-target 0 \
  --time 2 --headless gathering.png --state-json gathering.json
```

At two seconds the actual fraction is 0.5, while the requested target is zero.
Use `--sail-setting 0 --sail-target 1` to replay unfurling. Without an explicit
target, initial and requested fractions are equal. JSON retains `sail_setting`
as the requested fraction and adds `sail_deployed` and
`sail_deploy_rate_per_second`. This state is diagnostic, not a saved game.
The rig uses the same fixed 120 Hz ticks as movement and wave response; frame
partitioning and pause do not change the result.

## Geometry and scope

The attachment edge is `sail1` to `sail2` for both topologies, following
`SAILONE::DoSRollSail` and `DoTRollSail` in the inherited C++ source. Those edge
vertices stay fixed in the model frame. The remaining quad edge approaches
the attachment edge; the free triangle corner approaches its projection onto
that edge. Analytic folds gather cloth normal to the surface. Full deployment
retains the original preview shape. UVs, topology and GPU allocations are reused;
normals are recomputed analytically. All loaded sails share one deployment value
and still follow the hull's rigid yaw/pitch/roll.

A 2.5% span remains at zero for visible bundled cloth and nondegenerate triangles.
The band contributes no propulsion and has no wind flutter. This visual span
is not a physical measurement of aerodynamic area. Fold size, deployment rate
and the ship's total sail area are chosen prototype parameters. Individual sails,
script-selected rig states, original roll types, ropes, realistic cloth packing,
mass/tension, damage and collision remain unimplemented.

## Validation

Local macOS Intel / Radeon Pro 555X passes 27 CPU tests, seven Metal GPU tests,
formatting and Clippy. New CPU checks cover deployment rate, direction reversal,
actual-area drive, zero-area startup, frame partitioning, pause/target handling,
projected area, attachment edges, UVs, finite unit normals and nondegenerate
triangles for both topologies. Full deployment restores the previous cloth.

A persistent GPU scene checks that both red cloth surfaces become smaller,
remain visible as bundled cloth, stop fluttering when furled and restore after
unfurling. Only cloth pixels count, excluding animated water. Linux CI also
captures the partially furled procedural fixture and verifies a headless JSON
state with actual deployment 0.5 and target zero. The cannon used for that state
export checks plumbing; it is not a ship or sail-geometry validation.

Barkentine1 renders locally with all 20 GM files and 12 sails, including its
two triangles. The local animation replays a four-second manoeuvre from full to
zero deployment, with every frame checked against the CPU fraction. Original
game assets and captures stay outside Git and CI.

The [CI run for code commit 98c8148a](https://github.com/STG-44/storm-engine/actions/runs/39011763235)
passed all four jobs on 2026-10-17: Windows 2025, Ubuntu 24.04, macOS 15 Apple
Silicon and macOS 15 Intel. Every job passes 27 CPU tests, formatting, Clippy,
upstream fixture decoding and native builds. Linux additionally passes all
seven GPU tests on Vulkan/Mesa llvmpipe, the partial-deployment PNG/window case,
and the headless rig-state assertion. The downloaded `furling-vulkan` artifact
was inspected: its PNG shows both reduced cloth surfaces and its JSON records
0.5 actual deployment with a zero target at two seconds. Original ship rendering
remains a separate local Metal check; hosted Windows/macOS verify CPU/build
compatibility, without establishing physical GPU or gameplay compatibility.
