# Native scalar sea surface

The first marine-integration increment retains the numeric surface pipeline in
`src/libs/sea/src/sea.cpp`: raw bump bytes from Init, normal construction from
BuildVolumeTexture, CalculateHeightMap, CalculateNormalMap, Realize's frame and
movement arithmetic, and scalar WaveXZ. The Rust module is `storm-sim::sea`.
The full SEA entity and original water rendering are not complete.

## Retained behavior

- 64 frames of 128 by 128 blue bytes, in original file order. Init computes
  five temporal weights but assigns the unchanged current frame; no smoothing
  filter is introduced. The preview loader accepts raw 32-bit true-color TGA
  without an ID or color map, reads BGRA blue, and does not flip origin flags.
- Four wrapped neighbor differences, the original cross-product/sum order,
  CVECTOR normalization (float squared sum, double reciprocal square root),
  signed 16-bit X/Z packing, and decoding by 32767.5.
- Linear frame interpolation, including frame 63 to frame 0. Height amplitude
  is 1/255; CalculateNormalMap's amplitude argument is unused. Two animated
  layers retain separate frame indices, movement, scale and wave amplitude.
- WaveXZ's strict camera-distance cutoff, wrapped bilinear sampling, layer
  sum, reconstructed normal Y and combined normalized normal. Native ffloor
  is SSE nearest-even rounding of `x - .5`, including its integer tie cases;
  replacing it with floor would change intermediate behavior.
- Immutable prepared maps shared by CPU ship queries and the GPU sampler.
  The GPU boundary reflects renderer Z into native Z and reflects normal Z
  back. Normal sampling is distinct from the height-only ship query.

Checked Rust entry points reject malformed frames, non-finite parameters and
queries, unsafe native integer-coordinate ranges and updates over 1000 ms.
Failed animation/camera updates do not replace the published surface. A snapshot
retains its prepared maps when later animation frames are published. Negative
finite amplitudes/scales and zero amplitudes are retained in the numeric tests.

## Preview integration

Add `--native-sea` to a `--particle-fleet-demo --native-ship-motion
--native-ship-rocking` invocation. Without additional arguments the input byte
frames are explicitly synthetic and contain no game resources. Add
`--sea-frames-dir /external/game/RESOURCE/Sea` to load sea0000.tga through
sea0063.tga from a separate game installation. No original resource enters Git.

Both living ships query the same prepared surface through retained 36-point
ShipRocking; water shading samples those same maps, frame indices and layer
parameters. Animation advances once per fixed fleet step, independently of ship
count. Pausing/repeated presentation does not advance it. Removing one ship
retains its last rocking grid while the survivor and sea continue. Viewer
restart constructs a fresh sea from the same inputs.

This integration deliberately supplies original EditMode_Update layer defaults
and an explicit fixed native sampling camera at X/Z zero. The report names
these providers. The orbit camera does not yet drive SEA's camera cutoff;
weather attributes and native Execute/Realize ordering remain pending. The
existing two-sine diagnostic mode remains the default for preceding fixtures.

The preview water still uses its own ray intersection, lighting and reflection
approximation. Original SSE_WaveXZ, block tree/LOD, tessellation, reflection and
sunroad rendering, foam, underwater behavior, world offsets, weather/message
contracts and full SEA resource/entity lifetime are separate integration work.
This increment does not establish full original sea or naval gameplay parity.

## Validation

`rust/crates/storm-sim/reference/sea/generate.py` compiles unedited C++ method
bodies and the retained normal/animation blocks with UBSan, using original
CVECTOR and SSE helpers. The checked-in reference contains only synthetic data:
64 normal-map hashes, 64 complete prepared-map hashes/animation states and
6144 height-with-normal / height-only query pairs. Rust checks exact float bits
and full-map byte hashes, including negative/wrapped coordinates, camera cutoff
boundaries, zero/signed amplitudes/scales and frame wrap. Regeneration must be
byte-identical.

The GPU agreement test compares height and reflected normals at 192 queries
with an absolute tolerance of 0.0001 per component, including the distance
boundary. Additional tests cover TGA byte semantics, malformed input, immutable
snapshots, invalid-update atomicity, fixed-step partitioning, repeated paused
presentation and independent owner removal. CI runs CPU tests on all four
desktop hosts and the explicit GPU sampler/render/window checks on Linux Vulkan;
local checks use Metal. Hosted Windows/macOS physical GPUs remain uncovered.
