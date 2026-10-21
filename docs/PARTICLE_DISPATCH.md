# Named ship particle profiles

Native ship-fire messages now choose a loaded XPS profile by name. Successful
emissions retain a canonical cache key; presentation selects that profile's
billboard descriptions, curves and slots regardless of the smoke/fire channel.
Missing profiles return no particle handle. FirePlace still becomes active,
retains its timing and ignores replacement names until stopped, as in C++.

## Lookup and cache selection

PARTICLES::CreateSystem prefixes resource/particles and appends .xps unless
the last extension already matches it, ignoring case. DataCache lowercases
the resulting name for lookup. Another extension is retained: smoke.ini selects
smoke.ini.xps. Failed lookup allocates no wrapper or creation-capture entry.

The portable key is relative to RESOURCE/Particles. Both slash styles work on
every host. Input is bounded to 512 ASCII bytes; empty components, traversal,
absolute/drive paths and NUL are checked exclusions. The file loader resolves
case-insensitive components and rejects ambiguous paths and directory escapes.
Unsafe names and locale-specific case folding are outside this contract.

Original-resource previews cache Ship_Fire and Ship_Smoke by default.
Repeat --particle-profile to preload up to thirty additional names, for example:

~~~sh
cargo run -p storm-demo --locked -- \
  --model /path/to/RESOURCE/MODELS/Ships/ArabellaShip1/ArabellaShip1.gm \
  --game-dir /path/to/game-data --particle-profile smoke \
  --sail --sail-script rust/apps/storm-demo/fixtures/ship_particle_profiles.c \
  --rig-report selected-profiles.json
~~~

Aliases do not duplicate meshes. Missing, malformed or unsupported explicitly
selected data fails loading. Additional profiles wait for messages and do not
appear in manual fire mode. Reports include canonical emission keys and failed
creation ID zero with profile_not_cached. Additional style rows use diagnostic
kind message, since either channel can select them.

Generated fixtures and installations without RESOURCE/Particles retain the
explicit legacy diagnostic service and unchanged report shape. That service
accepts fixture names without claiming to resolve original XPS files.

## Verification and remaining scope

The reference generator compiles two unedited native CreateSystem/cache lookup
bodies under UBSan. Fourteen case/extension/success/missing queries compare
data-source selection and system/capture counts. Cache contents, allocation,
manager and output are adapters. Rust tests also cover Windows separators,
rejected names, partial failures, ignored active replacements, stop/restart,
default names and source-to-service dispatch. The extended GPU atlas test
requires identical pixels when the named profile uses the opposite channel,
and an unlit image when a different cached name is selected.

Cache selection remains explicit. Native .prj loading, global PARTICLES
registration, other creation messages and dynamic GPU resource loading are
pending. The default atlas, existing shader and prior checked lifetime/count/
world-replay limits still apply. Model/attached particles and remaining
techniques are unfinished.

The subsequent [instance increment](PARTICLE_INSTANCES.md) retains overlapping
generations and separate same-profile instances in both channels. Complete
global manager/RNG behavior remains pending.
Multiple diagnostic entities still share one preview ship. Sound requests have
no playback. Full native SHIP, combat, game programs and saves remain pending.


## Four-platform evidence, 2026-10-21

Code `f7d1e737362c0cb5569b8c3d7b9307d6c5e95da7` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39651914838) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39651901113) on
macOS Apple Silicon/Intel, Windows and Linux. All eight native jobs pass
formatting, Clippy with warnings denied, builds, 289 CPU tests including 135
interpreter tests, and sixteen standalone scenarios. Actual host triples and
test totals are checked from logs. Linux additionally passes all 15 explicit
software Vulkan GPU tests and native-window fixtures. Four archive SHA-256
digests and all 64 complete JSON reports match local results; all sixteen
preceding generated reports remain unchanged.

The final resource audit reloads all 40 original ship roots: 274 locators and
117 default patches retain 822 birth/world rows and prior manual counters
(38,086 created / 10,347 living). All 39 nonempty models also run named-profile
selection with additional smoke.xps, crossed default names, null creation of
an uncached profile and finite world summaries (empty pools have null bounds).
Additional cached profiles leave manual birth counters unchanged. All 40
preceding complete native-fire reports match after applying only the expected
missing-profile creation failures to diagnostic names absent from the real
cache. Locator, sail source/topology and rigging metadata remain unchanged.

Local Metal passes the extended named-profile/cross-channel/clear/pose test.
ArabellaShip1 renders an explicitly selected additional profile at 5.25 seconds
with 39 created/living particles; the image was inspected. A manual twenty-second
Metal capture with that extra cached profile is byte-identical to the preceding
world-phase image. Alias deduplication and rejected missing/traversal/incompatible
options also pass against original resources. Commercial files, captures and
local reports remain outside Git. Whole-profile decoding and damage audits were
not repeated. Prior local native-window timeouts remain unverified; hosted
macOS/Windows jobs establish CPU/build behavior, with physical GPU/window
verification there still pending. Remaining service/instance limits above apply.
