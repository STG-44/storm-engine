# Retained particle birth accumulation

`storm_sim::particle_emitter` ports the birth/time subset of BaseEmitter and
ParticleSystem. Graph sampling and processor allocation use an explicit
`BirthPort`; billboard and model destinations remain distinguishable.

The following birth milestone is retained as its validation record. Billboard
motion is now integrated in the subsequent [motion increment](PARTICLE_MOTION.md),
which supersedes the movement/forces/spin exclusions below. Historical world
transforms and full global service behavior remain pending.

For each enabled, unattached, running emitter, BornParticles performs 32
substeps. Each substep temporarily advances float32 emitter time and samples
each visible particle type's rate. `remainder += rate * (delta / 32)` preserves
the fractional part between frames. Every whole unit is consumed, including
when the active limit prevents allocation. A successful allocation increments
the active count; a failed processor allocation undoes that increment. Releasing
a dead particle decrements the same count, allowing later births without a
backlog burst. Birth records carry type, kind, substep time and matrix blend.

The substep clock is restored afterward; Execute then advances it by the full
frame delta. The original strict `elapsed > lifetime` check and single lifetime
subtraction are preserved, including overshoot and stopped-emitter clock
updates. Invisible emitters skip birth but advance time. Attached emitters skip
automatic birth but advance time. A paused system skips emitter execution
entirely; external particle aging/release can continue. This increment does not
implement native Restart, SetEnable destruction, attached births or manager
auto-deletion/service bootstrap.

The core accepts finite frame inputs, including zero/negative deltas. Invalid
or overflowing time/rate/remainder data, more than 256 types, active-count
underflow and over 65,536 birth attempts per emitter call return checked errors.
Invalid/unbounded native paths are outside the parity claim; errors can occur
after earlier callbacks in the same call. Full allocation/physics remains a
port, not raw native pointers.

## Random graph ranges

XPS `Graph::sample_range` now follows DataGraph::GetRandomValue/RandomRange:
interpolate bounds, swap inverted limits, use `rand_value % 1000 / 1000.0`,
multiply by the range and add the minimum. Ordinary GetValue retains its
original unswapped interpolation. Original libc global random sequences are
not assumed identical across platforms.

## Renderer embedding

XPS billboards now retain living particle records with stable birth serials,
float32 age/lifetime and persistent appearance factors. The previous mean-rate
slot reconstruction is removed. Each patch/locator feeds its XPS rate curve
through the retained 32-substep accumulator. The processor adapter runs after
birth, as ParticleManager does: newly created particles also age by that frame's
delta, strict `age > lifetime` removes them and swap-removes their record while
releasing capacity. Paused emission freezes its clock/remainder while particles
continue aging. A stopped timestamp prevents further birth.

The preview replays fixed 1/60-second frames, independently of display cadence.
This is an embedding policy: the retained PARTICLES::Realize receives the
actual frame delta in milliseconds and converts it to seconds. Complete frames
are cached; fractional presentation only ages a copy of existing particles.
Redrawing a timestamp cannot consume RNG or create particles. Rewinds, changed
activation/stop timestamps and changed birth configuration rebuild the replay.
A stop inside a fixed frame splits it into emitting and paused parts. Replay is
bounded to 7200 seconds and 16,384 locator keys, accommodating the bounded startup
plus headless interval. Generated/legacy effects retain their preceding preview.

The portable RNG is a per-patch/locator 32-bit LCG; it supplies a 31-bit raw
value for the native range/FRAND formulas. The adapter consumes direction,
initial offset, lifetime/mass/spin/velocity and twelve appearance factors in
the retained billboard initialization order. Unimplemented direction/physics
values are consumed but not applied. It is not the original global libc rand
stream. Separate billboard descriptions currently use separate replay streams;
full multi-type manager-wide RNG ordering remains pending. Each visible patch
still shows the latest activation selected by locator/kind and timestamps;
full overlapping-generation/service identity is not implemented.

