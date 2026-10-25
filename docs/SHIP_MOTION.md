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
snapshots as float bits. Character lookup/float values, MESSAGE, construction and
ShipRocking are adapters; the oracle does not execute native Mount or Execute.

Cases cover impulse/inertial slots, capacity/reuse, reserved deletion, zero and
positive millisecond frames, raw negative coefficients, SMALL_DELTA boundaries,
fixed speed, dead/collision flags, perk signs, rotated impulses and TouchMove
restoration/side effects. Additional checks cover live attributes, missing/foreign
bindings, malformed messages, finite-state guards, sparse touch output, independent
bodies, impulse consumption, deletion freeze and fractional presentation. The
native-motion fleet report is the nineteenth resource-free standalone scenario.
