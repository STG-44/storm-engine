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
fallback. Character indices and live fireplace counts now have a separate
[checked query subset](SHIP_QUERIES.md). `MSG_SHIP_CREATE`, full native
`SetACharacter` side effects, service initialization, movement impulses,
particle cleanup and original ship physics remain pending. Original commercial
programs still do not boot in this port.


## Four-platform evidence, 2026-10-19

Code `a62c560ea63bd48e83fb49f249fe4574b68ec506` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39210367729) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39210360784):
231 CPU tests, including 135 interpreter tests, formatting, Clippy with warnings
denied, native builds and all thirteen standalone scenarios on each host.
The four verified Rust host triples are `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`.
Linux also passes 13 explicit software Vulkan GPU tests and native windows.
Four downloaded script-state ZIP digests and all 52 complete JSON reports
match local results; all twelve preceding reports remain unchanged.

The eight added CPU tests cover the retained 20-case reference, malformed
messages, real gradual deployment, output-cell semantics, deletion, recovery
and raw commands surviving rudder/focus changes. Float64 preview inputs round
to float32 before selecting a set, consistently with subsequent application.

This phase rechecked all 40 original ship roots and 368 persistent sail masks:
2,204 diagnostic handlers, 1,105 attribute notifications and 1,099 native damage
callbacks. All 80 layout/event reports match the address-phase baseline. That
resource audit was run at the initial phase commit `d6241956`; the subsequent
precision fix affects sailing, which this metadata/collision audit does not
invoke. The final code also renders original ArabellaShip1 offscreen through
local Metal after the startup source; its complete state report is unchanged
by the precision correction. Resources, images and local reports remain
outside Git.

Local native-window attempts for both ArabellaShip1 and the generated fixture
timed out before confirming three presented frames. Offscreen Metal rendering
is verified; this window/surface check remains unconfirmed on this Mac.
Hosted macOS/Windows checks cover compilation and CPU behavior; physical GPU
coverage there remains pending. Full original SHIP initialization/game startup
and the remaining native messages remain unfinished.
