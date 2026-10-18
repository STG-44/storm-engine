# Immediate script event handlers

The preview now delivers `DoSailHole` and `ShpSailDamage` to executable Storm
source handlers. `OnSailHole` reads the original nine fields with `GetEventData`,
then writes `characters.<index>.ship.sails.<node>.<group>.hd` using `ref`, `aref`
and dynamic attribute paths. The write notifies a native entity through the
same owned attribute world. `OnSailDamage` reads all five fields and calls an
explicit diagnostic binding. The observer no longer writes masks.

The [fixture source](../rust/apps/storm-demo/fixtures/sail_events.c) is newly
written compatibility code, not the original commercial game's handlers. It
does not implement character lookup, damage balance or combat AI. Collision
finishes before these callbacks run; script changes to attributes cannot yet
alter collision traversal or cancel the already computed hit.

```sh
cargo run -p storm-demo --locked -- \
  --ship-demo --sail-volley 3 --ship-character 19 --shot-owner 7 \
  --rig-report ship-layout.json --event-report sail-events.json
```

This requires no GPU or game resources. The report has `script_vm: true`,
`original_game_scripts: false`, 12 completed handlers, six calls of each event,
six attribute notifications and two restored sail masks. `script_last_damage`
records the five fields received by the script's native binding. Native X shots
use the same path; reports retain the latest 256 payloads.

## Runtime and original behavior

`storm_script::Runtime` owns a compiled `Program` and a persistent event table.
Call an initialization function with `Runtime::call`, deliver an owned
`storm_core::Message` with `Runtime::emit`, and advance registrations with
`Runtime::process_frame`. `Host::world` supplies persistent attribute storage.
The older `Program::call` remains a standalone call; its event table is temporary.

The source builtins are `SetEventHandler(event, function, flag)`,
`DelEventHandler(event, function)`, `GetEventData()` and `EventsBreak()`.

- ASCII names compare without case. Handlers run in registration order.
- A new registration with flag zero is immediately active; other flags defer
  activation to `process_frame`. Registering an existing pair reactivates it
  immediately without changing its position or adding a duplicate.
- Deletion marks a handler inactive immediately. Dispatch re-reads the live
  table after every call, so deleting a later handler skips it; appending a new
  flag-zero handler allows it to run in that same dispatch.
- Each active handler rewinds the message. `GetEventData` reads the next `l`,
  `f` or `s` as an int, float or string. Reading past the end or reading an
  unsupported field fails with the event, handler and source location.
- The last non-void result survives later void handlers. `EventsBreak` finishes
  the current handler and suppresses the remaining handlers for that dispatch.
- Frame processing preserves the retained C++ index-reset quirk: after deleting
  an entry, iteration resumes at index one. A shifted first New/Deleted entry
  can therefore retain that status until another frame. Empty event entries
  remain allocated, as in `S_EVENTTAB`.

References: `core/src/compiler.cpp::SetEventHandler/DelEventHandler/ProcessEvent`,
`core/src/s_eventtab.cpp`, and the corresponding builtin branches in
`core/src/internal_functions.cpp`. The reference generator compiles the unedited
`s_eventtab.cpp` and header with only their unused `data.h` include stubbed:

```sh
python3 rust/crates/storm-script/reference/events/generate.py
```

Its 17 sequences contain 521 status snapshots, including duplicate registration,
case folding, deletion, flag activation and the frame index quirk. It runs under
UndefinedBehaviorSanitizer. Rust tests separately cover source dispatch, payload
rewind, live table mutation, results, failures, shared budgets and the full
collision-to-script-to-attribute path. A test changes only the handler assignment
and verifies that the restored mask changes accordingly.

## Bounds and pending scope

Tables allow 128 event names (up to 256 ASCII bytes each), 512 registrations and
128 message fields. One instruction budget covers an entire dispatch, including
its callees and appended handlers. Existing call, string, frame and stack bounds
apply. Handlers must have no formal parameters. Errors stop dispatch and leave
already completed side effects intact; later calls start with fresh execution
and message state.

Nested `Event`, delayed `PostEvent`, static `#event_handler`, segment unloading,
`ClearEvents`, entity/attribute/script-variable payloads (`i/a/e`), original
program loading and the full native game API remain pending. This establishes
immediate handler execution for preview sail events, not complete script VM or
gameplay compatibility. The collision and sail-damage arithmetic remain the
previously verified implementation.

## Local verification, 2026-10-18

Formatting, Clippy with warnings denied, native builds and all 106 CPU tests
pass, including 31 script tests and the 521 retained event-table snapshots.
The generated fixture preserves the prior complete payload list, ordered
attribute tree, six notifications and masks 548/274 while executing 12 source
handlers. Its canonical JSON SHA-256 (sorted keys, compact separators) is
`d2c7f2b75174144f8def5bedd4fbc1432a1b34365eef7cedc73570d7583158c5`.

A resource-dependent audit checks all 40 original ship roots and restores all
368 sail masks. It executes 2,204 source handlers: 1,105 `DoSailHole` writes with
entity notifications and 1,099 `ShpSailDamage` native callbacks. Every final
mask matches the loaded ship state. The zero-sail boat emits no events. A
textured Barkentine1 capture succeeds on local macOS Metal with 71 handlers,
36 attribute notifications and 12 reconstructed masks. Commercial assets,
captures and resource-dependent reports remain outside Git.
