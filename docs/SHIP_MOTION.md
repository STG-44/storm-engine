# Retained SHIP movement and impulses

The port now provides original SHIP strength application and movement as a
CPU component. It preserves native local speed units (knots), millisecond
frame input, acceleration/braking order, yaw integration, fixed forward speed
and the perk-turn override. The existing approximate sailing solver stays
available; this component does not yet replace the complete mounted SHIP class.

~~~sh
# CPU scenario with independent native bodies and the shared particle service.
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --time 3 --particle-fleet-report ship-motion-state.json

# Original geometry with native movement at the prescribed diagnostic controls.
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --model /path/to/ship.gm --game-dir /path/to/game --time 1.5 \
  --headless native-motion.png
~~~

## Native contracts

`CalculateNewSpeedVector`, `ApplyStrength`, `Move`, `TouchMove`, `AddStrength`
and `DelStrength` are the behavioral reference. The component retains sixteen
strength slots with one reserved main slot, first-free insertion, null capacity
results and reserved/out-of-range deletion rejection. Reserved contributions
ignore their bUse flag, as native code does. Non-reserved non-inertial impulses
add to velocity before acceleration/braking and are consumed only by an ordinary
move; collision prediction retains them. Non-reserved inertial entries occupy
a slot but do not contribute to these retained loops.

The asymmetric native SMALL_DELTA test, raw finite signs/coefficients and
1853/3600 knot conversion are retained. Sideways displacement uses yaw + PI/2;
translation uses the pre-integration yaw, followed by yaw rotation. Fixed speed
replaces forward displacement speed while leaving the ordinary state updates
intact. Dead/collision moves skip the rocking callback; dead ships still move.

Native message 50308 (`MSG_SHIP_ADD_MOVE_IMPULSE`, `llffffff`) reads inertia,
world speed and rotation. Its speed rotates to ship-local coordinates through
the retained negative-yaw RotateAroundY calculation. A full slot array still
returns message result zero. Malformed/non-finite payloads fail without inserting
a partial strength. Native code's invalid numeric propagation is replaced by
checked errors; a failed advance leaves the body unchanged. External provider
calls are not rolled back.

TouchMove accepts either/both output records, adds world X/Z offsets, predicts
with collision=true and restores the complete represented motion state. It
retains the vSpeedAccel side effect and leaves impulses unconsumed. Two absent
outputs return no prediction without moving. This is contact prediction, not
collision detection or response.

## Speed and sea providers

The live character lookup follows GetMaxSpeedZ/GetMaxSpeedY at Ship.MaxSpeedZ/Y,
with missing values zero and checked character lifetimes. The movement component
requests these limits through a provider; a sea provider supplies the height
and angles normally produced by ShipRocking. Their call order is part of the
native reference snapshots.

The diagnostic uses explicit 12/14-knot limits, chosen inertia coefficients and
a flat rocking provider. It does not derive the original ship's complete force
configuration from Mount, sails, cargo or weather. Its 60 Hz timeline rounds
cumulative time to integer milliseconds before invoking native Move; fractional
draw time adds no movement step. Poses/world birth histories remain separate
for both ships. A message queues an impulse only on the first at one second;
first-owner removal at two seconds freezes that body while the second continues.
Models without fireplace locators move with an empty effect set.
Renderer Z and yaw are reflected for the already reflected GM resources. The
flat diagnostic keeps pitch and roll zero.

Full Mount/Execute, original ShipRocking/sea, sail/wind speed providers, contact
resolution, islands, naval AI and gameplay remain pending. Weather is tracked
separately in the [weather roadmap](WEATHER_ROADMAP.md), including wind, fog,
rain/storms, lightning/thunder and day/night. Land/deck locomotion, boarding,
character combat, audio, interface and saves remain separate milestones.

## Reference and checks

The oracle compiles eight unedited SHIP methods, the impulse switch case,
RotateAroundY, native movement/strength/touch structs and retained math constants
with the complete CVECTOR header under UBSan and disabled FP contraction.
Sixty-four sequences compare all 704 complete state/strength/provider-call/touch
snapshots as float bits when arithmetic receives the recorded native sinf/cosf
values. The oracle observes those actual C-library calls; the Rust test also
checks their argument bits and call sequence. Production movement uses the
pinned scalar software `libm` 0.2.16 sine/cosine kernels, keeping platform C-library
rounding differences out of the body simulation. Every observed software result
must stay within one ULP of native output, with exact signs/signed zeros. This
separates equation parity from the explicitly chosen portable trig backend.
Character lookup/float values, MESSAGE, construction and ShipRocking are adapters; the oracle does not execute native Mount or Execute.

Cases cover impulse/inertial slots, capacity/reuse, reserved deletion, zero and
positive millisecond frames, raw negative coefficients, SMALL_DELTA boundaries,
fixed speed, dead/collision flags, perk signs, rotated impulses and TouchMove
restoration/side effects. Additional checks cover live attributes, missing/foreign
bindings, malformed messages, finite-state guards, sparse touch output, independent
bodies, impulse consumption, deletion freeze and fractional presentation. The
native-motion fleet report is the nineteenth resource-free standalone scenario.


## Four-platform evidence, 2026-10-25

Code `d47ade14da3998993ddcecec778d63cf2335880d` passes one corrected
[PR run](https://github.com/STG-44/storm-engine/actions/runs/40220630111) on macOS
Apple Silicon/Intel, Windows and Linux. All four native jobs pass formatting,
warnings-denied Clippy, builds, 318 CPU tests (135 interpreter tests) and nineteen
standalone scenarios. Actual host triples and test totals are checked from logs.
Linux also passes seventeen explicit software Vulkan GPU tests, the native-motion
capture and existing window fixtures. All seventeen render tests pass locally
on Metal. Four ZIP digests / 76 complete JSON reports match local results; all
eighteen preceding standalone reports remain unchanged.

The first run exposed platform C-library trig rounding differences on Apple
Silicon and Windows. Pinned software movement trig resolves that variation.
All 704 complete arithmetic snapshots match with recorded native trig inputs;
production scalar outputs are checked within one ULP with exact signs/zeros.
This is the numeric boundary described above, not a claim that every platform's
original C library returns identical sine/cosine bits.

The final executable loads all forty original model roots with two independent
native bodies, including Boat without fireplace locators. At three seconds,
first-owner removal freezes its body while the second keeps moving. Across the
fleet, 274 fire locators retain 548 second-owner systems, 822 finite world rows
and 12,243 living particles. Reflected pose translation matches body state;
fire/mast/hull/crew/waypoint/source metadata matches the preceding resource audit.
Four generated/original Metal captures at 1.5/3 seconds exercise the queued
first-body impulse and surviving second ship. Original frames were inspected.
Commercial resources, captures and audit reports remain outside Git.

Weather is explicitly added to the roadmap; its implementation remains pending.
Previous full manual/damage/whole-profile audits were not repeated for this
opt-in motion component. Physical hosted macOS/Windows window/GPU coverage,
complete Mount/Execute, original sea/sail/weather providers, contact resolution
and full gameplay remain pending.
