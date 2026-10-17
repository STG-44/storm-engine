# Sailing dynamics prototype

`storm-sim` adds CPU-only planar motion: apparent wind accelerates the ship,
water resists motion, and rudder force turns it. The renderer moves the entire
hull/rig with one rigid transform and keeps cloth in the model frame. These are
new approximate dynamics, not original Storm gameplay parity.

## Play

```sh
cargo run -p storm-demo --locked -- \
  --model /path/to/game/RESOURCE/MODELS/Ships/Barkentine1/barkentine1.gm \
  --game-dir /path/to/game --sail
```

Hold A/D to steer; release to centre. Opposing keys cancel, and losing focus
centres the rudder. Physical W/S keys change effective sail power in 10% steps,
independently of keyboard layout. This changes force without visual furling yet.
The default is full power. Space pauses motion and cloth; camera controls stay
active. Wind controls remain `+`/`-` and `,`/`.`. The title shows knots, course,
sail setting, rudder, wind, and pause. The original preview omits `--sail`.

The camera follows position and initially frames the static model's usual side.
Its world orbit angle stays fixed while the boat turns. R resets the view;
`--yaw` selects an explicit world camera angle.

## Deterministic replay

```sh
cargo run -p storm-demo --locked -- \
  --model /path/to/game/RESOURCE/MODELS/Ships/Barkentine1/barkentine1.gm \
  --game-dir /path/to/game --sail --wind-speed 12 --wind-direction 180 \
  --heading 180 --rudder 0.6 --time 40 --headless sailing.png \
  --state-json sailing-state.json
```

With `--sail`, `--time` first simulates from rest with the specified constant
controls, then captures that state (0–3600 seconds). A native window continues
from there; `--paused` initially holds it. Initial course defaults to wind
direction. Course and wind use 0° towards +Z, 90° towards +X. `--rudder` is
-1..=1; `--sail-setting` is 0..=1. State JSON contains units, pose, velocity,
helm, wind, configuration, and time. It is diagnostic output, not a game save.

The measured 40-second case above travels approximately 70.8 m from its start,
reaches 5.03 knots, and turns from 180° to 203.62°. It renders locally through
Metal with Barkentine1's 20 GM files, three original textures, and all 12 sails.

## Model and limits

The fixed step is 1/120 second with f64 state and semi-implicit Euler. An
accumulator retains fractional ticks; equal durations under constant inputs
produce equal state regardless of frame partition. Live catch-up after a stall
is capped at 0.25 seconds per rendered frame, discarding excess wall time.
Headless replay integrates the full requested interval. Pause advances no ticks.

Drive uses apparent wind (ambient wind minus ship velocity), effective sail
area, and an approximate polar. Within 40° of directly into the apparent wind,
drive vanishes. A side force models leeway. Linear/quadratic resistance is
stronger sideways. Rudder force depends on signed forward speed squared; it
cannot rotate a stationary boat in calm water. Torque and yaw resistance change
course; removing drive lets water resistance slow the boat gradually.

Chosen parameters are 120,000 kg, 35 m length, and 350 m² sail area. They and the
force coefficients are not loaded from original scripts and need calibration.
The static loader reflects Storm Z; the bow points +Z before that reflection,
so model yaw adds pi to the simulated course. Cloth receives apparent wind in
the local model frame, capped at 40 m/s for rendering. Lighting normals rotate
with the hull; static geometry is not re-uploaded.

There is no vertical buoyancy, wave force, heel/roll, physical wake, collision,
island navigation, original furling/rigging, damage, AI, or gameplay scripts.
The water remains diagnostic. Simulation is independent of GPU/window code.

## Validation

Local macOS Intel / Radeon Pro 555X passes 21 CPU tests and four Metal GPU tests.
Physics checks cover calm/headwind, drive, reduced/zero sails, resistance,
opposite helm turns, rudder at rest, frame partition, invalid inputs, and one
hour at maximum wind. Input checks cover release, opposing keys, focus loss,
pause, and bow/course alignment. A persistent GPU scene checks translation,
rotation, fixed sky, and invalid transforms. The original ship's paused native
window presents three frames. Linux CI additionally captures a sailing transform
and state JSON using the upstream cannon and opens its paused window: the cannon
only tests transform plumbing; the original ship stays a separate local test.

The [CI run for code commit dff737ea](https://github.com/STG-44/storm-engine/actions/runs/39009937364)
passed all four jobs on 2026-10-17: Ubuntu 24.04, Windows 2025, macOS 15 Apple
Silicon, and macOS 15 Intel. All run 21 CPU tests, formatting, Clippy, and native
builds. Linux passes all four GPU tests on Vulkan/Mesa llvmpipe, the deterministic
sailing capture/state export, and three frames in the paused sailing window.
Executables and fixture/state captures are workflow artifacts. Hosted Windows
and macOS establish CPU/build compatibility; original ship rendering is tested
locally on Metal with separately supplied resources.

An additional local capture replays 62 s at 12 m/s wind, initial course 180°,
and rudder +0.8. It reaches 6.07 knots, course 245.76°, and approximately 128 m
displacement. The camera follows position at a fixed world angle, so the ship
turns within the view. The accelerated GIF and per-frame trajectory JSON remain
outside Git because they render the original commercial ship.
