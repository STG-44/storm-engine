# Retained ship rocking

`storm_sim::ship_rocking` ports `SHIP::ShipRocking` from the retained C++ engine.
The sea interface supplies heights; it does not implement original SEA.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --time 3 --particle-fleet-report ship-rocking-state.json

cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --model /path/to/ship.gm --game-dir /path/to/game \
  --time 1.5 --headless ship-rocking.png
~~~

The native algorithm samples a 6-by-6 grid spanning root-model X/Z bounds,
rotates samples by current yaw and adds native X/Z world offsets. The mean of
all 36 heights drives vertical position. Three opposing row/column pairs drive
pitch and roll through atan; the final angles interpolate from the independent
native `vAng` snapshot. The 0.1-second upper time cap, division by 0.025 seconds,
separate height/angular smoothing factors, operation order and query order are
retained. Zero time still samples the grid. An absent sea leaves height and grid
untouched and returns `vAng`. The component exposes only ShipPoints.fY; unused
force fields and the matrix-dependent point.y calculation have no effect on this
function's result. Invalid dimensions/numbers and bad provider outputs produce
checked errors without committing a partial grid. External queries cannot be
rolled back.

The new fleet option uses root GM header dimensions, as native Mount does,
rather than complete assembled rig bounds. Resource-free ships use chosen
6-by-16-metre fixture dimensions. The living-frame adapter captures previous
angles from current state immediately before native Move. Two ships keep
separate grids, state and pose histories; releasing the first freezes its state
and grid while the second continues. Renderer Z/yaw/pitch are reflected; roll
is preserved. Water queries reflect Z as well. All ship geometry and world-born
particles receive the resulting rigid pose. Fractional draw time adds no body
or rocking step. Collision/dead Move still skips the rocking callback.

The surface is the existing two-wave diagnostic, with the same float
coefficients as the water shader (1.5-metre envelope, direction 1.2 radians).
Its CPU sampler uses pinned scalar software sine. GPU wave evaluation may
differ by normal shader rounding. This is not original water, buoyancy/keel
collision, cargo immersion, native waterline configuration, wake generation,
turn-induced presentation roll, sinking or full Mount/Execute. These remain
pending together with sail/wind/weather providers. The earlier prescribed and
flat-native fleet modes keep their report formats and behavior.

The oracle compiles the unedited ShipRocking, RotateAroundY, Min and complete
CVECTOR header under UBSan with FP contraction disabled. Matrix/state/sea
construction are adapters. Ninety-six sequences compare 480 complete outputs,
all grid heights and all sea/math queries, including absent/zero-time sea,
offsets, varied hull dimensions, headings, smoothing factors and capped times.
Arithmetic replays observed native sine/cosine/atan values exactly; production
uses pinned libm kernels checked separately within one ULP with exact signs and
signed zeros. Prescribed analytic/constant/alternating heights exercise grid
averaging without replacing the native method body. Additional tests cover
plane signs, the time cap, failed-grid rollback, sea/pose reflection, original
root dimensions, independent grids, owner removal and fractional presentation.

The rocking report is the twentieth resource-free standalone scenario.


## Four-platform evidence, 2026-10-25

Code `985096015e25fa3ad0e4e4eeb531a686fdb7f936` passes one
[PR run](https://github.com/STG-44/storm-engine/actions/runs/40222394085) on macOS
Apple Silicon/Intel, Windows and Linux: formatting, warnings-denied Clippy,
builds, 325 CPU tests (135 interpreter tests) and twenty standalone scenarios.
Actual host triples and counts are verified from logs. Linux additionally
passes seventeen explicit software Vulkan GPU tests, the new rocking capture
and existing native-window fixtures. All seventeen render tests pass locally
on Metal. Four archive digests and all 80 complete JSON reports match local
output; the nineteen preceding reports are unchanged.

The executable loads all forty original model roots, including Boat with no
fireplace locators. Grid dimensions match the root GM headers; both hulls have
finite nonzero heave/pitch/roll. First-owner removal freezes its body and grid
while the second continues. Across all ships, 274 locators retain 548 surviving
systems, 822 finite world rows and 12,243 living particles. Fire, mast, hull,
crew and waypoint metadata match the preceding audit. Four generated/original
Metal captures at 1.5/3 seconds were produced; the original frames were inspected.
Commercial resources, captures and audit reports stay outside Git.

These checks establish the retained rocking calculation over a diagnostic sea.
Original SEA, full mounting/immersion/waterline, wakes, sinking, force providers
and complete gameplay remain pending. Earlier whole-profile/manual-damage
audits were not repeated for this opt-in rigid-body pose increment. Hosted
Windows/macOS GPU/window behavior still needs physical coverage.
