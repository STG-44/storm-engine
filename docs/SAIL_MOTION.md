# Sail rolling state and unconstrained yard turning

`storm_sim::sail_motion` retains `SAILONE::SetRolling`, `DoRollingStep`, and
`TurnSail`'s path when no rope entity service is present. It preserves repeated
command no-ops, reversals without allocating/resetting the rolling block, forced
unfolding turns, the asymmetric native end checks, and the original nine-vector
geometry scaling/restoration order. The tenth `dopV` vector remains untouched.
The raw unfolding delta can exceed one for a frame; sail drive consumes that raw
value while visible deployment is bounded to 0..1. Fold completion tests `< 0`,
so a delta of exactly zero still has a transition. Unfold completion tests the
preceding delta `> 1`.

The default rolling rate is 0.0003 per millisecond (about 3.33 seconds for a full
transition). Turning retains the 0.002 radians/ms step, 0.01 deadband and default
0.6 angle limits. The forced SetRolling turn uses the fixed 0.002 angle, independent
of frame length, and does not emit a global matrix update. Folded tie updates and
node operation order are represented by an explicit port. Inputs and resulting
geometry must be finite; failures leave the scalar/geometry state unchanged.

## Fleet and rendering adapters

The viewer enables motion automatically. W/S selects the first ship's three sail
sets. Space pauses; R reloads the initial state. Batch mode opts in explicitly:

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --native-sail-motion \
  --fleet-sail-setting 1 --fleet-sail-target 0 --fleet-wind-direction 135 \
  --time 1.5 --particle-fleet-report sail-motion-state.json
~~~

`--fleet-sail-target` changes only the first ship after both ships initialize to
`--fleet-sail-setting`. Reverse 1/0 to observe unfolding. Without the motion flag,
the preceding instant-set drive fixture and its report remain unchanged.

Each fixed frame publishes pending viewer node changes, advances rolling,
evaluates wind with the current axes, turns eligible `rey_` nodes, queries speed
and applies requested rolling changes, then advances the ship. This is explicit
viewer scheduling, not a claim of full native entity/Model scheduling. Multiple
sails share a node adapter in mesh order; native registration order is pending.
Node adapters retain local/global yaw separately and use pinned scalar trig.
They reconstruct rotations around the original parent-space pivot and compose
inherited transforms. They do not reproduce native CMatrix accumulation bits.

Cloth and rope ties use the same per-sail deployment. Yards, their descendant
meshes, flags, and rope endpoints use the same node transforms; falling-mast
transforms apply once afterwards. Damage masks continue to select original sail
topology. Releasing a ship freezes its rig and drive while the other keeps running.

The current cloth renderer remains kinematic. It does not use original
SetGeometry/GoVWave/GoTWave/DoSRollSail/DoTRollSail vertex generation or native
rolling LOD/topology. The fleet's SAILGEOMETRY vectors are zero-valued ports;
the core geometry arithmetic is separately tested on nonzero native fixtures.
Rope-constrained turning, SetTurnLimits, original INI overrides, native rope
simulation, detached-sail rehosting, weather, sound and full gameplay scheduling
remain pending. Rendered rig transforms are adapters, not full rigging parity.

## Reference and validation

The reference generator compiles the unedited three C++ method bodies and full
SAILGEOMETRY/SAILROLLING declarations under UBSan with contraction disabled.
64 distinct profiles compare 1,088 complete represented state snapshots as
float bits: current/saved ten-vector geometry, rolling flags/delta, old wind
angle, tie endpoints, node position and ordered node calls. It covers all six
sail kinds, reversals/repeated commands, exact boundaries, overshoot, zero/long
steps and different rates/angle limits. Matrix operations are observed ports;
the absent rope branch aborts if entered. This reference excludes renderer
execution, native matrix arithmetic and cloth construction.

Regenerate with `python3 rust/crates/storm-sim/reference/sail_motion/generate.py`.
Additional tests cover parent-space yard pivots/inheritance, rope endpoints,
partial drive, reversal continuity, fixed forced turns and independent release.
The resource-free CLI scenario is included in the four-host workflow. Original
resources and local capture/audit evidence remain outside Git.