```sh
cargo run -p storm-demo --locked -- \
  --model /path/to/RESOURCE/MODELS/Ships/ArabellaShip1/ArabellaShip1.gm \
  --game-dir /path/to/game-data --fire --time 2 --rig-report particle-state.json
```

Rig reports now use the requested `--time`. Real-resource fire reports add
per-description/locator created/living counts, emitter clock and fractional
remainder bits. Headless scene reports expose the same counters. Errors are
reported explicitly. The XPS profiles/default atlas, appearance curves and
native FirePlace lifecycle from preceding increments remain in use.

Original movement/forces/drag, spin, world-space birth matrices/history,
attached/model rendering, global processor capacity, shader techniques, frame
blending, project selection, schema conversion, arbitrary native message
profile dispatch and audio playback remain pending. Matrix blend values are
preserved in the core birth contract; the renderer still positions particles
using the current ship/node transform and XPS tracks.

## References and verification

`rust/crates/storm-sim/reference/particle_emitter/generate.py` compiles unedited
BaseEmitter::BornParticles, Execute, IncreaseTime and ParticleSystem::Execute
under UBSan. Its 35 sequences / 245 complete state/rate/birth snapshots compare
float32 clocks/remainders, all rate calls, billboard/model routing, allocation
failure, capacity, release, pause, visibility, attachment, stopping and overshoot.
Construction, graph samples, matrix/position math, processor allocation and
external releases are adapters. Full original initialization and physics are
excluded. Retained body hashes accompany the fixture.

The XPS field/curve oracle additionally compiles unedited GetRandomValue and
RandomRange with injected rand values: 240 complete range samples match. All
previous ten loaded field states and 282 GetValue scalar/color samples remain
unchanged. Seven added CPU tests cover reference comparisons, limits, release,
frame cadence, repeated draw, rewinds, stop boundaries, independent locators,
aging and capacity recovery. The existing explicit XPS atlas GPU test now also
verifies identical pixels on a repeated draw of the same native emission state.


## Four-platform evidence, 2026-10-21

Code `3840b1f47ff10e7a68f45993e3883bc34f377d11` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39327662142) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39327654395) on
macOS Apple Silicon/Intel, Windows and Linux: 271 CPU tests, including 135
interpreter tests, formatting, Clippy with warnings denied, native builds and
all sixteen standalone scenarios. Host triples are `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-pc-windows-msvc` and
`x86_64-unknown-linux-gnu`. Linux additionally passes all 15 explicit software
Vulkan GPU tests and native windows. Four downloaded archive SHA-256 digests
and all 64 complete JSON reports match local results; all sixteen preceding
scenario reports remain unchanged.

The new resource audit reloads all 40 original ship roots. It verifies 274
fire positions, 117 style patches and 822 description/locator birth counters at
two seconds across the 39 models with locators. The zero-locator model remains
empty. The replay creates 38,086 particles with 10,347 still living, all within
the configured per-description capacity. All 40 complete native-fire source
reports equal the preceding XPS phase. Locator arrays, sail count/topology/source
weights and rigging metadata remain unchanged; older damaged layouts are not
compared for intact aggregate weights. The preceding whole-profile decode of
66 XPS files and earlier damage event/layout audits were not repeated.

Local Metal passes the existing XPS atlas/clear test, including identical
pixels on repeated presentation of the same emission state. Original
ArabellaShip1 renders at two seconds with 973 created / 264 living particles;
the image was visually inspected. Its seven locators yield 21 description
rows; a CPU-only report exposes the same birth counters. Commercial resources,
captures and local evidence stay outside Git. Previous local native-window
timeouts remain an unverified surface/window limitation; that check was not
repeated. Hosted macOS/Windows jobs cover CPU behavior/builds; physical GPU
coverage there remains pending. Full movement/forces, global RNG/processor
ordering, birth transforms, native profile dispatch and audio remain unfinished.
