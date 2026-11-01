# Shared ship and sea entity dispatch

The opt-in `--native-ship-stages` mode requires `--native-sea-stages` and its
dependencies. It runs retained ship movement, 36-point rocking and optional
living-pose arithmetic inside each ship entity's Execute callback, using the
same Core EntityManager that dispatches SEA Realize. Earlier modes retain
their preceding behavior and report structure.

## Retained contract

`SHIP::Init` reads optional signed `iShipPriorityExecute` and
`iShipPriorityRealize` script integers, falling back to 2 and 31. Conversion
to the manager's unsigned priorities is retained. The two Mount registrations
add the ship to Realize, then Execute. `SHIP::ProcessStage` delegates its
millisecond delta unchanged; time limiting remains inside the numeric body.

The preview uses explicitly supplied layer indices: Execute 1 and Realize 0.
Both ships use the native default priorities. SEA uses Realize priority 0,
so its numeric preparation precedes the ships' Realize callbacks. Scripts and
the existing sail/cloth input preparation precede ship Execute; ships query
the preceding prepared surface. Full original global layer membership and
SAIL/ROPE entity scheduling remain separate integration work.

Each actor owns a numeric frame during its callback. Checked input/output
copies connect the existing preview's ship bodies, limits, rocking grids and
living pose to those actors. Only the callbacks perform staged body movement;
the old direct body loop is bypassed. Character outputs and GPU model placement
remain preview bridges after Execute. Ship Realize currently records that
bridge and the prepared surface revision; it does not run original model,
lighting, fog, upper-ship or renderer calls. The report explicitly keeps
`full_ship_entity`, `original_model_draw` and `original_mount_services` false.

The fixture retires owner 1 after frame 120. Erase invalidates its entity
immediately; end-of-frame NewLifecycle destroys it and removes both layer
memberships. Owner 2 continues receiving Execute/Realize. The preview retains
the retired body's last values for diagnostics. Fixture effects/group removal
still uses the preceding owner cleanup: the original SHIP destructor's full
SAIL/ROPE/FLAG/VANT/model/blots/tracks cleanup has not been ported here.
Actual engine entity IDs are reported separately from preview owner numbers.

A paused camera change may dispatch zero-time Realize without Execute and
without moving ships. Repeated unchanged presentation is cached. Numeric
callback errors reach the frame owner through an explicit error bridge; no
whole-frame transaction or rollback of other actors/services is promised.

## Native reference and checks

The new oracle compiles unedited SHIP ProcessStage, the retained Init priority
block and the two retained Mount layer calls, alongside the original Core
Execute/Realize loops and original EntityManager/cache/Entity/VMA sources.
Script variables, service access, class factory and body callbacks are explicit
probes. This is not an execution of complete SHIP Init/Mount/Execute/Realize.
Under UBSan, 32 profiles produce 512 complete snapshots covering defaults,
signed/custom priorities, stable ordering, frozen layers, immediate invalidation,
deferred destruction, storage reuse and selective layer removal.

Integration tests compare actual ship/rocking/placement/living-pose outputs with
the preceding direct-body staged-sea mode, compare fixed-step partitions,
verify query/prepared revisions, preserve the retired body while the survivor
moves, reconstruct fresh sessions and check paused/error paths. Invalid layer
pairs are rejected before either registration mutates the manager.

This advances the [marine checkpoint](MARINE_INTEGRATION.md). Original ship
mount/destruction services, native model rendering, global script startup,
sea rendering and naval gameplay remain pending. The whole-engine estimate
remains about 20% (roughly 15–25%); land gameplay is included in that scope.


## Platform evidence, 2026-11-01

Code `7ebac6eb90c579d2c4d3737d8e5639dcfde2e6df` passes Windows x86_64, Linux x86_64 and macOS Intel in
[PR run 41339626007](https://github.com/STG-44/storm-engine/actions/runs/41339626007). Complete job logs
confirm formatting, warnings-denied Clippy, native builds and 404 CPU tests
(135 interpreter) on each of these three hosts. All 105 complete JSON reports
over 35 scenarios match local values without rounding or tolerances, and three
original ZIP SHA256 digests are verified. All 34 preceding reports are unchanged.

The macOS Apple Silicon job is still queued with no assigned runner at this
checkpoint. It has not been validated for this code commit. Four-host completion
and 140 matching reports must not be inferred from the three completed jobs.
The existing run remains queued; no duplicate run was started.

Linux additionally passes 24 software Vulkan tests and 27 fleet-window frames,
including three paused shared-dispatch frames. Local Metal passes 24 GPU tests
and three paused shared-dispatch frames on a bounded retry. An initial window
attempt started without completing presentation; the unchanged baseline and
the retry both completed. Manual input/restart and hosted Windows/macOS physical
GPU coverage remain untested.

The native 32-profile / 512-snapshot fixture regenerates byte-identically.
Original-resource rough/stopped captures with ArabellaShip1, native sea frames,
placement, parameters, sail drive, rolling, ropes and LOD match the preceding
mode pixel for pixel; the rough capture was visually inspected. The earlier
40-root audit was not repeated. Resources, captures and local reports remain
outside Git. The integration limitations above and the whole-engine estimate
of about 20% (roughly 15–25%) remain unchanged.
