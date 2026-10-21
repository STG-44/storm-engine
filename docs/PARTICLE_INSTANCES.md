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

Explicit XPS cache selection, .prj/atlas loading, complete PARTICLES registration,
automatic global deletion, model/attached particle rendering, audio and native
gameplay retain their preceding unfinished scope. Multiple diagnostic entities
still share one sailing simulation and one preview ship.
