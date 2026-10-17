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
- [ ] Display an original ship, sea, and movable camera. Verify the same scene on all three operating systems.
- [ ] Port animation, input, audio, and platform file handling.
- [ ] Port the script runtime, entity messages, and game-facing API.
- [ ] Integrate locations, interface, sea combat, land combat, and persistence.
- [ ] Verify representative gameplay and publish platform builds.

## Current evidence and prerequisites

The renderer foundation is implemented in `rust/`; see [run and validation commands](../rust/README.md).
The diagnostic sea is new test code, separate from the original Storm sea simulation.
Static ship preview, approximate sailing and damped wave response are implemented. Original
game loading, calibrated ship simulation, and gameplay remain pending.

Verified locally on 2026-10-17:

- macOS 15.7.8, Intel x86_64, Rust 1.96.0.
- Metal rendering on AMD Radeon Pro 555X; adapter discovery also finds Intel UHD Graphics 630.
- Formatting and Clippy pass with warnings denied.
- CPU format/shader/dynamics validation and all seven explicit GPU tests pass.
- The native diagnostic and original ship windows each present three frames and exit successfully.
- A 1280 x 720 frame is saved through the offscreen pipeline.
- Barkentine1 assembles from 20 GM files, with three original TX textures and
  twelve sails (two triangular): 34 draw meshes, 8,343 triangles. Wind deforms
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
animated geometry, older GM versions, original material/sea effects, original
rigging, ropes, full ship physics, and gameplay remain pending. Full decoding has been exercised on
the ship preview and upstream fixture; the previous whole-resource scan remains
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
