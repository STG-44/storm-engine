# Port status estimate

Planning snapshot, 2026-11-01, after shared ship/sea entity dispatch:

| Target | Approximate functional coverage |
| --- | --- |
| Native engine capable of running the complete original game on macOS, Windows and Linux, including land gameplay | About 20% (rough range 15–25%) |
| Ship presentation and sailing demonstration, including remaining polish and platform verification | About 70% |

These are subjective estimates of functional scope with substantial uncertainty,
not measurements of files, lines, tests or elapsed work. The denominator is the
complete original game running through the Rust engine with its original assets
and scripts. The full-game estimate gives considerable weight to the large
subsystems still pending; completing a ship milestone does not imply a fixed
percentage gain. About 80% of that scope remains, with the same uncertainty.
The [milestone checklist](RUST_PORT.md#milestones) and reproducible checks describe
the actual implemented scope.

Working foundations include native builds, GPU rendering, original ship assets,
sail sets/rigging/flags and damage presentation, entities, messages, attribute
trees and a bounded source-language runtime with events, persistent globals,
one-dimensional arrays and bounded multi-file source loading. Retained native
ship movement, rocking, sail drive, rolling, cloth and rope-constraint arithmetic
now have reference comparisons; their surrounding services remain incomplete.
Original resources have been exercised on all 40 available ship roots. CI covers
Windows, Linux and both macOS architectures; physical GPU coverage remains
limited. The [latest shared ship-dispatch validation](SHIP_STAGES.md#platform-evidence-2026-11-01)
checks 404 CPU tests on Windows, Linux and macOS Intel, with 35 scenarios and
105 matching complete reports. Apple Silicon remains queued and is not yet
validated for this increment. Retained movement/rocking now runs in ship Execute
callbacks on the manager that prepares SEA in Realize, with native priorities
and deferred registry cleanup. Full mount/destructor services, global entity /
camera integration and original rendering remain pending. These checks establish
the documented scope, not a percentage of the engine.

The [weather-attribute increment](SEA_WEATHER.md) now adds original numeric
amplitude/animation/movement controls and stop/resume through a script/entity
fixture. This is another bounded integration step; full weather and native
entity/camera scheduling remain pending.

The [camera/stage increment](SEA_STAGES.md) adds native view extraction and
Execute-before-Realize numerical timing through a bounded entity adapter.
Original global entity/camera integration and sea rendering remain pending.

The [shared ship-dispatch increment](SHIP_STAGES.md) runs retained movement and
rocking inside actual ship Execute callbacks, with native priorities and deferred
registry cleanup. Numeric input/output and model presentation remain preview
bridges; full mount/destructor services and global entity integration are pending.

The next priority is an [integrated marine checkpoint](MARINE_INTEGRATION.md):
original sea/coupled simulation and one playable naval encounter before the
first land location. Shared character/deck/boarding, interface, audio and save
systems span both environments; completing every marine feature first is not
the sequencing goal. This plan update does not change the coverage estimate.

## Scope still to cover

| Area | Current position |
| --- | --- |
| Shared engine foundation, formats and scripts | Implemented in bounded slices; complete script programs, native classes and API remain pending. |
| Ship display and sailing | An extensive demo and growing native numeric ports; complete sea, combat, AI and game service integration remain pending. |
| Land locations: towns, jungle and interiors | No playable location-loading/movement loop yet. GM/TX and animation readers are reusable foundations. |
| Player characters, NPCs and land combat | Movement, collision/navigation, interactions, combat and AI remain pending. Animated deck-crew previews do not establish these systems. |
| Player movement on decks and boarding | Playable deck movement, transitions and boarding combat remain pending. |
| Weather and time of day | [Separate unfinished milestone](WEATHER_ROADMAP.md), including sky, fog, rain, storms and gameplay integration. |
| Interface, audio and save/load | Complete native services and their connection to original game scripts remain pending. |
| Complete game and platform acceptance | Original startup/gameplay/save-resume loops and representative physical GPU/input checks across all targets remain pending. |

The largest remaining blocks are complete source/module loading and language
support, the original native game API and handlers, locations and their collision,
interface, sea/land gameplay and AI, complete animation/audio, save/load and
representative end-to-end gameplay on all target platforms. The default diagnostic sea is a prototype;
the opt-in scalar SEA pipeline retains original numeric behavior through
explicit frame, weather/camera and scheduling providers. Complete original sea
rendering and service integration remain pending. The full original game cannot yet be
launched through the Rust implementation. The ship-demo estimate must not be
used as the completion estimate for naval gameplay, land gameplay or the engine.
