# Shared particle project service

An explicitly selected particle project now provides a shared XPS description
library. Native fire messages can request any supported profile in that project
without preloading it with --particle-profile. The existing no-project preview
keeps its preceding explicit-cache behavior and standalone report shape.

~~~sh
# Project-backed native messages; smoke is requested by the script on demand.
cargo run -p storm-demo --locked -- \
  --model /path/to/RESOURCE/MODELS/Ships/ArabellaShip1/ArabellaShip1.gm \
  --game-dir /path/to/game --particle-project default --sail \
  --sail-script rust/apps/storm-demo/fixtures/ship_particle_profiles.c \
  --rig-report selected-effects.json

# Resource-free two-ship scenario and CPU report.
cargo run -p storm-demo --locked -- --particle-fleet-demo --time 3 \
  --particle-fleet-report fleet.json

# Both ships at one second; the surviving ship at three seconds.
cargo run -p storm-demo --locked -- --particle-fleet-demo --time 1 \
  --headless fleet.png
cargo run -p storm-demo --locked -- --particle-fleet-demo --time 3 \
  --headless survivor.png

# The same diagnostic can load an original ship model for both instances.
cargo run -p storm-demo --locked -- --particle-fleet-demo \
  --model /path/to/ship.gm --game-dir /path/to/game --time 1 \
  --headless original-fleet.png
~~~

## Data and instance ownership

Native OpenProject/OpenDefaultProject cache descriptions before system creation.
The portable library likewise decodes every available XPS description in the
selected project, retaining the preceding normalized distinct-name catalog and
path checks. This is the raw supported XPS reader, not complete native schema
conversion or geometry-cache loading. Missing files retain the project report's
missing entries; malformed available files fail project loading. Total source
input is capped at 64 MiB / 9,999 distinct profiles.

Billboard definitions are prepared on demand. Missing names return no handle;
unsupported model descriptions or incomplete billboard fields fail preparation
without caching a partial definition or consuming an ID. Definitions retain the
selected global atlas. Each manager keeps one compiled set per requested profile; distinct instance
keys and separate render scenes retain independent replay state;
sharing the library does not share emitter clocks, RNG, particles or pose history.
The normal map is still retained without changing the color-only shader.

The shared service issues monotonic nonzero IDs, records a full owner token and
routes emissions to that owner. Release uses native swap-remove ordering;
clear preserves identity monotonicity. Releasing one owner's systems does not
pause, renumber or clear another owner's systems. The 4,096-system limit returns
no handle without consuming an ID. Removed handles cannot become valid again.
This checked-token behavior deliberately avoids native pointer-reuse ambiguity.

## Alive, pause and release

The retained IsAlive rule is a running emitter or at least one live particle.
The service applies it to the existing fixed-frame billboard replay and releases
completed systems as the PARTICLES wrapper does. Non-looping effects can finish
without an explicit pause. A paused looping emitter remains alive even after its
visible particles expire: PauseEmission does not mark its emitter stopped. That
native behavior is retained rather than replacing it with a timeout. Explicit
release/owner release or scene teardown frees such systems.

The original wrapper's Stop, SetDelay and SetLifeTime bodies are empty. This
increment does not invent implementations for them. FirePlace's retained timing
and Pause(true) path remain the interface used by ship-fire messages. Complete
AutoDelete/deferred-delete dispatch, arbitrary resume/restart and live project
switching are separate work. Manager liveness uses profile-space replay;
rendering independently retains the world birth histories already implemented.
Native global libc RNG and processor ordering remain pending.

## Rendering and the two-ship diagnostic

A requested profile adds billboard meshes at runtime, including GPU vertex,
index and pose-buffer growth. Cached definitions clone fresh replay state into
each scene. All ships draw opaque geometry into shared depth before particle
draws, with one ocean pass. Transparent particles are still not globally sorted.
The fleet is bounded to sixteen scenes and 1,048,576 aggregate particle vertices;
each scene also retains the existing mesh/GPU-buffer limits. Dynamic profile
meshes stay cached until scene destruction; releasing a system reduces its draw
count and reclaims released replay keys at the next manager advance, without reloading the project's descriptions.

The deterministic diagnostic starts both ships, pauses the first at 0.5 seconds,
restarts it with another profile at 0.75 seconds, and explicitly releases/removes
it at two seconds. The second ship continues emitting. Its poses are prescribed
diagnostic motion, not a multi-ship SHIP implementation or naval AI. The command
supports CPU reports and offscreen captures; it is not an interactive fleet mode.
Land locations, player locomotion on decks, boarding and character combat remain
unimplemented gameplay milestones.

## Verification scope

The native oracle compiles five unedited manager/system bodies under UBSan:
CreateParticleSystemFromDataSource, RemoveResource, ValidateSystem,
DeleteAllSystems and IsAlive. Twelve complete registry/order/validity/release
snapshots and nine emitter-count/stopped cases match. Construction, data-source
contents, retained release storage and output are adapters; the oracle does not
exercise native allocation reuse, complete particle processing or DataSource
conversion. Existing native birth, motion, transform, project and lookup oracles
remain separate checks with their documented scope.

CPU coverage also checks on-demand preparation, partial failures, instance
capacity/identity recovery, separate library consumers, completed one-shots,
paused looping effects and owner isolation. A GPU test starts with both scenes
already created, adds their profiles on demand, checks each half of the image
against its unlit baseline, repeats the draw exactly, releases the first owner's
effect and requires unchanged pixels for the second. The fleet report is the
eighteenth resource-free standalone scenario.
