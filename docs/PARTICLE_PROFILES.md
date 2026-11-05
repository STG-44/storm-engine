# XPS particle data and ship appearance

The Rust reader now decodes original `PSYSv3.5` XPS files. The format follows
`src/libs/particles/src/system/data_source`: point emitters, billboard/model
particle records and all seven field types (bool, float, graph, position,
fixed string, UV frames and color). It preserves field order and source byte
strings, converts saved UV width/height to native end coordinates, normalizes
boolean flags and clamps negative graph values when the field disallows them.

```sh
cargo run -p storm-formats --locked -- decode /path/to/RESOURCE/Particles/Ship_Fire.xps
cargo run -p storm-formats --locked -- decode \
  rust/crates/storm-formats/reference/particles/synthetic.xps
```

Reports contain the complete loaded fields with float32 bit patterns. String
values/names retain their bytes; a convenience display accepts UTF-8 or falls
back to Windows-1251. This is the raw field stage: original schema `Convert`
(default insertion, wrong-type replacement and unknown-field removal) remains
pending. Model particle data is decoded but model particle rendering is not
implemented. Inspect/scan continue to validate GM/TX headers; XPS uses decode.

## Checked reads and curve behavior

Reads use explicit little-endian words, without native pointer/alignment casts.
Limits are 16 MiB input, 256 emitters/types/fields per list, 16,384 points per
curve and 262,144 total counted elements. Names use a declared length of 1..127
bytes with a contained NUL; fixed strings require a NUL within their 128 bytes.
The first NUL ends the native string. Unterminated strings, unknown types,
truncation, non-finite input, overflowing UV extents, trailing bytes and other
versions fail explicitly. C++ ignores its version warning and tolerates some
invalid reads; those are not compatibility guarantees adopted here.

Scalar curves retain first-segment selection, extrapolation before the first
point, the 0.001-second segment threshold and zero beyond the final segment.
Relative time is `time / lifetime * 100`. Min/max curves interpolate before
applying the supplied random fraction. Color time is normalized to 0..1 and
requires both segment bounds. Its native Color::Lerp changes RGB only: alpha
remains one. Out-of-range color returns white. Transparency is a separate
inverse percentage. Sampling is stateless; native monotone-time caches and
C rand state are not ported. Sorted graphs give the same segment selection;
cache behavior on unsorted graphs remains outside the comparison. Empty
scalar graphs return zero safely instead of native unsigned-loop underflow.

## Ship presentation

When RESOURCE/Particles exists, the model loader reads `Ship_Fire.xps` and
`Ship_Smoke.xps` and the original default `particles_list.tga` atlas. It creates
one patch for each billboard description, including the smoke description
inside Ship_Fire itself. The profile supplies slot capacity, emission rate,
particle lifetime, size, RGB, transparency, animation-frame UVs, emitter offset
and Track X/Y/Z. Z is reflected into renderer coordinates. Style metadata is
included in real-resource ship reports; generated fixtures retain the previous
presentation and report shape. Missing or unsupported selected data fails
explicitly. Resources without a Particles directory retain the prior preview.

```sh
cargo run -p storm-demo --locked -- \
  --model /path/to/RESOURCE/MODELS/Ships/ArabellaShip1/ArabellaShip1.gm \
  --game-dir /path/to/game-data --fire --time 2
```

The preview permits 1..2048 slots per particle description and positive maximum
lifetimes up to 60 seconds. Native fire cleanup now retains service tails for
the loaded profile's maximum lifetime (at least the previous seven seconds),
so original long-lived smoke does not disappear early from the service.

The subsequent [particle emission increment](PARTICLE_EMISSION.md) replaces
mean-rate slot reconstruction with retained 32-substep birth accumulation,
living particle records and a fixed-frame replay adapter. XPS appearance still
uses current UV frames and the default atlas; full movement/forces/drag, spin,
attached/model rendering, camera LOD, frame blending and original techniques
remain pending. The subsequent [named profile increment](PARTICLE_DISPATCH.md)
adds explicit cache selection and native message profile dispatch; .prj project
selection remains pending.
Audio playback is unbound.

## Native reference and tests

`rust/crates/storm-formats/reference/particles/generate.py` compiles seven
unedited field Load bodies, scalar min/max/value evaluation, color evaluation
and the original RGB-only Color::Lerp under UBSan. Ten loaded fields and 282
complete scalar/color sample tuples match, including flag combinations,
clamping, relative/absolute times, short segments, endpoints, extrapolation,
backtracking and color alpha behavior. Supplied memory reads/type storage,
setters, type dispatch and fixture serialization are adapters. Floating-point
contraction is disabled to compare scalar float32 operations across hosts.
Schema conversion, C rand, full DataSource initialization and particle physics
are outside this oracle. Retained body hashes accompany the synthetic fixture;
no commercial particle files are committed.

Eleven added CPU tests cover all field/reference cases, every truncated prefix
of the synthetic container, invalid sizes/types/data, safe sparse curves,
legacy text, atlas UVs, native stop/tail retention and appearance bounds. One
additional explicit GPU test samples a colored atlas cell through XPS style
curves and verifies that clearing native emissions restores the unlit image.
The synthetic CLI decoder is the sixteenth resource-free CI scenario.


## Four-platform evidence, 2026-10-20

Code `2768d83c629dc835720b5804b9bd3d56a9502e52` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39324451435) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39324443081) on
macOS Apple Silicon/Intel, Windows and Linux: 264 CPU tests, including 135
interpreter tests, formatting, Clippy with warnings denied, native builds and
all sixteen standalone scenarios. The Rust host triples are
`aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc` and
`x86_64-unknown-linux-gnu`. Linux also passes 15 explicit software Vulkan GPU
tests and native windows. Four downloaded archive SHA-256 digests and all
64 complete JSON scenario reports match local results. All fifteen preceding
reports remain unchanged; the sixteenth decodes the synthetic XPS container.

The resource audit decodes all 66 local XPS files and reloads all 40 original
ship roots. It counts 274 fireplace positions and 117 XPS style patches across
the 39 models with locators. All 40 complete native-fire scenario reports equal
the previous fire phase, including the rig state. Locator arrays, sail
count/topology/source weights and rigging metadata match the earlier layout
reports. Those older layouts contain sail damage, so their reduced aggregate
weights are not compared against these intact layout runs. Original damage
layout/event audits were not regenerated for this appearance increment.

The new explicit local Metal GPU test verifies the selected colored atlas
cell and exact restoration of the unlit frame after native emission clearing.
Original ArabellaShip1 renders all three XPS billboard descriptions through
local Metal at two seconds; its source atlas/curve-driven fire and smoke image
was visually inspected. Commercial resources, captures and local audit reports
remain outside Git. Previous local native-window attempts timed out; that
surface/window check was not repeated. Hosted Windows/macOS jobs cover CPU
behavior/builds; physical GPU/window verification there remains pending.
Full schema conversion, native birth/physics/RNG, dynamic message profile
selection, project selection, remaining original blend techniques and audio
remain unfinished.
