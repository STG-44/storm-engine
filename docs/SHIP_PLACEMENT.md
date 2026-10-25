# Immersion, waterline and interactive ship viewer

`storm_sim::ship_placement` ports `SHIP::CalculateImmersion` and the retained
model-root placement expression. Character `Ship.Immersion` refreshes each
simulation frame, defaulting to zero when absent. Ship-type `WaterLine` loads
separately, as in LoadShipParameters. They remain separate stored values.

Native model height is `vPos.y - State.fShipImmersion - SP.fWaterLine`, with
that float32 subtraction order. X/Z world offsets and X/Z dead-direction angle
additions follow the original root expression. The caller supplies independent
presentation vPos/vAng. Renderer Z/yaw/pitch are reflected; roll is preserved.
The component returns the native Euler/translation arguments, not a replacement
matrix implementation. Invalid handles, non-finite inputs and result overflow
produce checked errors. The native movement state and rocking grid are not
shifted by these model-root corrections.

~~~sh
# Interactive native viewer; both ships stay visible, initially without fires.
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --fleet-window

# Original model, paused at 1.5 seconds for inspection.
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --fleet-window \
  --model /path/to/ship.gm --game-dir /path/to/game --time 1.5 --paused

# CPU placement scenario, independent of a graphics device.
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --ship-immersion 0.75 \
  --ship-waterline 0.5 --time 3 --particle-fleet-report ship-placement-state.json
~~~

The viewer supports mouse drag/arrows to orbit, wheel zoom, Space pause/resume,
physical bracket keys to change immersion by 0.25 metres, B to toggle native
fire/smoke, R to restart and Esc to close. Keys work independently of keyboard
layout. Its thirty-second timeline pauses at the end; R starts again. Resize,
minimization/occlusion and lost/outdated surfaces are handled. `--frames 3`
provides a bounded native-window check. A restarted viewer creates fresh bodies,
attributes, histories, service and scenes; no simulation rewind is imposed on
live owners. The shared pause clock excludes wall time while paused.

The viewer reuses fixed 60 Hz native motion/rocking steps, persistent GPU scenes,
shared depth/sea and independent pose histories. Repeated/fractional presentation
does not add simulation steps or fractional history records. Its schedule keeps
both ships, starts without fire and omits the batch fixture's pause/restart/
impulse/removal events. Existing batch scenarios keep their complete reports.
Fire can be enabled explicitly; newborn particles use recorded placed poses,
while particles already released remain in world space.

`--ship-immersion` and `--ship-waterline` initialize diagnostic attributes; they
do not derive each commercial ship's real values from missing PROGRAM sources.
The viewer uses the existing explicit speed/inertia controls and diagnostic
two-wave sea. It is an inspection tool, not complete mounted gameplay. Full
Mount/Execute (including native presentation lag and turn-induced roll), cargo
calculation, sinking, original SEA/wakes, sail/wind speed providers and gameplay
remain pending. Dead-angle additions are covered as placement inputs; no sinking
simulation is claimed. The bracket control bounds only its user adjustment to
-5..10 metres; the CPU component retains raw finite values.

The oracle compiles unedited CalculateImmersion, the retained WaterLine assignment
and the exact root expression under UBSan with FP contraction disabled. Native
CVECTOR is retained; attributes, construction and a CMatrix argument recorder
are adapters. All 64 sequences / 192 complete immersion/Euler/translation
snapshots match as bits, including absent/deleted attributes, signed values,
offsets, dead tilt and large-coordinate rounding. This does not execute complete
LoadShipParameters, native matrix construction or Execute. Further tests cover
live attributes and mounted-value lifetimes, missing/foreign bindings, invalid
numbers, overflow, unchanged motion state and frame-partition-independent viewer
body/history updates. Placement is standalone scenario twenty-one.


## Four-platform evidence, 2026-10-25

Code `d900f9c9abcb1e7268033a579e86e112b5a86b02` passes one
[PR run](https://github.com/STG-44/storm-engine/actions/runs/40224545294) on macOS
Apple Silicon/Intel, Windows and Linux: formatting, warnings-denied Clippy,
builds, 330 CPU tests (135 interpreter tests) and twenty-one standalone scenarios.
Actual host triples/counts are verified from logs. Linux additionally passes
seventeen explicit software Vulkan render tests, existing window fixtures and
three frames of the new fleet window. The local Intel Mac passes seventeen
Metal render tests and three native viewer frames; a paused original-resource
viewer was also launched for inspection. Interaction remains a manual check.
Four archive digests / 84 complete JSON reports match local output; all twenty
preceding reports remain unchanged.

The executable loads all forty original ship roots with the diagnostic 0.75-m
immersion and 0.5-m waterline. Pose height matches native float32 subtraction;
native movement state/rocking grids match the preceding audit. Both model poses
and emitter histories use the placed roots. Across all models, 274 locators
retain 548 systems / 822 finite world rows / 12,243 living particles at three
seconds. Original fire/mast/hull/crew/waypoint metadata remains unchanged.
Four generated/original Metal captures at 1.5/3 seconds were produced; original
frames were inspected. Commercial resources, captures, launcher and evidence
remain outside Git.

This establishes the retained placement component and a bounded viewer. Actual
commercial ship parameters, cargo computation, native presentation lag, sinking,
original SEA and full mounted gameplay remain pending. Physical hosted
Windows/macOS GPU/window coverage remains necessary. Earlier whole-profile/
manual-damage audits were not repeated for this placement/viewer increment.
