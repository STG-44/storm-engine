# Native SHIP sail commands

The first two original `SHIP::ProcessMessage` cases now run in Rust:

| Code | Payload | Behavior |
| --- | --- | --- |
| `50305`, `MSG_SHIP_SET_SAIL_STATE` | `lf` | Store the requested raw float32 sail state. |
| `50325`, `MSG_SHIP_GET_SAIL_STATE` | `le` | Write that state to the supplied native script variable. |

Both successful handlers return zero. These cases and the original
`SetSailState`/`GetSailState` methods are in
`src/libs/ship/src/ship.cpp`; codes come from the retained shared `messages.h`.
The getter uses the existing checked `e` bridge: an address updates the caller
and retypes its DATA to float, while a plain argument writes a stack temporary.
An invalid or pending-deletion entity follows the existing core lookup and
returns zero without invoking the ship or modifying the output.

`ShipSailState` starts as NaN, matching the original constructor. The native
setter does not clamp, quantize or reject float patterns. Signed zero,
subnormals, infinities and quiet NaN payloads survive storage. A native getter
of NaN writes the output float before the existing DATA adapter reports
`NAN ERROR`. The standalone message component does not implement Mount or
`InitSailState` and does not invent character attributes to initialize itself.

## Connection to the sailing preview

`Sailing` uses this same component for native dispatch. Its existing explicit
preview initialization supplies the starting command. `advance`, including a
zero-time advance, applies valid commands to the three sail sets. The raw
command remains distinct from the selected set and from the deployed canvas;
reading `0.4` still returns float32 `0.4` after the normal set is selected.
The existing gradual rig movement and approximate physics remain in use.
Keyboard W/S now use the native setter too; rudder and focus changes preserve
the raw sail command.

The solver accepts only finite requests in `[0, 1]`. Other native requests stay
readable, but applying them fails before moving the ship or advancing time.
A subsequent valid request recovers the simulation. This is an explicit
preview boundary, not a claim that original SHIP clamps those values.

An explicitly supplied startup source can control the same simulation:

```sh
cargo run -p storm-demo --locked -- --sail-demo --sail \
  --wind-speed 0 --wind-direction 0 --wave-height 0 \
  --sail-script rust/apps/storm-demo/fixtures/ship_sails.c \
  --sail-script-report ship-sails-script-state.json
```

This report path exits before creating a GPU device. Omit the report option
to use the script-controlled state in a native window or headless render.
The script runs once before the `--time` replay interval. `PreviewShip` exposes
the existing preview entity; `PreviewAdvanceSailing` advances its real rig and
returns deployed area. Those two helpers and the source fixture are diagnostic
embedding APIs, not original commercial script functions. Source loading uses
the existing bounded, case-insensitive directory loader rooted at the source's
parent. Reports limit sends and advances to 256 each, the script has the usual
instruction/stack limits, and scripted simulation time is limited to one hour.

## Validation and remaining scope

`rust/crates/storm-sim/reference/ship_sails/generate.py` compiles the unedited
two retained switch cases, sail setter/getter and DATA float setter under
UBSan. It extracts the constructor field and message constants, records their
hashes, and generates 20 complete state/output/result cases. MESSAGE transport,
DATA allocation and error handling are adapters; full original initialization,
compiler execution and physics are outside this oracle.

Rust tests compare all cases, exercise malformed fields and unknown codes,
check real gradual deployment, invalid-command recovery, caller/temporary
output behavior, deletion and preservation through rudder/focus changes. The
standalone source scenario requests normal, furled and full sails, then sends
to a deleted entity. Native CI executes it without game assets or a GPU.

This is a two-message SHIP subset. The embedding calls its adapter
`preview_ship_sails` and registers no factory named `SHIP`. Other ship messages
return an explicit unsupported error, unlike the original switch's zero
fallback. `MSG_SHIP_CREATE`, character binding/index queries, live fireplace
counts, service initialization, movement impulses and original ship physics
remain pending. Original commercial programs still do not boot in this port.
