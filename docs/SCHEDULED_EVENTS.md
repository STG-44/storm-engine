# Nested and frame-driven script events

`Event`, `PostEvent` and `ClearPostEvents` extend the
[immediate event runtime](SCRIPT_EVENTS.md). They support the original `l/f/s`
message subset and operate on the same persistent handler table, host and
attribute world. Static declarations, original program loading, `i/a/e/p` fields
and the full game API remain pending.

## Reproducible fixture

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/timed_events.c \
  --frame-ms 0,9,1,0 --report timed-script-state.json
```

The newly written fixture executes a nested `Event`, writes sail masks 274 and
548, then posts a 10 ms commit. The four explicit frame deltas deliver 0, 0, 1
and 1 queued events. The commit writes the total hole count and posts a
zero-delay cleanup, which runs on the following frame. The result is 6, four
completed source handlers, two posted events, four entity notifications and no
remaining queue entries. This is diagnostic code, not an original game handler.

`Runtime::advance_frame(delta_ms, host, limits)` advances handler statuses and
queued events using a uint32 millisecond delta. `process_frame()` retains its
earlier meaning of advancing handler statuses only. The native preview calls
`advance_frame` from unpaused rendered frames. Its clock keeps fractional
milliseconds between frames and excludes paused wall time. The CLI accepts up
to 128 explicit deltas for deterministic, GPU-free checks.

## Retained behavior

- `Event(name)` inherits the current message, including its shared cursor;
  handlers rewind it as usual. `Event(name, format, ...)` uses a separate message
  and clears the current message pointer on return, as the retained MESSAGE*
  wrapper does. An outer handler cannot call `GetEventData` afterward until its
  own handler boundary restores the parent message. Empty payload and missing
  message remain distinct.
- The break flag is shared. A nested dispatch resets it on entry, including for
  an event without handlers. An inner `EventsBreak` can suppress remaining outer
  handlers. Event calls return void; external `Runtime::emit` still returns the
  last non-void handler result.
- Message `l` requires an integer, including through a checked ref. Message `f`
  accepts float or int; an integer reached through ref converts that original
  variable to float. Message `s` requires a direct string value. Formats can be
  referenced strings. Extra values are evaluated and discarded after the
  fields consumed by the format; missing values and unsupported formats fail.
- `PostEvent(name, delay)` owns its optional payload. Negative signed delays
  retain their uint32 period bits; elapsed time wraps at 32 bits. The retained
  builtin constructs a message only with at least four arguments; a lone third
  argument is ignored.
- The first scheduler pass advances elapsed time and marks an entry processed
  without firing it, even if it is already due or has zero delay. Later passes
  check the period after adding the frame delta. A post appended during delivery
  can receive its preparation pass in that same frame.
- `DelEventHandler` also calls `ProcessTime(0)` on every queued entry, including
  unrelated names. Matching entries that are not due are removed; due and
  zero-period entries survive. This can prepare unrelated events early.
- `ClearPostEvents` invalidates the list immediately. End-of-frame cleanup keeps
  the original index-reset quirk, so a shifted first invalid entry can remain
  retained until another frame. Removing earlier queue entries during a callback
  also preserves the original loop's skipped-next-entry behavior.

The control flow comes from retained `COMPILER::ProcessEvent`, `CreateMessage`,
`DelEventHandler` and `ProcessFrame`, plus `S_EVENTMSG` and `POSTEVENTS_LIST`.
Source tests cover nested cursor/break behavior, field conversions, timing,
cancel/clear, posting during delivery, errors and bounded execution.

## Reference and bounds

```sh
python3 rust/crates/storm-script/reference/post_events/generate.py
```

The generator compiles the unedited timer/list headers and the original compiler
frame and cancellation loops under UndefinedBehaviorSanitizer. Only MESSAGE
storage, platform wrappers and native callback bodies are stubbed. Its 22
sequences contain 431 complete queue/delivery snapshots, including timer wrap,
callback posting/deletion, cancellation side effects and cleanup quirks. This
oracle checks the scheduler, not complete C++ compiler or game-program execution.
The earlier 521 event-table snapshots and 9,213 scalar reference cases remain.

The queue allows 256 retained entries and 1 MiB of charged names/field storage.
Invalid entries still consume that budget until cleanup. Message limits remain
128 fields and 4,096 bytes per string. One instruction budget covers each whole
frame, including queued and nested handlers and posts appended during delivery;
call/message depth remains bounded. Payloads are owned values with checked
references and no raw pointers. Unsupported field formats are rejected before
building their message.

Errors stop the dispatch/frame and preserve completed side effects. A due event
is invalidated before its handler runs, so an interrupted handler is not retried.
Cleanup still runs after a frame error; later calls receive fresh execution and
message state. The preview still invokes damage handlers after collision
traversal; original gameplay and full engine lifecycle compatibility remain
separate work.

## Local verification

All 120 CPU tests pass, including 43 script tests, the 431 scheduler snapshots
and the earlier 521 handler-table snapshots. Formatting, Clippy with warnings
denied and native workspace builds pass. The timed CLI report has canonical
JSON SHA-256 `09efcc4fc9b47b3f52fae4b2cbd05d6a7be40f73e258c896e30737065deb5fc4`
(sorted keys, compact separators). Both earlier standalone CLI reports remain
unchanged.

The generated ship preview presents three frames through local Metal with the
new scheduler hook. All complete event and layout reports from the 40-ship
resource audit equal the prior reports: 368 reconstructed masks, 2,204 immediate
handlers and 1,105 entity notifications. Resources and dependent reports stay
outside Git.
