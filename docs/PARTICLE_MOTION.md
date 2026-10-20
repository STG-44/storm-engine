# Retained billboard motion

`storm_sim::particle_motion` ports the live-particle portion of
`BillBoardProcessor::Process`, `SolvePhysic`, `AddGravityForce` and the native
vector/track transform arithmetic. `MotionPort` supplies seven graph values in
native order after advancing the particle's float32 age. Strict `age > lifetime`
expiry skips every graph and leaves capacity release to the owning pool.

Gravity is clamped to 0..100 percent and adds `-9.8 * signed_mass * gravity`
to external Y force. Acceleration divides by absolute mass; zero mass has zero
acceleration. Native Vector division uses a double reciprocal/product, then
converts each component to float32. Velocity is advanced before position.
Friction becomes `clamp(1 - percent * .01, 0, 1)` and multiplies displacement;
it does not damp stored velocity. External force resets after integration.
Signed negative mass therefore reverses gravitational acceleration.

Spin has its own percentage friction. Track X/Y/Z are transformed by the saved
birth matrix. Physics blend is `clamp(1 - graph * delta, 0, 1)`; native Lerp runs
from track to physical position and writes the result back into physical
position. Old render position is retained and render angle follows accumulated
spin. Neither a different integrator nor conventional drag is substituted.

Finite zero/negative deltas preserve the native arithmetic. Non-finite input,
graph output or arithmetic overflow returns a checked error. An individual body
is unchanged on error; graph callbacks already made are not rolled back.
An enclosing emitter/pool update is not transactional. Inputs with native
uninitialized state or non-finite arithmetic are outside the parity claim.

## Preview integration

XPS profiles now supply emitter direction, initial random track offset,
lifetime, mass, spin and velocity. The renderer retains all twelve random
factors and samples friction, gravity, spin friction, track and physics blend
on each processor frame. It preserves the preceding portable RNG draw order,
so birth counts/lifetimes and capacity reuse stay stable. The point-emitter
direction uses the native `rz * rx * ry` matrix Y-axis formula. Trigonometric
library results are not claimed bit-identical across systems.

Living particles use the retained motion step, including during emission
pause and fractional presentation. Cached fixed 60 Hz frames still create all
births; fractional draws advance a copy, so repeated draws do not change the
simulation. Rewind and changed birth/motion/track/position configuration rebuild
the cache. Quad corners rotate by the accumulated angle; atlas coordinates and
appearance factors remain independent of rotation.

The saved preview transform contains the profile's emitter translation in
native coordinates. Z is reflected only at presentation. Fire locators still
add the current ship/node position to profile-space particles. Full historical
world-space birth transforms and movement of the parent ship during emission
are not yet integrated. The sim transform supports affine native point math;
the preview does not reconstruct native old/new matrix interpolation.

`--rig-report` and headless state reports now add `particle_motion` rows beside
existing `particle_births` for active XPS descriptions/locators. They report
living/moving/spinning counts, profile-space render bounds, angle bounds and
maximum speed, allowing CPU-only inspection. They do not expose world bounds.
Commercial data and resource-dependent reports/captures stay outside Git.

Speed-oriented billboards, attached emitters, model-particle motion/rendering,
global processor capacity/RNG ordering, overlapping service generations,
arbitrary native profile dispatch, blend/normal-map techniques, frame blending,
schema conversion and audio remain pending. `AddPower` is a native drawing
parameter, not an extra physics force; its drawing technique remains pending.

## Reference and tests

`rust/crates/storm-sim/reference/particle_motion/generate.py` compiles the
unedited complete billboard Process, both physics helpers and Matrix::MulVertex
with the retained full `math3d/vector.h`, under UBSan with FP contraction off.
It records 96 sequences / 672 complete pool/motion/graph-call snapshots. Cases
cover signed/zero/fractional mass, gravity/friction clamping, time-dependent
graphs, spin, track/physics blend, affine transforms, strict expiry, negative
delta, force reset and swap-removal order. Every live body's motion fields and
every graph call's order/time/lifetime/factor/value are compared as float bits.
Source hashes accompany the fixture.

Graph functions, object construction/storage/freeing and unattached emitter
interfaces are adapters. Attached births stay inactive in this oracle. It does
not execute AddParticle, the full manager, global RNG, model processing or GPU
techniques. Three sim tests compare the fixture and check semantic boundaries
and safe error handling. Two renderer tests cover continuing motion after
pause, configuration rebuilds, rotation and unchanged atlas coordinates.
The existing GPU atlas/clear/repeated-draw test now uses moving, spinning
particles; no additional GPU test is introduced.
