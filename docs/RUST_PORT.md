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
