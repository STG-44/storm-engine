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

Presentation retains the latest emission per locator/channel: restarting can
replace an earlier tail. The same profile in both channels shares presentation
slots. Independent generations/instances and global manager/RNG are next.
Multiple diagnostic entities still share one preview ship. Sound requests have
no playback. Full native SHIP, combat, game programs and saves remain pending.
