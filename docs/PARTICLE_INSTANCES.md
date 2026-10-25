# Independent ship particle instances

Every successful native fire creation now keeps its own stable service ID.
Stopping a fire pauses its emitters and leaves their particles alive; restarting
adds new instances without replacing earlier smoke. The same XPS profile can
run in both fire channels and at every locator simultaneously. Entity cleanup
still stops active systems while their shared-service tails survive.

## Native contract and replay

The reference is ParticleManager::CreateParticleSystemFromDataSource: every call
allocates a new ParticleSystem from the shared data source and appends it to
Systems. PARTICLE_SYSTEM::Pause delegates to ParticleSystem::PauseEmission.
FirePlace::DeleteParticle pauses the two handles and clears its references;
the manager still owns the old systems when Run creates replacements.

The retained generator compiles those three unedited factory/pause bodies
under UBSan. Eight complete snapshots compare instance retention, identity and
independent pause state across repeated same-source creation. Resource binding,
allocation ordinals, class storage and output are adapters. This reference
does not execute the complete manager, global particle processor or deletion.

Replay keys now combine locator and 64-bit instance ID. Each keeps its birth
counter, emitter clock, remainder, RNG, bodies and world transforms separately.
Stopping/replaying one instance cannot reset another. Rewinds and reclaimed
streams rebuild deterministically. Invisible/expired instances leave the
presentation cache; the service removes stopped records after its existing
bounded retention interval (loaded maximum lifetime, at least seven seconds).
This cleanup policy is a preview bound, not a port of native global deletion.

ID zero preserves the existing caller-supplied legacy stream. Service-created
instances use unique nonzero IDs. The portable RNG incorporates both halves
of that ID, the locator and source name. Legacy/manual ID-zero output retains
its old sequence. Independent streams are an embedding policy: the original
shared libc random sequence and cross-system processing order remain pending.

## Geometry and reports

Particle geometry now includes every currently presented generation. Vertex and
index buffers grow only when needed; smaller frames use a reduced draw count.
Clearing or expiring instances cannot draw stale triangles. All generations in
one profile description share its existing camera-distance sorting pass.
Sorting between descriptions, original blend techniques and global processing
order are not implemented.

The service accepts up to 4,096 retained instances. Exhaustion returns a null
creation result without consuming an ID; cleanup restores capacity. Ship state
rejects duplicate nonzero IDs. Frames/reports are bounded to 1,048,576 particle
vertices across the scene, checked before replay or buffer growth, with GPU
buffer limits checked separately. Direct style replay also bounds stream keys
to 16,384. These are checked preview limits, not original engine capacities.

Native reports list all retained IDs. Per-profile birth, motion and world rows
include instance IDs and cover each presented generation. Manual report shape
and ID-zero counters remain unchanged. The resource-free fire scenario now
lists six instances instead of its former last pair; projecting it to the old
latest-per-channel view reproduces the entire preceding report.

~~~sh
cargo run -p storm-demo --locked -- \
  --model /path/to/RESOURCE/MODELS/Ships/ArabellaShip1/ArabellaShip1.gm \
  --game-dir /path/to/game-data --sail \
  --sail-script rust/apps/storm-demo/fixtures/ship_particle_instances.c \
  --rig-report repeated-fire.json
~~~

That diagnostic creates the same profile in both channels at every original
fireplace, stops the first generation, then starts a second. CPU tests cover
identity/pause, same-time restart, capacity recovery, high ID bits, RNG/motion
isolation, rewind/cache rebuilding, geometry growth/expiry and duplicate IDs.
The Metal/Vulkan atlas test also checks overlapping-generation pixels, repeat
draw equality and stale-geometry removal after shrink and clear.

The subsequent [project increment](PARTICLE_PROJECTS.md) adds startup PRJ/atlas
selection while retaining the explicit XPS presentation subset. Complete PARTICLES registration,
automatic global deletion, model/attached particle rendering, audio and native
gameplay retain their preceding unfinished scope. Multiple diagnostic entities
still share one sailing simulation and one preview ship.


## Four-platform evidence, 2026-10-21

Code `7c7fbf60171c145bddbbe1f7bb94cd4f4a7117fc` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39663253978) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39663244166) on
macOS Apple Silicon/Intel, Windows and Linux. All eight native jobs pass
formatting, Clippy with warnings denied, builds, 297 CPU tests (135 interpreter
tests) and sixteen standalone scenarios. Actual host triples/test counts are
checked from logs. Linux additionally passes 15 explicit software Vulkan GPU
tests and native windows. Four archive SHA-256 digests / 64 complete JSON reports
match local results. Fifteen preceding reports remain unchanged. The fire report
retains six IDs; projecting only the former latest-per-channel view reproduces
the entire preceding report, including all service/message/state fields.

The final-build resource audit reloads all 40 original ship roots. Default
manual output retains 274 locators / 117 patches, 822 birth/world rows and
38,086 created / 10,347 living particles. All 39 nonempty models exercise
same-profile restart in both channels at every locator: 1,096 distinct retained
instances / 2,192 finite world-summary rows include old and new generations.
Named-profile scenarios and extra-profile manual suppression also pass. Forty
prior native-fire reports with uncached diagnostic names retain the preceding
expected null-creation results. Locator/sail topology/source/rig data stay
unchanged. Empty pools retain null bounds and the zero-locator model stays empty.

Local Metal passes the extended overlapping-instance pixel/repeated-draw/shrink/
clear/world-pose test. An inspected ArabellaShip1 restart frame at 4.125 seconds
has 28 instances / 56 rows and 3,942 created / 617 living particles: 56 old and
561 new. The final-build PNG matches that inspected image. A twenty-second
manual Metal capture with an extra cached profile remains byte-identical to the
preceding world-phase frame. Commercial resources, captures and local evidence
stay outside Git. Whole-profile decoding/damage audits were not repeated.
Prior local native-window timeouts remain unverified; hosted macOS/Windows
jobs establish CPU/build behavior, with physical GPU/window coverage pending.
The preview/global-manager limits above still apply.
