# Complete engine integration sequence

The target is native macOS, Windows and Linux gameplay using unchanged original
Storm-language game programs and assets. Sea, land, combat, audio/interface and
durable saves belong to the completion criteria. Ship-preview progress alone
does not establish the engine's completion percentage.

| Dependency milestone | Acceptance |
| --- | --- |
| Shared runtime startup and core services | One engine host loads configured source, owns script/native objects and frames, exposes original API names and diagnoses missing services. The initial [headless runtime](ENGINE_RUNTIME.md) is a bounded foundation. |
| Real scripted scene through that host | Bounded [MODELR rendering](STATIC_MODELS.md) and a [desktop window/TIMER adapter](NATIVE_WINDOW.md) run through the shared world. Complete model/sea/camera, input/audio and original rendering while extending source/compiler/API compatibility. |
| Integrated naval encounter | Complete Mount/transforms, rig geometry and GPU/service lifetime, sea/weather, controls/camera, cannon/projectiles, swept hits, damage, contact/opponent behavior and sinking. |
| Playable land scene and shared characters | Load a location with animation, player collision/movement, navigation/NPC behavior and combat; reuse these systems for decks and boarding. |
| Full shared services and transitions | Complete sound, interface/dialogues, remaining rendering/resource APIs and sea/land transitions without replacing the script/world ownership model. |
| Durable save/resume and whole-game acceptance | Restore globals, attribute/reference graphs, scheduled events and native entity state after process restart in both environments; validate original-save compatibility separately. Run representative game startup, play and transitions on all targets. |

Original game programs remain compatibility inputs. The engine, native services,
compiler/runtime and API move to Rust; game scripts are not being rewritten in
Rust. Original PROGRAM resources are required for unchanged-script acceptance,
but their absence does not block shared runtime or subsystem implementation.
Raising source limits alone does not establish language/API compatibility.

Existing arithmetic references and GPU tests remain regression foundations.
Future geometry work must feed real entity scheduling, buffer updates and draw
ownership, and future character work must feed actual locations. Each slice
should extend the same executable and acceptance path. CI builds/software Vulkan
are part of platform evidence; physical graphics, input and audio validation
remain necessary. The current full-game estimate remains about 20% (15–25%).
