# Storm Engine: Rust port

Started: 2026-10-17.

## Goal

Gradually replace the C++ engine with Rust while preserving compatibility with Sea Dogs: To Each His Own (Корсары: Каждому своё). Target native desktop builds for macOS, Windows, and Linux.

## Architecture decisions

- Rust for the new engine implementation.
- wgpu for a shared renderer: Metal on macOS, Vulkan on Linux, Vulkan or DirectX 12 on Windows.
- WGSL for new shaders.
- Preserve the existing script language and engine API before considering script migration.
- Preserve the GPL-3.0 licensing and upstream attribution. Game resources must be provided separately.
- Keep the original C++ implementation as a behavioral reference during migration.

## Development history

- `develop` preserves the upstream baseline.
- `rust-port` is the integration branch for the port.
- Work is recorded in focused commits with validation results. Completed milestones should have reproducible commands and platform evidence.
- Upstream baseline: `a68f43df197a50c5177f40209abc109f01f4b114` from `storm-devs/storm-engine`.
- Public development review: [PR #1](https://github.com/STG-44/storm-engine/pull/1), ready for review into this fork's `develop`; upstream is archived.

## Milestones

- [x] Create the GitHub fork and retain upstream branches, tags, and history.
- [x] Record scope and architecture in the first port commit.
- [x] Create the Rust workspace, native window, GPU diagnostics, and offscreen rendering.
- [x] Verify CI builds for all three operating systems.
- [x] Inspect original GM/TX headers with bounded reads and format tests.
- [x] Decode static GM 1.05 render data and 2D TX mip pixels with bounds checks.
- [x] Display an original ship with textures, static sails, preview water, and an orbit camera on local macOS Metal.
- [x] Add triangular sails, deterministic preview wind deformation, and pause/wind controls.
- [x] Add a fixed-step planar sailing prototype with wind drive, rudder, water resistance, and physical-key input.
- [x] Share an analytic sea between CPU/GPU and add damped heave, pitch and roll.
- [x] Gather/unfurl both sail topologies and drive the ship with actual deployment.
- [x] Restore original three sail sets, locator ropes/shrouds and national flags.
- [x] Preview original animated deck crew, fire, mast/hull damage and persistent sail holes.
- [x] Complete the current ship/sailing prototype audit and record its gameplay gaps.
- [ ] Display an original ship, sea, and movable camera. Verify the same scene on all three operating systems.
- [ ] Port animation, input, audio, and platform file handling.
- [ ] Port [weather and the day/night cycle](WEATHER_ROADMAP.md).
  - [ ] Restore weather state, scripted presets and transitions over game time.
  - [ ] Port sky/clouds, sun/moon/stars, lighting, fog and visibility.
  - [ ] Port rain, storms, lightning and thunder with original effects/audio.
  - [ ] Connect wind/weather to sails, ship motion and the original sea.
  - [ ] Integrate weather with locations, gameplay scripts and save/load.
- [ ] Port the script runtime, entity messages, and game-facing API.
  - [x] Add the [native entity/message foundation and sail event observer](RUNTIME_FOUNDATION.md).
  - [x] Port the [native attribute tree and entity bindings](ATTRIBUTES.md); roundtrip sail masks through original character attribute paths.
  - [x] Add a bounded executable source subset with native attribute bindings ([scope](SCRIPT_RUNTIME.md)).
  - [x] Add object/ref/aref expressions, first original attribute functions and entity notifications ([scope](SCRIPT_OBJECTS.md)).
  - [x] Bind `IsEntity`, `DeleteClass` and persistent `EntityUpdate` ([scope](ENTITY_SCRIPT_API.md)).
  - [x] Add native class factories/Init and `CreateEntity`/`CreateClass` ([scope](ENTITY_CREATION.md)); original native classes remain pending.
  - [x] Bind synchronous `SendMessage` with original field types and full 64-bit results ([scope](ENTITY_MESSAGES.md)).
  - [x] Pass checked `e` variable cells to native messages, including output parameters, arrays and retained globals ([scope](SCRIPT_VARIABLE_MESSAGES.md)).
  - [x] Accept original `&variable` / `&array[index]` reference expressions, including direct entity creation ([scope](ADDRESS_REFERENCES.md)).
  - [x] Execute [immediate script handlers for sail damage](SCRIPT_EVENTS.md).
  - [x] Add [nested calls and the original frame-driven event queue](SCHEDULED_EVENTS.md).
  - [ ] Port remaining attribute encodings/conversions, cached script programs, complete script compilation/execution and game handlers.
- [ ] Integrate locations, interface, sea combat, land combat, and persistence.
  - [ ] Load land locations and implement player movement/collision.
  - [ ] Support player movement on ship decks and boarding transitions.
  - [ ] Port character combat, enemies and encounter logic.
  - [ ] Complete naval SHIP logic, combat, interface and saves.
- [ ] Verify representative gameplay and publish platform builds.

A rough functional coverage estimate is recorded in [port status](PORT_STATUS.md):
15–20% toward original-game compatibility, and about 70% for the ship demo
including remaining polish and platform verification. These are planning estimates.

## Current evidence and prerequisites

The renderer foundation is implemented in `rust/`; see [run and validation commands](../rust/README.md).
The diagnostic sea is new test code, separate from the original Storm sea simulation.
Static ship preview, approximate sailing and damped wave response are implemented. Original
game loading, calibrated ship simulation, and gameplay remain pending.

Verified locally on 2026-10-17:

- macOS 15.7.8, Intel x86_64, Rust 1.96.0.
- Metal rendering on AMD Radeon Pro 555X; adapter discovery also finds Intel UHD Graphics 630.
- Formatting and Clippy pass with warnings denied.
- CPU format/shader/dynamics validation and all nine explicit GPU tests pass.
- The native diagnostic and original ship windows each present three frames and exit successfully.
- A 1280 x 720 frame is saved through the offscreen pipeline.
- Barkentine1 assembles from 20 GM files, with six texture slots and
  twelve sails (two triangular): 38 draw meshes, 19,431 triangles including rigging. Wind deforms
  the cloth interior with pinned boundaries. A 1280 x 720 capture is saved outside Git.
- GPU fixture checks cover texture sampling, geometry visibility with the depth
  pipeline, zero-normal fallback lighting, camera changes, and invalid cameras.
- The first sailing case reaches 5.03 knots and changes course by 23.62° in 40 s.
  [The prototype notes](SAILING_PROTOTYPE.md) record controls, units, and remaining scope.

The [Rust CI run for commit 7c890862](https://github.com/STG-44/storm-engine/actions/runs/39002786501)
passed all four jobs on 2026-10-17:

- Ubuntu 24.04 x86_64: formatting, Clippy, CPU tests, native build, Vulkan GPU
  readback/animation test, PNG capture, and three frames in an X11 window under Xvfb.
  The software adapter was Mesa llvmpipe (LLVM 20.1.2).
- Windows 2025 x86_64: formatting, Clippy, CPU tests, and native build.
- macOS 15 Apple Silicon: formatting, Clippy, CPU tests, and native build.
- macOS 15 Intel: formatting, Clippy, CPU tests, and native build.

CI publishes the native executables and the Linux Vulkan capture as workflow artifacts.
Windows and hosted macOS checks do not exercise a physical GPU or native window.
Native platform support must also be verified on representative physical GPUs.
Original model and texture resources are now available locally, outside Git.
The native `storm-assets` tool can inspect a file or scan a resource directory;
see [the asset commands](../rust/README.md#original-game-resources).
`inspect` and `scan` read at most 72 bytes per file and check minimum declared
table lengths. `decode` additionally reads static geometry, names, materials,
labels, vertices, indices, and complete 2D mip pixels. Supported layouts and
explicit limits are documented in the run instructions. BSP collision traversal,
animation beyond the crew subset, older GM versions, original material/sea effects, original
rigging dynamics, full ship physics, and gameplay remain pending. Full decoding has been exercised on
all 40 available ship roots and the upstream fixture; the previous whole-resource scan remains
header-only evidence.

The [first original-resource inspection results](ASSET_VALIDATION.md) record
4,129 accepted GM headers, 5,732 accepted TX headers, and seven GM compatibility
cases. The inspector's [four-platform CI run](https://github.com/STG-44/storm-engine/actions/runs/39004859635)
passed for code commit `ca6a70b6`.

The [original-model preview CI run](https://github.com/STG-44/storm-engine/actions/runs/39006901573)
passed all four jobs for code commit `096c2519` on 2026-10-17. All platforms pass
formatting, Clippy, 11 CPU tests, upstream cannon decoding, and native builds.
Linux additionally passes both GPU tests on software Vulkan (Mesa llvmpipe),
captures both scenes at 641 x 361, and presents three frames in each scene under
Xvfb. [The asset evidence](ASSET_VALIDATION.md#first-native-ship-preview-2026-10-17)
includes the CI cannon capture. Native executables and the inspector for each
platform are available as workflow artifacts. Original ship rendering has only
been verified locally through Metal; original resources stay outside Git/CI.

The [wind preview CI run](https://github.com/STG-44/storm-engine/actions/runs/39008381018)
passed all four jobs for code commit `83ea00e1`. It passes 15 CPU tests and three
GPU tests, including persistent cloth buffer updates through software Vulkan.
The procedural quad/triangle fixture also renders a PNG and presents three
paused native window frames on Linux. [Sail notes](SAIL_PREVIEW.md) include the
native Metal animation and the remaining rigging/physics scope.

The [sailing dynamics CI run](https://github.com/STG-44/storm-engine/actions/runs/39009937364)
passed all four jobs for code commit `dff737ea`: 21 CPU tests, formatting,
Clippy, native builds, and upstream geometry decoding on each platform. Linux
also passes all four GPU tests through software Vulkan, a sailing transform
capture with JSON state, and three frames in its paused native window.
[Sailing notes](SAILING_PROTOTYPE.md) record controls, reproducible cases, and
the difference between this approximate prototype and original gameplay parity.

[Wave response notes](WAVE_RESPONSE.md) describe the shared height/gradient
field, hull surface fit, rigid pitch/roll, water depth and remaining limitations.
The local original-ship capture includes all 12 sails. Local validation passes
24 CPU tests and six Metal GPU tests, including CPU/WGSL agreement and an
emerging/submerged geometry check. CI adds a separate wave capture/state export
and paused native window using the small upstream cannon fixture.

The [wave response CI run](https://github.com/STG-44/storm-engine/actions/runs/39010786290)
passed all four jobs for code commit `c3b2520e`: 24 CPU tests and native builds
on Windows, Linux and both Mac architectures, plus six GPU tests and native
wave capture/window checks through Vulkan on Linux. Original ship captures stay
local; the public workflow artifacts contain only the upstream fixture.

[Sail gathering](SAIL_FURLING.md) adds a rate-limited deployment state shared by
propulsion and geometry. Quad and triangular sails gather towards the original
attachment edge, reuse buffers and preserve UVs. Local validation passes 27 CPU
tests and seven Metal GPU tests, including shrinking/restoring both cloth surfaces.

The [sail gathering CI run](https://github.com/STG-44/storm-engine/actions/runs/39011763235)
passed all four jobs for code commit `98c8148a`: 27 CPU tests and native builds
on Windows, Linux and both Mac architectures. Linux passes seven GPU tests,
captures partially gathered quad/triangle cloth, opens its paused window and
checks an exported rig state with actual deployment 0.5 and target zero.

[Original sail sets](ORIGINAL_SAIL_MODES.md) restore three-state selection and
model-specific membership. All 40 available ship roots (776 GM files, 368 sails)
match independent C++-rule classification. Local checks pass 30 CPU/eight GPU tests.
The [original-set CI run](https://github.com/STG-44/storm-engine/actions/runs/39026148582)
passes all four platform jobs, including eight Vulkan GPU tests on Linux.

[Rigging preview](RIGGING_PREVIEW.md) adds rope ties, shrouds and flag animation.
All 40 available ship roots decode with generated rigging; local validation passes
35 CPU tests and nine Metal GPU tests. Commercial assets/captures remain external.
The [rigging CI run](https://github.com/STG-44/storm-engine/actions/runs/39026983401)
passes all four jobs for `a132be4c`, with 35 CPU tests on each platform and nine
software Vulkan GPU tests plus procedural rigging capture/window checks on Linux.

[Ship flag selection](FLAG_SELECTION.md) adds the published TEHO national and
special-flag rules, explicit context controls, bounds-checked atlas changes and
live switching in the existing scene. Local checks pass 41 CPU/ten Metal GPU
tests. Character/group lookup and gameplay script integration remain pending.
The [flag-selection CI run](https://github.com/STG-44/storm-engine/actions/runs/39047481959)
passes all four platform jobs, including 41 CPU tests per platform and ten
Vulkan GPU tests plus national-flag capture/window checks on Linux.


[Ship presentation](SHIP_PRESENTATION.md) adds bounded AN/two-bone GM readers,
original walking crew/routes, fireplace flame/smoke, mast grouping and hull
sections. Swept test shots damage persistent 10/12-sector sail masks and reduce
absolute propulsion area. Before proceeding to entities and the script VM,
validate ships and sailing together and record the remaining combat/physics
scope. This does not establish original battle or sailor AI parity.


The [ship presentation CI run](https://github.com/STG-44/storm-engine/actions/runs/39050610912)
passes all four jobs for `dd3e12a3`: 56 CPU tests per platform, native builds,
formatting and Clippy. Linux additionally passes 13 software Vulkan GPU tests,
generated crew/damage capture and native window checks, and lower speed after
sail damage. Full textured loading and three-sector test shots pass on all 40
original ships (776 GM files, 368 sails). All 40 also render through local Metal;
the labelled contact sheet was reviewed. Original crew loads for 37 matching
route files. See [the presentation scope](SHIP_PRESENTATION.md) for missing
routes, locator diagnostics and combat/physics limits. The current prototype
validation gate is complete; game entities/messages and the script runtime are
next, with the remaining gameplay work explicitly recorded.


[Original sail damage](ORIGINAL_SAIL_DAMAGE.md) replaces the initial procedural
circles with the game's alpha mask, original full-resolution index partition,
UV/sampler rules and coarse collision behavior. A retained-C++ reference checks
7,934 cases independently of Rust. Script events and original wind/rolling
integration remain separate work; this change does not claim full sail/VM parity.

The [first executable script subset](SCRIPT_RUNTIME.md) compiles functions,
expressions, conditions and loops to private native stack instructions. A
GPU-free sail-mask fixture executes script calls and changes the ported attribute
tree through explicit preview bindings. Scalar parity is checked against 5,675
cases generated from retained C++ methods. That first milestone left object/ref
syntax for the following [object/attribute integration](SCRIPT_OBJECTS.md).
Original game programs, the full native API and damage-event handlers remain pending.

The [script subset CI run](https://github.com/STG-44/storm-engine/actions/runs/39118509827)
passes all four hosts for `a128f186`: 84 CPU tests, native builds and execution of
the new script/attribute fixture. Its four downloaded reports are identical to
the local result. Linux additionally passes 13 software Vulkan GPU tests and
native-window fixtures. See [the verified scope](SCRIPT_RUNTIME.md#four-platform-evidence-2026-10-18)
for reference cases, limits and the original game features still pending.

The [object/reference integration](SCRIPT_OBJECTS.md) adds `object`, `ref`,
`aref`, original literal/dynamic attribute paths and the first original internal
attribute/conversion functions. Code `27222bf3` passes all four hosts in the
[object script CI run](https://github.com/STG-44/storm-engine/actions/runs/39120759769):
97 CPU tests, builds, formatting, Clippy and both executable script fixtures.
Both complete downloaded reports match the local results on all hosts. Linux
also passes 13 software Vulkan GPU tests and native-window fixtures. The C++
reference now covers 9,213 scalar/reference/conversion/format cases and six
attribute operation sequences. See [verified scope](SCRIPT_OBJECTS.md#four-platform-evidence-2026-10-18)
for object ownership, notifications and the script-event integration still pending.


The [original sail damage CI run](https://github.com/STG-44/storm-engine/actions/runs/39077873810)
passes all four platform jobs for `9de18547`: 58 CPU tests including 7,934 cases
from retained C++ methods, plus 13 Vulkan GPU tests and native fixtures on Linux.
Local Metal checks exercise the actual original mask; all 40 original ship roots
load it and their INI settings. Original wind/rolling and script event integration
remain distinct from the now-portable damage material/collision implementation.

The [immediate script event integration](SCRIPT_EVENTS.md) routes preview shots
through SetEventHandler/GetEventData handlers into persistent ship attributes.
The native observer now counts notifications; source code writes the masks.
A retained-C++ event-table oracle checks 521 status snapshots. Full original
game handlers, nested/delayed events and side effects during collision traversal
remain pending.

The [immediate event CI run](https://github.com/STG-44/storm-engine/actions/runs/39138395313)
passes all four hosts for `97781754`: 106 CPU tests, native builds, formatting,
Clippy and executable sail event handlers. Linux additionally passes 13 software
Vulkan GPU tests and native windows. A local audit executes 2,204 handlers across
40 original ship roots and reconstructs all 368 masks through script-written
attributes, with 1,105 entity notifications. The textured Barkentine1 capture
also succeeds through Metal. See [event scope and evidence](SCRIPT_EVENTS.md).

[Nested and frame-driven events](SCHEDULED_EVENTS.md) add Event, PostEvent and
ClearPostEvents with original timer/cancellation/cleanup behavior and bounded
owned payloads. A retained-C++ scheduler oracle checks 431 snapshots. The native
preview advances the queue on unpaused rendered frames, and a GPU-free timed
script fixture checks deterministic nested and delayed attribute writes.

The [nested/timed event CI run](https://github.com/STG-44/storm-engine/actions/runs/39142258576)
passes all four hosts for `abcbd096`: 120 CPU tests, native builds, formatting,
Clippy and three executable script fixtures. Linux also passes 13 software Vulkan
GPU tests and native windows. All twelve downloaded source-fixture reports equal
local results, with verified archive digests. The 40-ship event/layout audit
remains identical to the earlier reports. See [scheduler evidence](SCHEDULED_EVENTS.md#four-platform-evidence-2026-10-18).

[Persistent globals and arrays](SCRIPT_GLOBALS.md) support indexed characters,
objects and checked references across calls, nested events and scheduler frames.
The original GetArraySize/SetArraySize names operate through explicit refs.
A retained-C++ scalar-array oracle checks 444 snapshots, and a fourth diagnostic
script defers two characters' sail masks until a later frame before notifying
the visible ship. Full original program/module loading remains pending.

The [global/array CI run](https://github.com/STG-44/storm-engine/actions/runs/39144685224)
passes all four hosts for `f5ad68bf`: 133 CPU tests, formatting, Clippy, native
builds and four source fixtures. Linux additionally passes 13 software Vulkan
GPU tests and native windows. All sixteen downloaded push-workflow source
reports match local JSON, with four ZIP digests verified. The complete 40-ship
event/layout audit remains unchanged. See [global storage evidence](SCRIPT_GLOBALS.md#four-platform-evidence-2026-10-18).

[Multi-file source loading](SCRIPT_SOURCES.md) adds an explicit SourceLoader and
rooted UTF-8 directory loader, typed define constants, append-order includes and
source file/line diagnostics. Named array dimensions require earlier integer
defines. A four-file ship diagnostic reproduces the preceding single-file state.
An independent source-registration oracle runs unedited retained lexer/tables
and extracted compiler include/define/append bodies in 22 cases under UBSan.
Dynamic segment/native-library loading and complete original programs remain
pending.


The [source-loading CI run](https://github.com/STG-44/storm-engine/actions/runs/39167565014)
and its [push run](https://github.com/STG-44/storm-engine/actions/runs/39167561073)
pass all four hosts for `8def27a1`: 146 CPU tests, including 69 script tests,
formatting, Clippy, native builds and five source fixtures. Linux additionally
passes 13 software Vulkan GPU tests and native windows. All twenty downloaded
source reports match local JSON, with four ZIP digests verified. The 22-case
retained-C++ registration oracle includes binary64-to-binary32 literal rounding.
The complete 40-ship audit remains unchanged, and the local generated ship
presents three Metal frames. See [source-loading evidence](SCRIPT_SOURCES.md#four-platform-evidence-2026-10-19).

[Static event declarations](STATIC_EVENT_HANDLERS.md) add compile-time
`#event_handler`, including forward string defines, handlers in uncalled/dead
code, append-order registration and retained static flags. Runtime installs them
before its first entry; one-shot calls receive fresh tables. An independent
reference uses unedited TOKEN/S_EVENTTAB and compiler declaration/registration
bodies: 16 declaration cases, 55 lifetime snapshots and one isolated confirmed
frame hang, which becomes an explicit error in Rust. A sixth executable fixture
reproduces the complete multi-file ship report without dynamic registrations.

The [static-handler CI run](https://github.com/STG-44/storm-engine/actions/runs/39170644575)
and its [push run](https://github.com/STG-44/storm-engine/actions/runs/39170640157)
pass all four hosts for `6a6c7a32`: 156 CPU tests, including 79 script tests,
formatting, Clippy, native builds and six executable script fixtures. Linux also
passes 13 software Vulkan GPU tests and native windows. All twenty-four downloaded
source reports match local JSON, with four ZIP digests verified. The complete
40-ship event/layout audit remains unchanged and the generated local ship
presents three Metal frames. See [static-handler evidence](STATIC_EVENT_HANDLERS.md#four-platform-evidence-2026-10-19).

[ClearEvents](CLEAR_EVENTS.md) adds source and native embedding entry paths for
retained dynamic-handler cleanup. Static flags, globals and posted messages
survive; disabled statics, skipped deletions and name-budget retention remain.
An independent reference routes through unedited event tables and three retained
native/core/compiler bodies: 25 sequences, 627 state snapshots and one confirmed
clear-time hang, replaced by a bounded Rust error. A seventh executable scenario
clears an unwanted dynamic ship handler while preserving the queued static
commit and the preceding complete report.

The [ClearEvents CI run](https://github.com/STG-44/storm-engine/actions/runs/39172045516)
and its [push run](https://github.com/STG-44/storm-engine/actions/runs/39172042381)
pass all four hosts for `d63efa99`: 165 CPU tests, including 88 script tests,
formatting, Clippy, native builds and seven executable script scenarios. Linux
also passes 13 software Vulkan GPU tests and native windows. All twenty-eight
downloaded source reports match local JSON, with four ZIP digests verified.
The complete 40-ship audit remains unchanged and the generated local ship
presents three Metal frames. See [ClearEvents evidence](CLEAR_EVENTS.md#four-platform-evidence-2026-10-19).


[Script entity bindings](ENTITY_SCRIPT_API.md) add `IsEntity`, `DeleteClass` and
persistent `EntityUpdate`, including pending deletion callbacks and checked
64-bit identity through objects/refs/arefs. The retained native/DATA/MOVE oracle
checks 32 sequences / 560 snapshots. Factories/Init, script creation and
`SendMessage` are the next lifecycle work.

The [entity API CI run](https://github.com/STG-44/storm-engine/actions/runs/39173977578)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39173974555)
pass all four hosts for `bc8efb67`: 177 CPU tests, including 100 script tests,
plus all 13 explicit Linux software Vulkan GPU tests. Four ZIP digests and all
32 complete standalone fixture reports are verified against local results.
The seven preceding reports and all 80 resource-dependent fleet reports remain
unchanged. See [scope and evidence](ENTITY_SCRIPT_API.md#four-platform-evidence-2026-10-19).


[Native entity creation](ENTITY_CREATION.md) adds registered class factories,
pre-Init identity/attribute binding, nested construction, deferred rejection
cleanup and `CreateEntity`/`CreateClass`. References update caller identity;
plain object arguments retain their original stack-copy semantics. Class
queries now preserve allocation-stamp order after slot reuse. Checked callback
borrows and bounded initialization errors have explicit documented differences
from unsafe C++ re-entry and thrown initializers.

The [creation CI run](https://github.com/STG-44/storm-engine/actions/runs/39175675359)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39175672054)
pass all four hosts for `044efa10`: 197 CPU tests, including 110 script tests,
plus all 13 explicit Linux software Vulkan GPU tests. The native C++ oracle
checks 15 sequences / 404 complete snapshots. Four archive digests and all
36 complete scenario reports are verified; the eight preceding reports and all
80 fleet reports remain unchanged. Original native game classes and script
`SendMessage` remain pending.


[Original address references](ADDRESS_REFERENCES.md) add `&variable` and
`&array[index]`, including `CreateEntity(&ship, class)`. References resolve
before later argument side effects; arrays and arefs keep checked identity,
index evaluation and weak lifetimes. Six source tests check these contracts.

The [address CI run](https://github.com/STG-44/storm-engine/actions/runs/39202728588)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39202725517)
pass all four hosts for `f48256f1`: 203 CPU tests, including 116 script tests,
plus all 13 explicit Linux software Vulkan GPU tests. Four ZIP digests and all
40 complete scenario reports match local results. The tenth AddressMain report
exactly equals the ninth creation report; all nine preceding reports and all
80 fleet reports remain unchanged. Full original classes/programs and script
message pointers remain pending.


[Script entity messages](ENTITY_MESSAGES.md) add synchronous `SendMessage`,
original `l/p/f/i/a/s` fields and opaque full-width `ptr` results. Attribute
payloads use the checked runtime store. The C++ reference checks cover 53
message cases and 208 DATA pointer cases. Script-variable `e` fields and the
original commercial class/message API remain pending.

The [message CI run](https://github.com/STG-44/storm-engine/actions/runs/39204491289)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39204487728)
pass all four native hosts for `1df6617e`: 212 CPU tests, including 125 script
tests, plus all 13 explicit Linux software Vulkan GPU tests. Four verified
ZIP digests and all 44 complete scenario reports match local results. All ten
preceding reports remain unchanged; the eleventh preserves a complete 64-bit
message result and checks pending deletion.


[Native variable message fields](SCRIPT_VARIABLE_MESSAGES.md) now pass `e`
output parameters, including scalars, checked arrays, object/aref attributes
and complete entity IDs. Handles follow actual local/global/array lifetimes;
plain stack temporaries expire after the send. The original binding and ten
DATA getter/setter methods match 120 complete C++ result snapshots under UBSan.
Event e fields and the remaining VDATA/commercial API are still pending.

The [variable-message PR run](https://github.com/STG-44/storm-engine/actions/runs/39207766932)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39207762638)
pass all four native hosts for `59c82681`: 223 CPU tests, including 135
script tests, plus all 13 explicit Linux software Vulkan GPU tests. Four
verified ZIP digests and all 48 complete scenario reports match local results.
All eleven preceding reports remain unchanged; the twelfth checks native
output parameters and retains a global across synchronous sends.

[Original SHIP sail messages](SHIP_SAIL_MESSAGES.md) now store and return the
raw float32 command through the checked native `e` bridge. Both handlers also
control the real preview sailing state; the raw request, selected sail set and
gradually deployed canvas remain separate. A startup source fixture exercises
all three sets and deletion. The C++/DATA reference covers 20 cases, including
signed zero, threshold neighbors and non-finite patterns. Full SHIP mounting,
character binding and the remaining native messages are still pending.


The [SHIP sail-command PR run](https://github.com/STG-44/storm-engine/actions/runs/39210367729)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39210360784)
pass all four native hosts for `a62c560e`: 231 CPU tests, including 135
interpreter tests, plus 13 explicit Linux software Vulkan GPU tests and native
windows. Four archive digests and all 52 complete scenario reports are verified.
The twelve preceding reports remain unchanged. The phase's 40-ship resource
audit reproduces all 80 previous layout/event reports; final code also renders
original ArabellaShip1 through offscreen Metal with scripted sail commands.
Local native-window checks timed out; see [scope and evidence](SHIP_SAIL_MESSAGES.md#four-platform-evidence-2026-10-19).

[SHIP character and fireplace queries](SHIP_QUERIES.md) add the original
50307/50306 handlers over the live runtime attribute store and loaded locator
records. A checked message callback context avoids copying or re-entering the
script tree. Preview factories now bind characters during native Init;
indices update on every query and record counts follow the current vector.
The retained reference covers 56 sequences / 224 complete query snapshots.
Full SHIP mounting and native fireplace particle/sound objects remain pending.


The [SHIP query PR run](https://github.com/STG-44/storm-engine/actions/runs/39289799260)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39289786087)
pass all four native hosts for `84e76583`: 241 CPU tests, including 135
interpreter tests, plus 13 explicit Linux software Vulkan GPU tests and native
windows. Four archive digests and all 56 complete scenario reports are verified.
The thirteen preceding reports remain unchanged. A new resource-dependent
query audit covers all 40 original ship roots and 274 fireplace locators;
200 native deliveries and 80 ignored sends preserve live index/count behavior.
Original ArabellaShip1 also executes the query source and renders through
local offscreen Metal. Full mounting, particle/sound objects and native
cleanup remain pending; see [scope and evidence](SHIP_QUERIES.md#four-platform-evidence-2026-10-19).


[Native fireplace messages](SHIP_FIRE_MESSAGES.md) add 50304/50315 to the four
existing SHIP handlers. Run/Stop/Execute now preserve active rerun behavior,
raw float32 timing, owner/profile state, service validation and pause/cleanup
order. SAFE_DELETE clears the same live record vector used by the count query.
The selected controller survives startup for replay/live simulation; the
preview service retains paused particle tails across entity deletion. Sound
requests are recorded but audio playback and original particle simulation
remain pending. The new C++ reference has 60 sequences / 480 complete lifecycle
snapshots and 15 native-message sequences; its service/GetPos/storage adapters
and canonical libc++ destruction order are documented with the feature.


The [fireplace PR run](https://github.com/STG-44/storm-engine/actions/runs/39313370736)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39313359331)
pass all four native hosts for `615e5ebb`: 253 CPU tests, including 135
interpreter tests, plus 14 explicit Linux software Vulkan GPU tests and native
windows. Four archive digests and all 60 complete scenario reports match local
results; the fourteen preceding reports remain unchanged. The new audit reloads
40 original ship roots, counts 274 fireplace records and verifies first-locator
activation/expiry/cleanup/recreation on all 39 models with locators (352 native
deliveries overall). Local Metal passes the new emission pixel test and renders
original ArabellaShip1 after the new source. Particle appearance and sound
remain explicit preview/unbound services; see [scope and evidence](SHIP_FIRE_MESSAGES.md#four-platform-evidence-2026-10-19).


[XPS particle profiles](PARTICLE_PROFILES.md) now have a bounded PSYS v3.5
reader for all seven field types, billboard/model records and retained
scalar/color curve arithmetic. Real ship presentation loads Ship_Fire/Ship_Smoke
and the original default atlas; it samples profile size/transparency/RGB/UV/track
on deterministic preview slots. The native service retains longer profile tails.
Full schema conversion, emission/physics/RNG, arbitrary native profile selection,
project selection, original blend techniques and audio remain pending.


The [XPS PR run](https://github.com/STG-44/storm-engine/actions/runs/39324451435)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39324443081)
pass all four native hosts for `2768d83c`: 264 CPU tests, including 135
interpreter tests, plus 15 explicit Linux software Vulkan GPU tests and native
windows. Four archive digests and all 64 complete scenario reports match local
results; all fifteen preceding reports remain unchanged. Local decoding accepts
all 66 XPS profiles. Reloading 40 original ship roots finds 274 fire positions
and 117 XPS style patches; all 40 native fire reports equal the previous phase.
The new Metal atlas pixel test and original ArabellaShip1 XPS fire/smoke capture
pass. Full particle dynamics, arbitrary native profile selection and audio remain
pending; see [scope and evidence](PARTICLE_PROFILES.md#four-platform-evidence-2026-10-20).


[Retained particle emission](PARTICLE_EMISSION.md) replaces mean-rate slot
reconstruction with the original 32-substep clock/rate/remainder/capacity rules.
Living records retain age, lifetime and appearance factors; pause holds emitter
state while tails age. The native birth core is compared against 35 sequences /
245 complete C++ snapshots, with another 240 retained RandomRange samples.
The renderer supplies deterministic fixed frames and a portable per-patch RNG;
full particle movement, manager/global RNG ordering, native profile dispatch and
audio remain pending.


The [particle birth PR run](https://github.com/STG-44/storm-engine/actions/runs/39327662142)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39327654395)
pass all four native hosts for `3840b1f4`: 271 CPU tests, including 135
interpreter tests, plus all 15 explicit Linux software Vulkan GPU tests and
native windows. Four archive digests and all 64 complete scenario reports match
local results; all sixteen preceding reports remain unchanged. The new native
reference matches 35 sequences / 245 complete birth snapshots and 240 random
range samples. All 40 original ship roots reload; 274 positions / 117 style
patches yield 822 birth-counter rows at two seconds (38,086 created, 10,347
living). All 40 native-fire script reports match the preceding phase. Local
Metal verifies repeated-draw stability and renders ArabellaShip1 with 973/264
created/living particles. Full movement/forces and global service behavior
remain pending; see [scope and evidence](PARTICLE_EMISSION.md#four-platform-evidence-2026-10-21).

Billboard motion now uses retained native gravity, signed/zero mass, displacement
friction, spin friction and track/physics blending. XPS profiles drive live
position/velocity/angle state through the existing fixed-frame replay. The C++
reference matches 96 sequences / 672 complete motion/pool/graph-call snapshots.
World-space birth history, speed-oriented/attached/model particles and full
global service behavior remain pending; see [motion scope](PARTICLE_MOTION.md).


The [billboard motion PR run](https://github.com/STG-44/storm-engine/actions/runs/39501553195)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39501542794)
pass all four native hosts for `348e5516`: 276 CPU tests, including 135
interpreter tests, plus all 15 explicit Linux software Vulkan GPU tests and
native windows. Four archive digests / 64 complete JSON reports match local
outputs; all sixteen preceding reports remain unchanged. The new C++ motion
reference matches 96 sequences / 672 complete pool/motion/graph-call snapshots.
All 40 original ship roots reload: 274 positions / 117 style patches yield 822
finite motion rows; all 10,347 living particles move and 2,740 have spin. All
preceding birth counters and 40 native-fire reports match. Local Metal verifies
moving/spinning repeated-draw stability and an inspected ArabellaShip1 frame.
Historical world transforms, attached/model particles and full global service
behavior remain pending; see [scope and evidence](PARTICLE_MOTION.md#four-platform-evidence-2026-10-21).


Real XPS ship fire now retains native world-space birth transforms, including
the wrapper's fixed one-radian yaw, old/new matrix interpolation and saved
particle track matrices. The approximate sailing preview records every existing
physics-step pose before startup source execution, supporting headless/live
world trails independently of display cadence. The new reference covers 49
matrix pairs / 1,617 complete birth transform/point/normal snapshots. Generic
caller history sampling, explicit teleports and full native service/ship
physics remain limited; see [world scope](PARTICLE_WORLD.md).


The [world birth PR run](https://github.com/STG-44/storm-engine/actions/runs/39506026633)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39506020767)
pass all four native hosts for `2f8a8786`: 283 CPU tests, including 135
interpreter tests, plus 15 explicit Linux software Vulkan GPU tests and native
windows. Four archive digests / 64 complete JSON reports match local results;
all sixteen preceding reports remain unchanged. The retained reference matches
49 matrix pairs / 1,617 complete birth-matrix/point/normal snapshots. All 40
original ships reload: 274 locators / 117 patches yield 822 finite world rows;
all prior birth counters and native-fire reports match. Local Metal verifies
world-particle independence from the current ship pose and an inspected moving
ArabellaShip1 smoke trail. Full native ship/service behavior, arbitrary emitter
dispatch and generic teleport history remain pending; see [scope and evidence](PARTICLE_WORLD.md#four-platform-evidence-2026-10-21).


Named ship particle messages now select explicitly cached original XPS profiles,
including case/extension aliases and null results for missing profiles.
Additional profiles can be preloaded with --particle-profile and wait for
activation; generated diagnostics retain their prior service. Independent
particle generations are now covered by the [instance extension](PARTICLE_INSTANCES.md).
Complete project/service behavior remains pending.
See the [lookup contract and limits](PARTICLE_DISPATCH.md).


The [named particle PR run](https://github.com/STG-44/storm-engine/actions/runs/39651914838)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39651901113)
pass all four native hosts for `f7d1e737`: 289 CPU tests / 135 interpreter
tests, 15 explicit Linux software Vulkan GPU tests and native windows. Four
archive digests / 64 full JSON reports match local results. Fourteen retained
C++ profile/cache queries match; all 40 original ships reload and 39 nonempty
models execute named-profile scenarios. Manual birth/world counters remain
unchanged, including an identical original-resource Metal capture with an extra
cached profile. See [current lookup scope and evidence](PARTICLE_DISPATCH.md).
Independent particle generations and full project/global service remain next.


Ship particle creation now retains independent instance IDs and overlapping
smoke tails across stop/restart, including the same XPS in both channels.
Replay and dynamically sized draw geometry preserve old generations until
expiry; complete global manager/RNG/project behavior remains pending.
See [instance identity, limits and verification](PARTICLE_INSTANCES.md).


The [particle instance PR run](https://github.com/STG-44/storm-engine/actions/runs/39663253978)
and [push run](https://github.com/STG-44/storm-engine/actions/runs/39663244166)
pass all four native hosts for `7c7fbf60`: 297 CPU tests / 135 interpreter
tests, 15 explicit Linux software Vulkan GPU tests and native windows. Four
archive digests / 64 full JSON reports match local results. Three unedited
native factory/pause bodies match eight complete identity/pause snapshots.
All 40 original ships reload; 39 nonempty roots retain 1,096 distinct instances
and 2,192 world rows across both generations. Manual counters and a Metal
capture remain unchanged. Global project/atlas/processor/RNG/deletion and
multi-ship service behavior remain pending; see [instance scope](PARTICLE_INSTANCES.md).


Particle project descriptors now select native profile membership and global
atlases at startup. The default-project option discovers root-level XPS files;
named projects retain native INI ordering/gap/duplicate rules and texture suffix/
stem requests. Presentation still uses the explicitly loaded billboard subset.
See [project scope and verification](PARTICLE_PROJECTS.md).


The [particle project validation](https://github.com/STG-44/storm-engine/actions/runs/40198817921)
passes all four native hosts for `9781dbe5`: 305 CPU tests / 135 interpreter
tests, sixteen explicit Linux software Vulkan GPU tests and native windows.
Four archive digests / 68 complete JSON reports match local results; all sixteen
prior scenarios remain unchanged. Seventeen unedited-native PRJ/INI/atlas
snapshots pass. All forty original ship roots reload with 120 full prior-report
matches and 39 named-project matches. Both original-resource Metal captures
match the preceding sailing frame. The owner restored fork Actions; future
rust-port PR updates use one matrix, with develop pushes/manual dispatch retained.
Full global cache/GeomCache registration, arbitrary lazy creation, live project
switching, native normal/blend techniques and manager RNG remain pending; see
[project scope](PARTICLE_PROJECTS.md).


The [shared particle service](PARTICLE_MANAGER.md) caches selected project XPS
descriptions and prepares billboard definitions by native message request.
Independent owners retain separate systems; explicit release/clear and native
IsAlive-based completion replace the new mode's former timeout approximation.
GPU scenes add requested profile meshes dynamically and draw multiple ships
through shared depth. A deterministic two-ship diagnostic exercises pause,
restart and first-owner removal while the second keeps emitting. This advances
project/service presentation; complete native processors, global RNG, SHIP
physics, naval combat, land/deck locomotion and boarding remain pending.


The [shared-service PR validation](https://github.com/STG-44/storm-engine/actions/runs/40202477211)
passes all four native hosts for `a58e0227`: 312 CPU tests / 135 interpreter
tests, seventeen explicit Linux software Vulkan GPU tests and native windows.
All seventeen explicit render tests pass locally on Metal. Four ZIP digests /
72 complete JSON reports match local results; all seventeen preceding scenarios
remain unchanged. Five unedited native bodies match twelve registry snapshots
and nine IsAlive cases. All forty original roots reload, retaining 120 complete
prior manual/world reports; 39 models create an additional project profile on
demand. Original two-ship captures retain 42 systems after restart and fourteen
second-owner systems / 315 living particles after first-owner release. See
[shared-service scope](PARTICLE_MANAGER.md) for the diagnostic motion, native
adapters, resource limits and remaining game systems.


[Retained SHIP movement](SHIP_MOTION.md) now ports strength application,
acceleration/braking, millisecond movement, collision prediction and native
impulse message 50308. An opt-in fleet mode uses independent native bodies
instead of prescribed poses; its speed/rocking providers remain explicit
fixtures. Full Mount/Execute, original sea/sail/weather providers and gameplay
are pending. The [weather roadmap](WEATHER_ROADMAP.md) now separately tracks
weather state, day/night, sky/fog, rain/storms/lightning/audio, wind integration
and location/save transitions.


SHIP movement uses pinned scalar software sine/cosine to avoid platform-library
rounding differences. The native oracle records C-library trig observations;
movement arithmetic replays them bit-exactly and the production trig kernels
are checked within one ULP, including exact signs/signed zeros. Existing sailing
and rendering math remain unchanged; see [movement numeric scope](SHIP_MOTION.md).


The [SHIP movement PR run](https://github.com/STG-44/storm-engine/actions/runs/40220630111)
passes all four hosts for `d47ade14`: 318 CPU tests / 135 interpreter tests,
seventeen explicit Linux software Vulkan GPU tests and native windows. Four ZIP
digests / 76 full JSON reports match local output; all eighteen preceding
scenarios are unchanged. Sixty-four native sequences / 704 complete arithmetic
snapshots use recorded C-library trig values; pinned software trig is separately
checked within one ULP. This resolves the initial ARM/Windows rounding mismatch.
All forty original models run with independent native bodies, including the
empty-fireplace Boat. Three-second owner routing retains 548 systems / 822 world
rows / 12,243 living particles, with original model metadata preserved. Four local
Metal captures and all seventeen render tests pass. See [movement scope](SHIP_MOTION.md)
and the newly explicit [weather roadmap](WEATHER_ROADMAP.md).

[Retained ship rocking](SHIP_ROCKING.md) adds the original 36-point hull grid,
vertical smoothing and pitch/roll calculation. An additional fleet option uses
root GM dimensions and the rendered diagnostic wave surface, with reflected
coordinates and independent grids/particle pose histories. Original SEA,
immersion/waterline, wake, sinking and full Mount/Execute remain pending.


The [ship rocking PR run](https://github.com/STG-44/storm-engine/actions/runs/40222394085)
passes all four hosts for `98509601`: 325 CPU tests, seventeen explicit Linux
software Vulkan GPU tests and native-window fixtures. All 80 complete reports
match local results with verified ZIP digests; nineteen preceding scenarios
are unchanged. Ninety-six native sequences / 480 complete rocking snapshots
match with recorded native scalar math; pinned software kernels are independently
checked within one ULP with exact signs/zeros. All forty original models use
their root GM dimensions and independent rocking grids, with first-owner freeze,
second-owner survival and original model metadata preserved. Four local Metal
captures and all seventeen render tests pass. Original SEA and full Mount/Execute
remain pending; see [rocking scope](SHIP_ROCKING.md).

[Immersion and model placement](SHIP_PLACEMENT.md) add the native live character
lookup, separately loaded ship-type waterline and original root expression.
The new interactive fleet window reuses fixed native motion/rocking, keeps both
ships visible and supports orbit/zoom, pause, immersion changes and explicit
fire toggling. Commercial ship parameters, original SEA and full mounted gameplay
remain pending.


The [placement/viewer PR run](https://github.com/STG-44/storm-engine/actions/runs/40224545294)
passes all four hosts for `d900f9c9`: 330 CPU tests and twenty-one standalone
scenarios. Linux also passes seventeen explicit software Vulkan render tests
and three frames of the new native fleet window; local Metal checks pass the
same render suite and bounded viewer. All 84 reports match local results with
verified ZIP digests, preserving the preceding twenty reports. Sixty-four
native sequences / 192 complete immersion/root-argument snapshots match.
All forty original models use correctly placed roots without changing native
body state, and four generated/original Metal captures pass. The interactive
viewer keeps both ships and supports orbit, pause, immersion changes and fire.
Full mounted gameplay and original SEA remain pending; see
[placement/viewer scope](SHIP_PLACEMENT.md).

[Living ship frames](SHIP_FRAME.md) retain Execute's pre-Move model pose,
bounded additive turn heel and post-Move script outputs. The viewer uses this
boundary and supports held A/D steering. Body state, model root and script
fields remain separate; complete mounted gameplay and original sea/force
providers remain pending.


The [living-frame PR run](https://github.com/STG-44/storm-engine/actions/runs/40227614735)
passes all four hosts for `5ec7e101`: 335 CPU tests and twenty-two standalone
scenarios. Linux software Vulkan and local Metal each pass seventeen render tests
and three frames of the updated fleet window. All 88 reports match local output
with verified archive digests; 21 preceding reports stay unchanged. Forty-eight
distinct input profiles / 288 complete snapshots match native living-frame,
movement and script-write slices. All forty original model roots use delayed,
heeled presentation with independent script outputs, unchanged physical state
and preserved metadata. A/D steering is available in the viewer; full mounted
Execute and original SEA remain pending. See [frame scope](SHIP_FRAME.md).


[Ship-type parameters](SHIP_PARAMETERS.md) now load retained typed attributes
and numeric Mount state, configuring inertia/mass and waterline without resetting
motion. Current character limits stay separate from nominal type rates. The
viewer uses the reader and accepts bounded external attribute files; commercial
PROGRAM definitions and full mounting/forces remain pending.


The [ship-parameter PR run](https://github.com/STG-44/storm-engine/actions/runs/40381752291)
passes all four hosts for `53a83b2c`: 342 CPU tests, twenty-three standalone
scenarios and 92 complete JSON reports matching local output with verified ZIP
digests. All 22 preceding reports remain unchanged. The parameter/mounting oracle
matches 64 distinct profiles / 192 complete represented snapshots. Linux passes
seventeen software Vulkan tests and three parameter-enabled viewer frames;
local Metal passes seventeen render tests and four generated/original captures.
The Mac window initializes, but its current three-frame presentation check is
unverified. All forty original roots accept external records and preserve
metadata while actual motion changes. Commercial ship definitions, full mounted
services and original SEA remain pending. See [parameter scope](SHIP_PARAMETERS.md).


[Native sail drive](SAIL_DRIVE.md) now supplies actual fleet strength from retained
wind/damage formulas and the speed message case. Local hard-point capacities,
node-frame input and the three set thresholds are preserved. W/S selects the
first viewer ship's sail set; groups and release remain independent. Static node
frames and instant settled geometry are explicit adapters; original yard turning,
rolling animation, weather and full scheduling remain pending.


The [sail-drive PR run](https://github.com/STG-44/storm-engine/actions/runs/40387887268)
passes all four hosts for `9d573fdf`: 348 CPU tests, 24 scenarios and 96 complete
JSON reports matching local output with verified ZIP digests; all 23 preceding
reports stay unchanged. Ninety-six native profiles / 288 sail outputs / 384 group
snapshots match. All forty original roots and 368 sails use native drive while
preserving metadata and owner lifetimes. Linux passes seventeen Vulkan render
tests and three updated viewer frames; Metal passes seventeen render tests and
six captures, including three inspected original sail sets. Local native-window
checks were not repeated. Original rolling/yard turning and weather remain
pending. See [scope and adapters](SAIL_DRIVE.md).
