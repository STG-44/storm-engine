# Integrated marine checkpoint

Sequencing decision, 2026-11-01: complete a coherent marine gameplay checkpoint
before starting the first playable land location. The preceding ship/sail
increments establish substantial numeric and presentation behavior, but a
complete naval gameplay loop has not yet been assembled. The ship-demo estimate
in [port status](PORT_STATUS.md) is separate from naval-gameplay completion.

Finishing every marine feature before land would also require shared character,
collision/navigation, animation, interface, audio and persistence systems.
Boarding and player movement on decks use much of the same foundation as land
play. Build those shared systems across both environments after the checkpoint;
do not require full naval AI, every weather effect or final visual polish first.

## 1. Sea and coupled simulation

First increment: [scalar sea surface](SEA_SURFACE.md) now retains bump/normal
preparation, frame interpolation and WaveXZ, coupled to ship rocking and the
preview sampler. The [weather-attribute increment](SEA_WEATHER.md) adds numeric attribute changes
and script/entity notification dispatch. The [camera/stage increment](SEA_STAGES.md)
adds native view extraction and query-before-preparation timing in the fleet
fixture. [Shared ship dispatch](SHIP_STAGES.md) now moves the retained numeric
bodies inside ship Execute callbacks on that same manager. Full mount/destructor
services, SAIL/ROPE entity scheduling and original model drawing remain pending.
The [ship-cleanup increment](SHIP_CLEANUP.md) retains destructor request order,
logical group marks and checked cascading registry release. Original geometry
destruction, service compaction and complete Mount remain pending.
The remaining items below include full weather,
service/camera scheduling and original rendering integration.

- Port original SEA height/normal sampling and its prepared wave-frame inputs.
  Start with `SEA::WaveXZ` in `src/libs/sea/src/sea.cpp` and the code producing
  `pSeaFrame1/2` and `pSeaNormalsFrame1/2`. Compare unedited C++ behavior with
  synthetic reference inputs before exercising original resources separately.
- Feed that same sea state into the retained 36-point ShipRocking calculation
  and the water renderer, with explicit coordinate/precision checks. The current
  two-sine diagnostic in `storm-sim/src/waves.rs` is not original SEA.
- Connect shared wind and basic calm/rough conditions to sea, sails and motion.
  The complete [weather/day-night milestone](WEATHER_ROADMAP.md) remains broader.
- Integrate the needed SHIP/SAIL/ROPE lifetimes, registration, update ordering and
  damage cleanup through original attributes/message contracts. Preserve the
  validated numeric methods and documented camera-presentation behavior.

Acceptance: steering and sail requests run over the same changing surface that
is drawn; pause, restart and independent owner removal retain correct state.
Queries and state transitions have native reference checks. Rendering agreement
uses a stated precision tolerance rather than assuming GPU bit equality.

## 2. One playable naval encounter

- Add actual cannon/projectile updates and swept hit queries using the retained
  `src/libs/sea_ai/src/ai_cannon.cpp` and `ai_balls.cpp` as references.
- Route hits through sail, mast and hull damage, fire and sinking state. Damage
  must follow a shot/hit, rather than a manual preview toggle.
- Connect ship/ship and ship/island collision detection and response. Existing
  TouchMove prediction alone is not contact resolution; inspect `src/libs/touch`
  and `src/libs/island` for the missing service behavior.
- Add one minimally controlled opponent and the essential combat feedback:
  selected ammunition, reload/shot state and damage. Use original event/attribute
  contracts as they become available; record bounded compatibility fixtures
  separately from full original game startup.

Acceptance: a repeatable scenario covers maneuver, fire, hit, damage and sinking,
including a collision case. Player actions trigger the integrated state/effects;
restart cleans up entities/effects without stale references. Run CPU/native
references and representative scenes on the supported desktop targets. This is
a defined checkpoint, not a claim that forts, all ammunition/AI tactics, saves
or complete original game programs already work.

## 3. Land and shared character systems

After that checkpoint, assemble the first land location, then implement player
movement/collision, interaction and combat. The same character foundation will
support decks and boarding. Shore01 resources have been inventoried as a useful
candidate; no playable land scene exists yet. Island geometry needed for the
marine collision work can be introduced earlier without claiming land gameplay.

Full weather/lighting, animation, audio, interface, persistence, game scripts
and broader naval AI continue as shared integration work. Keep original assets
external and retain source/behavior comparisons for each increment. The overall
planning estimate remains about 20% (roughly 15–25%); changing the work order
does not change implemented coverage.
