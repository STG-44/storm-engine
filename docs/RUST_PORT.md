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
- [ ] Display an original ship, sea, and movable camera. Verify the same scene on all three operating systems.
- [ ] Port animation, input, audio, and platform file handling.
- [ ] Port the script runtime, entity messages, and game-facing API.
- [ ] Integrate locations, interface, sea combat, land combat, and persistence.
- [ ] Verify representative gameplay and publish platform builds.

## Current evidence and prerequisites

The renderer foundation is implemented in `rust/`; see [run and validation commands](../rust/README.md).
The diagnostic sea is new test code, separate from the original Storm sea simulation.
Static ship preview is implemented; game loading, ship simulation, and gameplay
are not implemented yet.

Verified locally on 2026-10-17:

- macOS 15.7.8, Intel x86_64, Rust 1.96.0.
- Metal rendering on AMD Radeon Pro 555X; adapter discovery also finds Intel UHD Graphics 630.
- Formatting and Clippy pass with warnings denied.
- CPU format/shader validation and both explicit GPU tests pass.
- The native diagnostic and original ship windows each present three frames and exit successfully.
- A 1280 x 720 frame is saved through the offscreen pipeline.
- Barkentine1 assembles from 20 GM files, with three original TX textures and
  ten curved static sails generated from four-corner locators: 32 draw meshes,
  8,055 triangles. A 1280 x 720 capture is saved outside Git.
- GPU fixture checks cover texture sampling, geometry visibility with the depth
  pipeline, zero-normal fallback lighting, camera changes, and invalid cameras.

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
animated geometry, older GM versions, original material/sea effects, triangular
sails, ropes, and gameplay remain pending. Full decoding has been exercised on
the ship preview and upstream fixture; the previous whole-resource scan remains
header-only evidence.

The [first original-resource inspection results](ASSET_VALIDATION.md) record
4,129 accepted GM headers, 5,732 accepted TX headers, and seven GM compatibility
cases. The inspector's [four-platform CI run](https://github.com/STG-44/storm-engine/actions/runs/39004859635)
passed for code commit `ca6a70b6`.
