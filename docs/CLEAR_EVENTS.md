# ClearEvents table cleanup

The source builtin `ClearEvents()` and native embedding method
`Runtime::clear_events()` apply the retained event-table operation. They mark
non-static handlers deleted and immediately run the original frame-status
cleanup. Static handlers are not marked deleted; an already disabled static
handler is not resurrected. Pending static entries follow normal frame
activation, including its skipped-entry quirk. Re-registering a surviving pair activates it at its
existing position, preserving its initial static flag.

This operation does not touch posted-event names, payloads, timers, preparation
flags or counters. It does not invoke the scheduler, cancel posts by name, reset
globals or erase native entities. `ClearPostEvents()` remains the separate
operation that invalidates all posted entries. `DelEventHandler` has its own
retained queue cancellation/preparation side effects; `ClearEvents` does not use
that path. The public embedding method can run before the first entry without
allocating globals or taking the Host's World.

The original cleanup resets its index after deletion. This can leave the first
shifted dynamic entry in the table with deleted status after one clear; it is
inactive during dispatch. Another clear/frame can remove it. Event names with
empty handler lists remain registered. Rust preserves those states, so cleanup
refunds erased handler slots but does not refund the 128-name budget. Static and dynamic
handlers retain the shared 512-entry bound.

If an already disabled static entry traps the original cleanup in an infinite
loop, both Rust entry paths return the existing bounded error. A source call
includes its function/file/line context. Already applied changes remain visible;
re-registering the static pair permits later calls to proceed. Posted events
remain intact on this error as well.

Clearing from a running handler does not abort that function, rewind its message
or reset the shared EventsBreak flag. Subsequent handler lookup uses the live
table, including physical removals made by the clear. Nested events keep the
existing message-pointer and break propagation rules. Source calls take no
arguments, return void and use the caller's shared instruction budget.

## Diagnostic and retained reference

```sh
cargo run -p storm-script --locked -- \
  --script-root rust/crates/storm-script/fixtures/static_ship MAIN.C \
  --entry ClearMain --frame-ms 0,0 --report clear-script-state.json
```

`ClearMain` first runs the existing ship scenario, which stores masks 548/274 in
two global characters and queues its commit. It then adds a dynamic commit
handler that would write an unwanted ship attribute, and calls ClearEvents.
The static commit and queued message survive. The second supplied frame emits
three native notifications, while the dynamic handler never runs. Its complete
report equals the preceding static scenario, including source names, frame
deliveries, handler counters and attribute trees. All diagnostic source is newly written.

`reference/clear_events/generate.py` copies S_EVENTTAB source/header unedited,
including Clear and ProcessFrame. It extracts the native `FUNC_CLEAR_EVENTS`
case, CoreImpl::ClearEvents and COMPILER::ClearEvents verbatim, routing every
clear through those bodies. Owning-class declarations and the input/state
driver are scaffolding. C++20 with fail-fast UndefinedBehaviorSanitizer produces
25 sequences with 627 snapshots of function order, status and static flags.
One final isolated process confirms the original clear-time loop with a
one-second timeout; Rust returns an error for that operation. Source/body hashes
are recorded in golden data. This table oracle does not execute posted messages,
the full original interpreter or a game program.

Separate source tests cover public/script entry paths, global/static lifetime,
queue payload/timer preservation, clearing inside live/nested dispatch, message
cursors and EventsBreak, error recovery, bounds and the native ship scenario.
Full original game startup, dynamic segment unloading, saved/cached programs and
the remaining native game API are separate work. This phase adds no game assets.

## Four-platform evidence, 2026-10-19

Code `d63efa993cf22622476614355cef8015398c668a` passes the
[PR workflow](https://github.com/STG-44/storm-engine/actions/runs/39172045516)
and [push workflow](https://github.com/STG-44/storm-engine/actions/runs/39172042381)
on Windows, Ubuntu, macOS Intel and macOS Apple Silicon. All eight jobs succeed.
Each host runs 165 CPU tests, including 88 script tests, formatting, Clippy with
warnings denied, native builds and seven executable script scenarios. Rust host
triples are verified in the logs. Linux additionally passes 13 explicit software
Vulkan GPU tests and native-window fixtures.

All four push-workflow script archives were downloaded and their ZIP SHA-256
digests verified. All twenty-eight complete JSON reports match local results.
The six previous reports remain unchanged. The seventh ClearMain report equals
the static scenario's complete state, with canonical sorted compact JSON SHA-256
`1ce5bca0ac9c84116ca63ce4421060361fcbcb2aa57c2bb61a84ab6dfc04ec76`.

The retained routing/table oracle checks 25 sequences and 627 finite snapshots,
plus one timeout-confirmed original clear-time hang that Rust reports as an
error. Separate source tests check queue timers/payloads, globals, nested/live
dispatch, message cursors, break state, budgets, cleanup and recovery. This is
bounded compatibility evidence, not execution of the original game program.

A local resource-dependent audit passes all 40 original ship roots and restores
368 sail masks through 2,204 script handlers and 1,105 entity notifications. All
80 complete event/layout reports match the preceding phase. The generated ship
presents three native Metal frames on the local Intel Mac. Hosted Windows/macOS
checks establish CPU/build behavior; physical GPU coverage for those additional
machines remains pending. Commercial resources and resource-dependent reports
remain outside Git. Full original game startup, module unloading and the
remaining native game API remain pending.
