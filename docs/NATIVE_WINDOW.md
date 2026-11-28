# Shared engine desktop window and TIMER

`storm-engine --window` drives the same configured Engine, script World, MODELR
entities and GPU scene owner as the headless path. It opens a winit window and
presents actual script-created static models. No preview entity or diagnostic
ocean is substituted. Complete original rendering, cameras, input/game controls,
audio, time globals/scaling, service lookup and gameplay remain pending.

## Retained application boundary

Original references are `src/apps/engine/src/main.cpp`,
`src/libs/window/src/sdl_window.cpp`, renderer `s_device.cpp` and core `timer.h`.
The native root engine.ini fields supply resolution, display, borders,
fullscreen, background policy, max_fps and vsync. Original defaults and decimal
integer-prefix/narrowing behavior are retained. Bare/empty first keys retain
native missing-value/default behavior, including when a valued duplicate follows.
Malformed/overflowing configuration and unsupported window dimensions are checked
errors; the complete INIFILE service is still a separate port.

The window is centered on the selected monitor, shown before game services start
and uses physical pixel dimensions. The portable fullscreen adapter is borderless;
native exclusive mode selection and full DPI/window script APIs remain pending.
Vsync chooses the corresponding wgpu presentation preference with its supported
platform fallback. Original max_fps integer millisecond division is retained.

Focus gates frame processing unless run_in_background is enabled. Original
CoreImpl::AppState only notifies Controls; no synthetic script AppState event is
emitted. Controls focus/reset and sound fade/background hooks remain pending.
Inactive processing waits as the native main loop does. Hidden/background
stepping is paced at 16 ms when presentation supplies no pacing, an explicit
desktop adapter rather than the original unconstrained polling loop.

Close emits `DestroyWindow` after initialization and exits the loop; common
shutdown emits `ExitApplication` once and drains entity cleanup. Platform script
events use the existing Runtime between frames. Native stage-to-script reentrancy
is not established by this API. Keyboard gameplay mappings, including the
original control activation/deactivation path, have not been replaced by demo
shortcuts.

Zero-size and occluded windows avoid invalid presentation. Resize, suboptimal,
outdated/lost surface and timeout paths preserve the shared CPU world. Suspend
releases window/device resources and rebuilds them on resume. Full native device
loss/reset/service restoration remains pending.

## Retained time state

The TIMER port accepts an explicit monotonic nanosecond elapsed provider. It
retains float conversion/truncation, the 100 ms cap, FPS counters/ring ordering,
the one-millisecond minimum and fixed-delta behavior, including the stored fixed
value surviving disable. FPS accounting precedes the minimum; fixed GetDeltaTime
does not overwrite the retained raw/real/high-precision state. Native core time
scaling, clock/date globals and profiler use of Ring are not yet connected.

The oracle compiles the complete unedited original header under UBSan. A namespace
adapter supplies a deterministic nanosecond clock while retaining standard chrono
types/operations. Sixteen profiles cover 1,040 complete scalar snapshots. Unsupported
unsigned elapsed conversion returns a checked error without changing timer state.
This is a numeric/provider boundary, not equivalence of every OS clock behavior.

Explicit headless --frame-ms remains a diagnostic input and may include zero.
Window mode uses TIMER; --window-fixed-delta exercises its original fixed-time
control. --window-frames closes after a number of actual presentations. Surface
readiness can cause more logical frames than presentations, so a window smoke
trace validates sequential frames and fixed deltas rather than assuming equality.

## Reproduce

From the repository root:

```sh
python rust/apps/storm-engine/fixtures/prepare_model_game.py generated-model-game
cargo run -p storm-engine --locked -- --game-dir generated-model-game \
  --script-encoding utf8 --window --window-frames 3 --window-fixed-delta 16
python rust/crates/storm-runtime/reference/timer/generate.py
cargo test -p storm-runtime --locked
```

The generator requires a new directory and uses the licensed upstream cannon
mesh, generated texture pixels and newly written source. It includes no commercial
game programs/resources. A local Metal run presents three model frames and checks
one load plus sequential fixed-delta frame callbacks, DestroyWindow and
ExitApplication. Local checks pass 429 CPU tests and 26 Metal GPU tests; all 36
preceding complete scenario reports and the twelve-line configured startup trace
remain unchanged. The TIMER oracle regenerates byte-identically. Four-host CI for this increment is complete (see below). The Linux workflow adds the same model/window path under Xvfb
and software Vulkan. Windows/macOS physical input, focus, suspend and surface
recovery coverage remains pending beyond documented local checks.

This extends [shared startup](ENGINE_RUNTIME.md) and [static models](STATIC_MODELS.md).
The next scene integration is original sea/camera through this same host, while
full script/native API work continues toward sea and land gameplay. Whole-engine
coverage remains about 20% (roughly 15–25%).


## Four-host evidence, 2026-11-05

Source 3c1f709d649963d456c068125e28cf0699a25a48 passes all four native jobs in
[PR run 42073388748](https://github.com/STG-44/storm-engine/actions/runs/42073388748): Windows
x86_64, Linux x86_64, macOS Apple Silicon and macOS Intel. Logs confirm formatting,
warnings-denied Clippy, builds and 429 CPU tests per host (135 interpreter,
15 runtime). Four original artifact ZIP SHA256 digests are verified; all 144
complete reports over the existing 36 scenarios and all four twelve-line startup
traces match the local baseline. The TIMER oracle regenerates byte-identically.

Linux additionally passes 26 software Vulkan tests, three actual shared-engine
window presentations and 30 fleet-window presentations. Thirty other existing
preview-window presentations also pass. The archived engine trace checks model
load, three sequential delta=16 callbacks and one DestroyWindow/ExitApplication
pair. Local Metal passes 26 GPU tests and three actual engine presentations;
surface readiness permits more logical updates than presentations.

The first window run 41930273608 passed three hosts but failed the Linux trace
assertion because xvfb-run combined diagnostics with script stdout. The follow-up
isolates child stdout before that wrapper and always archives the trace. This
correction is included in the passing source above. Script lines are validated
without filtering. Native ARM CPU tests now execute successfully for this source;
the earlier model source retains its separate historical hosted-runner gap.

Physical Windows/ARM GPU, input/audio and full-game acceptance remain pending,
along with native sea/camera and complete game services. Subsequent local
compiler/library work is excluded from this validation cohort.
