# Ship-fire world birth transforms

Real XPS ship-fire particles now retain world-space birth matrices, positions and
velocities. The current ship pose no longer moves previously emitted particles.
Gravity remains vertical in the world while the hull turns, pitches and rolls.
Particle meshes use an identity object transform and world-space camera-facing
quads; other ship meshes retain their existing object transforms.

## Native contracts

`storm_sim::particle_transform` ports BaseEmitter SetTransform, Teleport and
BlendMatrix, including native quaternion conversion/SLerp/GetMatrix arithmetic.
The first setter initializes old and new matrices; later setters update new.
Teleport sets both. Birth uses the retained substep blend (0 through 31/32),
transforms the profile emitter position and stores that translated matrix in
the particle. The old matrix commits to new only after a complete visible
BornParticles pass. Invisible, paused, attached and early-stopped paths do not
commit it. Point offsets and velocity use native point/normal multiplication;
velocity direction rotates before multiplication by the sampled velocity power.
Teleport preserves the native first-set initialization flag: if it precedes
the very first setter, that setter still initializes both old and new matrices.

The implementation preserves the native SLerp threshold 0.1, sign choice,
float/double intermediates and absence of implicit quaternion normalization.
In the positive-trace matrix conversion, C++ `sqrt(float)` rounds to float before
promotion to double. Non-finite inputs/results and blend factors outside 0..1
return checked errors. General non-rotation matrices are outside the quaternion
contract; native uninitialized/non-finite behavior is excluded.

FirePlace::GetPos applies the ship matrix to its original locator position.
It does not attach the effect to a separately falling mast mesh. FirePlace
passes `(0,1,0)` into PARTICLE_SYSTEM::SetEmitter; that wrapper constructs an
Euler matrix in **radians**. The preview therefore uses the native fixed yaw of
one radian, rather than interpreting that triple as an upward direction or
copying the hull rotation into the emitter. Renderer Z reflection is applied at
the native/world coordinate boundary. The original service transport/profile
dispatch remains an adapter; this increment does not register full PARTICLES.

## Pose history and replay

The approximate sailing simulation can opt into `PoseHistory`. It appends a
validated pose sample after every existing 1/120-second step, independent of
display cadence. The demo enables it before startup source execution, retaining
script warm-up, subsequent headless intervals and live control changes. Paused
time appends no new samples. The history is shared by Arc; append-only prefixes
remain immutable and repeated particle draws do not copy the history.

History is bounded to 864,001 samples / 7200 seconds, matching the bounded
startup plus preview interval. Invalid time/data, edits at existing timestamps,
lock poisoning and exhausted budgets return explicit errors. Sailing checks
the remaining sample/time budget before advancing an interval. The history
belongs to the new approximate ship/wave solver, not original SHIP physics.

WorldEmitter transforms the locator at surrounding recorded ship poses and
linearly interpolates those positions. Cached particle replay still uses fixed
1/60-second frames and native 32 birth substeps; the matrix endpoints use the
exact replay interval before conversion of particle delta to float32. Each
particle stores its own native birth matrix for later track/physics blending.
Emission pause freezes further births while existing world particles continue
their motion. Rewind/configuration/path changes rebuild the replay against the
same retained history, preserving portable RNG draw order and birth counters.

Generic ModelScene callers without an explicit history record their supplied
pose at each draw. The first pose extends backward, and endpoint sampling clamps
to available history. Sparse supplied poses are interpolated, so their accuracy
depends on the caller's sampling. Rewinding or replacing a same-time generic
pose resets this fallback history. Full history is required for deterministic
rewinds/cadence comparisons; the sailing demo supplies it. Explicit generic
teleport/event history and arbitrary rotating emitter dispatch remain pending.
Replay requires history coverage through the requested birth interval (or the
emission stop); it rejects missing future coverage before changing cached state.

## Inspection and verification

CPU rig and headless reports add `particle_world` rows when sailing history is
present. They contain world render bounds, created/living/moving/spinning counts
and maximum speed. Existing `particle_births` and profile-space `particle_motion`
rows remain available. `--sail --rig-report` now executes the requested sailing
interval/startup source and uses its actual resulting time; GPU creation is
still unnecessary for that report.

The retained reference compiles three unedited BaseEmitter methods with the
full original vector, matrix, plane and quaternion headers under UBSan and
disabled FP contraction. A forward-declared D3D matrix pointer type replaces
the unused platform SDK dependency. Forty-nine old/new matrix pairs yield
1,617 complete birth-matrix/point/normal snapshots. Native setters and teleport
are executed; constructor/storage and profile-position assignment are adapters.
Header/method hashes are retained. This does not execute full ship simulation,
native manager dispatch or GPU techniques.

Tests also cover fixed-step history/cadence, rejected history edits, matrix
commit boundaries, rotated locators with fixed fire angles, retained world
positions after a moving parent/emission pause, rewinds and appended history.
The existing atlas/clear/repeated-draw GPU test additionally relocates/rotates
the current ship pose at the same particle state and requires identical pixels.
Startup/live pause tests verify the recorded history includes script warm-up.
Commercial data, captures and local audit results remain outside Git.

Native global timing/RNG/processor capacity, overlapping service generations,
arbitrary profile dispatch, speed orientation, attached/model particles,
shader techniques/frame blending, full native ship physics and audio remain
unfinished. The retained birth/motion arithmetic is distinct from these preview
history and coordinate adapters.
