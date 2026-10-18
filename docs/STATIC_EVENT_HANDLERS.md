# Static script event declarations

The source compiler accepts the original declaration:

```c
#event_handler("Damage", "OnDamage");
#event_handler(COMMIT_EVENT, "OnCommit")
#define COMMIT_EVENT "Commit"
```

The event argument is a string literal or a string `#define`. Defines and
function names can resolve forward across the complete appended source set.
The handler argument must be a quoted function name; a string define there is
not accepted by the retained compiler. Names fold ASCII case. The optional
semicolon remains a real empty statement.

Declarations register during compilation, in source encounter order, including
inside uncalled functions, false branches and code after `return`. They emit no
runtime instruction. A declaration alone does not supply an executable
conditional/loop body; its following statement or semicolon does. Includes keep
the existing program-root-relative append order. Repeated event/function pairs
keep one handler at the original position.

`Runtime` installs these handlers once, before its first call, event or frame.
No initialization function or dynamic `SetEventHandler` call is required.
Persistent calls do not reinstall disabled handlers. Each one-shot
`Program::call` receives fresh globals and a fresh initial static handler table.
Separate runtimes remain isolated. Static and dynamic entries share the existing
128-event and 512-handler bounds and instruction budgets during dispatch.

## Static lifetime and bounded compatibility

The original event table retains an entry's initial static flag when a duplicate
is registered. `DelEventHandler` marks either kind inactive immediately and
cancels matching queued events. Frame cleanup can erase dynamic entries, but
refuses to erase static ones. A deleted first static entry therefore remains
inactive across frames. Re-registering its pair restores it immediately, even
with a nonzero activation flag, and keeps its static flag and position.

The retained `ProcessFrame` resets its loop index after every deletion attempt,
including a refused static deletion. A deleted static entry after index zero can
make that loop repeat forever. Rust detects this case and returns an error from
`Runtime::process_frame` or `Runtime::advance_frame`. Re-registering the pair can
restore normal processing. This deliberately replaces a confirmed original hang
with a bounded failure. Other deletion/index-reset behavior remains unchanged.

Handler functions must exist and take no parameters, matching the current
bounded event dispatcher. Invalid declarations and unavailable handlers reject
compilation with a source location. The retained parser can register before
discovering a missing closing parenthesis, and its void `SetEventHandler` can
log a failed function lookup without aborting the parser. Rust exposes no
partially compiled runtime from either failure.

Segment unloading, `ClearEvents`, cached/saved program declarations, the full
native game API and complete original game startup remain pending. This phase
adds newly written diagnostic scripts and no commercial resources.

## Executable fixture and independent reference

```sh
cargo run -p storm-script --locked -- \
  --script-root rust/crates/storm-script/fixtures/static_ship MAIN.C \
  --frame-ms 0,0 --report static-script-state.json
```

The four-file static fixture produces exactly the same complete JSON report as
`multi_ship`, including its normalized source list. It registers damage before
its event define, resolves a commit-event define from an appended file, includes
a duplicate declaration, and declares commit inside a false branch of an uncalled
function. Two immediate handlers store masks 548/274 and six holes. The posted
commit emits three native entity notifications on the second supplied frame.

`reference/static_handlers/generate.py` copies four retained TOKEN/S_EVENTTAB
source and header files unedited. It extracts the complete compiler
`EVENT_HANDLER` case and `SetEventHandler` method verbatim. Function/define
lookup, cache/error/platform storage and the outer scan are explicit adapters;
the complete original compiler/interpreter does not run.

C++20 with fail-fast UndefinedBehaviorSanitizer generates 16 declaration cases
and nine static-lifetime sequences with 55 snapshots of function order, status
and static flags. One final isolated reference process confirms the original
infinite frame loop with a one-second timeout. Rust checks the finite states and returns an error for that
last frame. Core files and extracted-body hashes are recorded in golden data.
Separate source tests cover initialization timing, dead code, control-flow
semicolons, persistent/isolated runtimes, include order, file diagnostics,
resource limits, dispatch error recovery and the native ship fixture.

## Four-platform evidence, 2026-10-19

Code `6a6c7a32c8b08de373416ab32b2bf5de5dfc574a` passes the
[PR workflow](https://github.com/STG-44/storm-engine/actions/runs/39170644575)
and [push workflow](https://github.com/STG-44/storm-engine/actions/runs/39170640157)
on Windows, Ubuntu, macOS Intel and macOS Apple Silicon. All eight jobs succeed.
Each host runs 156 CPU tests, including 79 script tests, formatting, Clippy with
warnings denied, native builds and six executable script fixtures. Linux also
passes 13 explicit software Vulkan GPU tests and native-window fixtures.

The four push-workflow script-state archives were downloaded and their ZIP
SHA-256 digests verified. All twenty-four complete JSON reports match local
results. All five preceding script reports are unchanged. The static fixture's
complete report equals the preceding multi-file report, with canonical sorted
compact JSON SHA-256
`1ce5bca0ac9c84116ca63ce4421060361fcbcb2aa57c2bb61a84ab6dfc04ec76`.

The retained-C++ reference generates six accepted and ten rejected declarations,
55 finite static-table snapshots and one timeout-confirmed original frame hang.
Rust checks these cases, plus forward includes, initialization timing, malformed
control-flow bodies, limits, frame error recovery and native attribute writes.
A local resource-dependent audit passes all 40 original ship roots and 368 masks
through 2,204 handlers and 1,105 entity notifications. All 80 complete event/layout
JSON reports match the preceding phase. The generated native ship presents three
Metal frames on the local Intel Mac. Hosted Windows/macOS checks establish
CPU/build behavior; physical GPU coverage for those additional machines remains
pending. Commercial resources and resource-dependent reports remain outside Git.
Full original game startup, native APIs and dynamic modules remain pending.
