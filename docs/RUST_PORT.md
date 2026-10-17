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
- [ ] Create the Rust workspace, native window, GPU diagnostics, and CI builds for all three operating systems.
- [ ] Read Storm GM geometry and TX textures with bounds checks and format tests.
- [ ] Display an original ship, sea, and movable camera. Verify the same scene on all three operating systems.
- [ ] Port animation, input, audio, and platform file handling.
- [ ] Port the script runtime, entity messages, and game-facing API.
- [ ] Integrate locations, interface, sea combat, land combat, and persistence.
- [ ] Verify representative gameplay and publish platform builds.

## Current evidence and prerequisites

This commit establishes the project plan; it does not implement a Rust renderer or claim playable game support. Native platform support must be verified on each OS and supported GPU family. The user's game resource directory is still needed for integration testing with original ship models and textures.
