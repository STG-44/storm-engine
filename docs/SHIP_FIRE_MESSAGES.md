# Native SHIP fireplace lifecycle

The Rust SHIP subset now handles two more retained messages:

| Code | Payload | Behavior |
| --- | --- | --- |
| `50304`, `MSG_SHIP_ACTIVATE_FIRE_PLACE` | `llsssfl` | Index, smoke profile, fire profile, sound profile, float32 duration, projectile character owner. |
| `50315`, `MSG_SHIP_SAFE_DELETE` | `l` | Stop active fireplaces and clear the actual fireplace-record vector. |

Both return zero. Activation reads every field before checking the unsigned
index, matching `SHIP::ProcessMessage`. Invalid indices and malformed fields
produce checked errors before changing the selected fireplace. A count query
immediately sees zero after SAFE_DELETE. This message does not itself delete
the ship entity. Recreating the preview entity initializes a new locator vector;
no activation or keyboard action silently repopulates a cleared vector.

## Retained lifecycle and service boundary

`storm_sim::fire::FirePlace` ports Run, Stop and Execute from
`src/libs/ship/src/fire_place.cpp`. Running an already active fireplace is a
no-op, including its remaining time, owner and profile names. Without the
particle service activation is ignored. With that service, activation succeeds
even when one or both particle creations return null. Smoke is requested before
fire, then the looping 3D WAV/FX sound. The native service arguments, including
up direction, zero creation time, sound flags and zero stop fade, are asserted
by the C++ reference adapter.

Execute subtracts the raw float32 delta. It stops only when remaining time is
strictly negative: exact zero remains active. Negative inputs, infinities and
NaN retain the native arithmetic. The native constructor leaves run time
uninitialized; Rust represents that undefined initial field as `None` and never
reads the C++ value before a successful Run. The public preview clock itself
still requires finite, forward time.

Stop validates each non-null particle through the available service and pauses
its emission; it does not destroy already emitted particles. It clears local
particle handles, stops a nonzero sound, resets sound/owner/active, and retains
profile names and remaining time. Execute moves live emitters and the sound,
including a sound ID of zero, as the retained implementation does. `ShipFire`
owns the service port and invokes Stop during both SAFE_DELETE and Drop.
Cleanup runs from the end of the vector, matching the retained macOS libc++
reference. C++ vector destruction order can differ with another STL; this is
an explicit canonical order for the Rust port, not a portable C++ guarantee.

`FireEffects` abstracts particle availability/creation/validation, pausing and
position updates, plus sound play/stop/position requests. `ShipFire` uses the
same actual records that serve 50306 and the existing live character queries.
Emitter positions use the preview ship matrix. Original GetPos, Mount,
SetACharacter and service bootstrap remain outside this increment.

## Application and rendering

The diagnostic `preview_ship_sails` class owns a controller for each instance.
A shared preview particle service survives native entity destruction, allowing
paused particle tails to decay. The startup result retains the selected live
controller for subsequent simulation frames; deleting it first stops it and
leaves only the service. Replay, live updates and pause use the sailing clock.

The existing billboard renderer now selects individual fireplace/kind emissions
and stops spawning after their stop timestamp. Existing flame/smoke tails decay
within the preview lifetimes (2.4 / 7 seconds). An explicit empty emission list
suppresses fire even when the legacy manual fire flag was set. The service
tracks particle IDs separately, but presentation shows only the latest
emission per locator/kind. Overlapping generations/instances at the same
locator, original particle profile configuration, particle physics and audio
playback remain unfinished. Profile names and owner IDs are retained; sound
requests are explicitly reported as undelivered with ID zero.

With a native fire session, B stops active fireplaces or restarts existing
records through the activation handler. A restart uses saved profile names,
preview fallbacks, a 60-second duration and owner -1. Zero stops native emission
and clears its displayed tails, then performs the existing preview repair.
These keys are diagnostic policy, not original gameplay or message semantics.

```sh
cargo run -p storm-demo --locked -- --ship-demo --sail \
  --wind-speed 0 --wind-direction 0 --wave-height 0 --ship-character 19 \
  --sail-script rust/apps/storm-demo/fixtures/ship_fires.c \
  --sail-script-report ship-fire-script-state.json
```

This resource-free source queries the actual vector, ignores an active rerun,
checks exact-zero survival and negative-time expiry, restarts, clears twice,
recreates the entity and leaves one fire active for live/headless rendering.
It finishes at 2.125 seconds with one second remaining and owner 23. The three
advances observe active counts 1, 0, 1. The service records six creations and
four pauses; the source performs nine delivered messages on a model with
fireplaces. A model without locators executes only the initial count query.
Omit the report option to open a window, or use `--headless frame.png --time 0.5`.
Replace `--ship-demo` with an original model and `--game-dir` to use real locators.

## Reference and checks

`rust/crates/storm-sim/reference/fire/generate.py` compiles eight unedited
FirePlace methods and the two SHIP switch cases under UBSan. The committed
macOS libc++ fixture contains 60 lifecycle sequences / 480 complete state and
ordered-service snapshots, plus 15 native message sequences. It covers missing
services, null creation results, validation masks, lost services, sound IDs
zero/nonzero, reruns, restart, destruction, signed index conversion and raw
float boundaries. Retained method hashes and message/service constants are
recorded. MESSAGE storage, Core/service dispatch, object layout, GetPos and
assertion-to-error conversion are adapters. No original particle simulator,
audio driver, save/load or complete SHIP initialization is executed.

Twelve added CPU tests cover those comparisons, malformed messages, cleanup,
service lifetime, preview keys, script/entity replacement, pause/replay and
locator-specific particle decay. An additional explicit GPU test compares
active-fire pixels and verifies that clearing native emissions restores the
unlit image. Native CI runs the fifteenth standalone scenario on all four
hosts; Linux also opens its native Vulkan window.
