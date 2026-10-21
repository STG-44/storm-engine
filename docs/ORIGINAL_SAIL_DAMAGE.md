# Original sail damage

The procedural sector-sized circles from the first damage preview are removed.
The original game's `RESOURCE/Textures/Ships/parus_hole.tga.tx` now supplies the
small, irregular transparent punctures. It remains external to Git. The mask
is loaded through the existing bounded TX decoder with its original mip chain;
a missing mask is an error for an original textured ship. CI uses separately
generated alpha markers, never commercial artwork.

## Source correspondence

| Original source | Rust behavior |
| --- | --- |
| `sailone.cpp::SetSail` | 13 x 17 quad vertices, 17 triangular rows (153 vertices), twelve/ten damage bits |
| `sailone.cpp::FillIndex` | Same full-resolution triangle indices and intact/damaged ranges, including the original reversed placement/order in the hole range |
| `sailone.cpp::SetTexGrid` | Mask UV = (1 + 0.5 * base.u, 0.5 * base.v + (triangular ? 0.5 : 0)) |
| `sail.cpp::Realize` | Damaged triangles first, mirror U / wrap V; intact triangles use wrap U / V |
| `src/techniques/ship/Sail.fx` | Multiply base alpha by mask alpha; mask RGB does not tint cloth; source-alpha blending and depth writes |
| `renderer/src/s_device.cpp` | Original alpha test: greater than 0xA0 / 255 |
| `sailone.cpp::Trace`, `SSailTrace`, `TSailTrace`, `CheckSailSquar` | Original coarse point sets, box admission, two-sided projected edge checks, all intersected sector bits and nearest intersection |
| `sailone.cpp::SetAllHole` | Restore the low ten/twelve bits; repeated damage does not increase the sector count |
| `sail.cpp::GetSailSpeed` | Original float arithmetic and nonnegative speed clamp, using `fHoleDepend` from INI |

The mask's clean and damaged regions are selected by the addressing mode.
It is not a texture tiled once per damage sector. Different punctures belong
to different places across the entire sail. The triangular mask occupies the
lower half. Damage changes which triangles expose the original mask; it does
not generate a new circle at a cell centre or a new procedural hole per impact.
This also means one damaged sector can reveal several small punctures, just as
in the source. Test volleys sample separated sectors throughout the cloth.

Cannon traces do not sample texture alpha in the original: a ray can still hit
a damaged sector. That behavior is retained, including source endpoint-on-plane
handling and the asymmetric Y check in its bounding-box admission. The collision
grid samples the current preview cloth, with the original cell connectivity.

`fHoleDepend`, `fTHoleFlexDepend` and `fSHoleFlexDepend` are read from `[SAILS]`
in `rigging.ini`, with the source defaults and upper flex clamps. The latter
coefficients scale billow (and triangular flutter) in the existing preview.
The full original wind generator, rolling/free-sail integrator and distance LOD
selection are separate from this damage port; the renderer keeps full detail.
`SetAllHole`-style restoration is available through `ModelAsset::set_sail_holes`.
The demo now dispatches `DoSailHole`/`SHIP_SAIL_DAMAGE` payloads through a native
observer; see the [runtime foundation](RUNTIME_FOUNDATION.md). Script execution
and gameplay side effects during collision traversal remain pending.

## Reproduce

```sh
cargo run -p storm-demo --locked -- --sail-demo --sail-volley 4
cargo run -p storm-demo --locked -- \
  --model /path/to/game/RESOURCE/MODELS/Ships/Barkentine1/barkentine1.gm \
  --game-dir /path/to/game --sail-volley 4
```

X sends a test segment through the camera centre; 0 restores the ship. Space
pauses the shared clock. `--sail-volley N` is a deterministic test helper using
the same collision path. It is not a cannon combat or ballistic implementation.
`--rig-report` exports the original mask path, grid sizes, INI coefficients and
per-sail masks without a GPU. Original resources/captures stay outside Git/CI.

## Compare against the retained C++

`rust/crates/storm-render/src/model/sail_damage/reference/generate.py` extracts
and compiles the retained C++ `FillIndex`, `SetTexGrid`, `Trace`, `SSailTrace`,
`TSailTrace`, `CheckSailSquar` and `GetSailSpeed` bodies. It uses the original
`c_vector.h`; only renderer/entities/events are stubbed. It does not call the
Rust implementation or read game data. Source hashes are recorded in the result.

```sh
python3 rust/crates/storm-render/src/model/sail_damage/reference/generate.py
cargo test -p storm-render damage_indices --locked
```

The checked-in reference contains 7,934 cases: all 5,120 possible damage masks,
full-resolution index-order hashes/counts, every original vertex UV, 2,700
coarse trace cases (including oblique, repeated and endpoint hits), and 112
speed cases. Rust checks these portable results on every CI platform. The
manual generator requires Python 3 and a C++17 compiler; routine tests do not.
GPU tests additionally check alpha-mask changes, flutter/furling, pause and exact
restoration in one persistent scene. Real game artwork is verified locally.


## Validation on 2026-10-18

Code [`9de18547`](https://github.com/STG-44/storm-engine/commit/9de185474a80cf87d9bb7ac94a8b33570b73f88c)
passes [all four platform jobs](https://github.com/STG-44/storm-engine/actions/runs/39077873810):
58 CPU tests (including all 7,934 original C++ cases), formatting, Clippy and
native builds on Linux, Windows, macOS ARM and macOS Intel. Linux also passes
13 GPU tests using software Vulkan and native fixture windows under Xvfb. Its
downloaded damage artifact was inspected; mask state, drive fraction and sailing
speed match a local Metal capture exactly.

Locally, all 40 original ship roots (368 sails) load the original damage texture
and INI coefficients. Deterministic test volleys damage 1,105 sectors (some
segments intersect additional cloth). The original Barkentine renders through
Metal with the supplied mask; all 48 damaged sectors remain recorded on twelve
sails after complete furling. Both its native window and the generated fixture
present three frames. Commercial resources and original-ship captures remain
external; physical Windows/Linux GPU checks with game data are still pending.
