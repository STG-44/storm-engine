# Storm Rust renderer foundation

This native application is the first executable part of the Storm Engine Rust
port. It opens an animated diagnostic sea using wgpu, or renders the same scene
to a PNG without a window. The diagnostic scene is newly written and does not
yet implement Storm gameplay, ship loading, or the original sea simulation.

![First diagnostic frame rendered with Metal](screenshots/first-frame.png)

## Run

Install Rust through [rustup](https://rustup.rs/). Run these commands from the
repository root. `rust-toolchain.toml` pins the compiler and required components;
`Cargo.lock` pins dependency versions.

```sh
cargo run -p storm-demo --locked
cargo run -p storm-demo --locked -- --gpu-info
cargo run -p storm-demo --locked -- --headless first-frame.png
```

The window closes with Escape or its close button. Its title identifies the
selected adapter and graphics API. Resize, minimization, surface recovery, and
resume are handled by the native event loop. A smoke run exits after three
presented frames:

```sh
cargo run -p storm-demo --locked -- --frames 3
```

`--backend auto` allows native Metal, Vulkan, and DirectX 12 implementations.
Use `--backend metal`, `--backend vulkan`, or `--backend dx12` to require a
specific API. An unavailable API is reported as an error. Windows
and Linux builds need a compatible GPU driver. macOS uses Metal directly.

The first local check passed on macOS 15.7.8 / Intel with a Radeon Pro 555X:
Metal offscreen readback and animation tests, and three frames presented in a
native window. This is renderer validation; original game support is pending.

Linux needs the X11/Wayland development packages when compiling. On Ubuntu:

```sh
sudo apt-get install libxkbcommon-dev libwayland-dev libx11-dev libxi-dev libxrandr-dev libxcursor-dev
```

For software rendering on Linux, install `libvulkan1 mesa-vulkan-drivers`.

## Validate

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test -p storm-render --locked -- --ignored
```

The default test validates the WGSL shader without a GPU. The explicitly ignored
GPU test renders two animation times, checks opaque nonuniform pixels, verifies
row padding is removed, and rejects invalid frame sizes and times. It requires
an actual native graphics adapter or a software Vulkan implementation.

The Rust workflow builds and runs CPU checks on Linux, Windows, macOS Apple
Silicon, and macOS Intel. Linux also runs the GPU test with software Vulkan and
the native window under Xvfb. Hosted macOS/Windows builds alone do not establish
GPU or gameplay compatibility; physical-device checks remain necessary.

## Original game resources

Keep the complete game installation outside Git, or use the ignored `game-data/`
directory. The current diagnostic needs no assets. GM/TX readers and original
ship rendering are subsequent milestones in [the port plan](../docs/RUST_PORT.md).
