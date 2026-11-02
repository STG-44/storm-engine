# Shared engine runtime foundation

`storm-runtime` owns one persistent script Runtime, its World/native entity
host and service frame sequence. `storm-engine` loads `engine.ini` and executes
that host without the ship preview. This initial executable is headless and
does not yet run the complete game. Native window/input, production classes,
rendering/audio and state loading still need to be attached to this same host.
The [static MODELR path](STATIC_MODELS.md) now adds bounded original model
messages and real GPU draws; full model/sea/game services remain pending.

## Startup and shared ownership

The startup reader implements root `program_directory` and `run` directives.
It keeps the first duplicate, trims spaces/tabs and retains inline semicolons
as original IFS values do. Other INIFILE/application directives are not yet
ported. Configured paths resolve beneath one explicitly selected game folder,
fold ASCII case and accept Windows separators on every target. Missing,
ambiguous, escaping, oversized and nonregular inputs return errors.

The embedding explicitly selects UTF-8 or Windows-1251 source decoding; UTF-8
BOM is recognized. This is a portable text adapter for the original byte-based
language, not proof of all original encodings/localization semantics. Existing
compiler/source budgets and unsupported language features remain unchanged.
Missing `PROGRAM` and unsupported grammar/functions/classes report their
path/source context rather than silently substituting preview implementations.

NativeHost provides the same World to script objects and entity construction,
attribute notifications and synchronous messages. Subsystems explicitly register
their real native factories and functions. No preview or probe class is included
in the engine executable. `Main` runs once on its first frame, preserving globals
for subsequent events. `Trace` uses an embedding-provided sink without retaining
an unbounded log. `GetTargetPlatform` retains the original `"pc"` value.

The original internal functions `LayerSetExecute`, `LayerSetRealize`,
`LayerAddObject`, `LayerDelObject`, `LayerFreeze` and `GetDeltaTime` now bind to
the host without user declarations. Integer layer arguments and unsigned native
priorities retain the original mapping. Invalid layer/type inputs return checked
errors rather than native assertions or failed scratch-register reads.

## Frame contract

The supported order follows `CoreImpl::Run`, `ProcessExecute`, `ProcessRealize`,
`ProcessRunStart/End`, `SERVICES_LIST` and the shared native layer constants:

1. Set frame delta and, once, call `Main`.
2. Process posted script events, then emit `frame`.
3. Start All services; a requested skipped frame returns here.
4. Start Execute services, dispatch entity Execute, end Execute services.
5. Start Realize services, dispatch entity Realize, end Realize services.
6. Drain checked deferred entity destruction, then end All services.

Services run newest first, matching the native service-list traversal. The skip
flag resets every frame and is checked after All run-start only. The initial
native layer types (0–14) are established; remaining layers stay Common.
Production service lookup/reference counts and dynamic service creation are not
yet implemented by this bounded service adapter.

Original clock globals/time scaling, pending state loading, Steam callbacks,
controls update/events, focus/background behavior and window-close handling
remain pending. They have defined insertion points in the native source; they
are not silently simulated by this host. A frame error faults the host and
prevents another frame from repeating partial side effects.

Explicit shutdown emits `ExitApplication` once and drains checked deferred
entity cleanup even if the event fails. A failed cleanup can be retried without
repeating that event. This bounded Rust shutdown is not the native `EraseAll`
algorithm, reset, serialization or complete service cleanup. Production callers
must call it; Rust Drop cannot report teardown errors.

## Reproduce

From the repository root:

```sh
cargo run -p storm-engine --locked -- \
  --game-dir rust/apps/storm-engine/fixtures/startup \
  --script-encoding utf8 --frame-ms 0,9,1,0
cargo test -p storm-runtime --locked
```

The startup fixture is newly written Storm source, not commercial game scripts.
It loads a second file through a Windows-style include and reports the exact
Main/posted/frame/delta/exit sequence. Native runtime tests create a test-only
entity, send a message through the shared attribute store, change attributes,
run both stages and delete it from a delayed event. They also cover newest-first
services, skip/lifecycle ordering, original layer operations/priority mapping,
shutdown errors, persistent globals, source-context failures and rooted legacy
source loading. Ordering assertions are derived from the native source; this
increment does not introduce a new full-C++ runtime differential oracle.

The next step is to extend static models with sea/camera and native window in this
same runtime, extending language/API compatibility alongside that scene. See
the [complete-engine sequence](ENGINE_ROADMAP.md). Full-game coverage remains
about 20% (roughly 15–25%); runnable fixtures are not complete game startup.


## Four-platform evidence, 2026-11-02

Code `6ebb5d15484e884c788344a6064c41a225103234` passes [PR run 41490786016](https://github.com/STG-44/storm-engine/actions/runs/41490786016)
on Windows x86_64, Linux x86_64, macOS Apple Silicon and macOS Intel. Complete
logs verify formatting, warnings-denied Clippy, native builds, target triples
and 420 CPU tests per host, including 135 interpreter and seven new runtime
tests. Each host also runs the configured `storm-engine` executable.

Four original artifact ZIP SHA256 digests are verified. All 144 complete JSON
reports over the existing 36 scenarios match local values exactly, and all four
new startup traces match the same Main/posted/frame/delta/exit line sequence.
All 36 preceding reports remain unchanged. Linux additionally passes 25 software
Vulkan tests and the existing 30 fleet-window frames. No GPU code changes in
this runtime increment; local Metal tests were not repeated.

Local startup also checks the external installed resource root: its configuration
correctly reports that the required PROGRAM directory is absent. No commercial
programs/resources are included in the fixture or repository. That missing data
and full language/native-service compatibility remain requirements for original
game acceptance. Production classes/window, controls/audio, state loading and
complete service lifecycle remain pending. The next uncommitted static-model
integration is excluded from this evidence. Whole-engine coverage remains about
20% (roughly 15–25%).
