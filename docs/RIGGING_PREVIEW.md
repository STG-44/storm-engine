# Ropes, shrouds and flags

The Rust preview now builds rigging from original GM locators. Hull and rigging
share the same rigid transform, including yaw, pitch, roll and heave. This is a
new kinematic preview, not the original tension, damage or script simulation.

`ROPE::AddLabel` supplies paired `ropebN/ropeeN`, `falbN/faleN`, and stay endpoints.
Pairs can span different hierarchy nodes. Six-sided segmented tubes use the
configured rope/stay width and texture. Rope spans have bounded sag and movement;
halyards/stays remain taut. Sail-group endpoints follow the nearest original
hard point as that sail gathers, retaining their rest offset. This preserves
attachments through all three sail sets.

`VANT_BASE::AddLabel` supplies `vant`, `vanx` and `vanz` groups, each with upper,
left and right points. The preview generates fan-shaped shroud ropes and ladder
rungs. It uses configured widths/rope counts; rung spacing and 3D tube geometry
are new, rather than a port of the original VANTS texture packing.

`FLAG::SetAdd` defines a quad's attached edge as f1/f4 and its free edge as f2/f3.
Three-point preview pennants use a tapered strip without collapsed tip triangles.
Flag poles stay fixed while the free surface follows local apparent wind. Calm
flags remain still; Space pauses flags, cloth, ropes and movement together.
Flags remain visible when sails are gathered. Normals are rebuilt after deformation.

## Materials and limits

When available, `RESOURCE/INI/rigging.ini` supplies rope/flag texture names, widths,
segment length, shroud rope count and flag atlas dimensions. The tested game uses
`sail_rope.tga` and `flags/flagall_ship_b.tga`, with a 4-by-8 flag atlas. Texture
pixels stay in the external game installation. The first atlas cell is selected,
matching the C++ fallback when `GetRiggingData` supplies no texture coordinates.
Character nation, pirate/special flag selection and flag-changing scripts remain
unimplemented. Seeing a flag does not establish that gameplay nation selection
has been ported.

Incomplete endpoint groups are counted and omitted. Repeated points use the last
locator, as the original initializer does. Invalid original rope lengths are
reported; the model is still previewable. Locator/mesh limits and the existing
512 MiB scene budget apply. Tube geometry and CPU/GPU buffers are reused each
frame. There is no rope tension solver, rope collision, falling-mast transfer,
original flag motion, procedural weather or original rigging/gameplay parity.

## Run and inspect

```sh
cargo run -p storm-demo --locked -- --rigging-demo --wind-speed 12
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --game-dir /path/to/game --sail-setting 0.5
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --untextured --rig-report rig.json
```

`--rigging-demo` uses only generated ropes, shrouds, flags, pennants and sails.
The JSON report includes complete, incomplete, duplicate and invalid rigging
group counts, alongside original per-sail mode membership.

## Validation

Local macOS Intel / Radeon Pro 555X passes 35 CPU tests and nine Metal GPU tests,
formatting and Clippy. New checks cover original quad corner order, texture/INI
parameters, cross-node rope pairing, a moving sail tie, finite tube normals,
three-point shrouds, incomplete groups, both flag topologies, atlas UV bounds,
pinned poles, calm/windy behavior and nondegenerate flag tips. A persistent GPU
scene checks flag movement and visibility while sails are gathered, excluding
animated water from the comparison.

All 40 available ship roots fully decode with rigging: 776 reachable GM files,
368 sails, 2,896 complete rope pairs, 264 shroud sets and 114 flags. Original
sail membership still matches an independent C++-rule audit for all 40 models.
The collection also contains 20 unmatched rope pairs, 22 repeated points and
two invalid original rope lengths; these are recorded in the local report.
There are no incomplete shroud or flag groups. This is decoding/geometry
validation; not all 40 models have been visually or gameplay tested.

Barkentine1 has 93 complete rope pairs, six shroud sets and three flags, without
incomplete or duplicate groups. It renders 20 GM files, 38 draw meshes, six
texture slots, 12 sails and 19,431 triangles locally through Metal. Original ship
captures and metadata reports remain outside Git. Linux CI exercises the
generated rigging fixture and its native window through software Vulkan.
