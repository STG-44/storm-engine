# Entity and message foundation

The Rust core now has a small native runtime foundation. It follows the retained
C++ contracts below; it does not load or execute original game scripts.
Development is tracked in [PR #1](https://github.com/STG-44/storm-engine/pull/1),
now open for review.
The fork is public; the original storm-devs repository is archived.

## Compatibility contracts

| Reference | Implemented behavior |
| --- | --- |
| `core/src/entity_manager.cpp::CalculateEntityId` | Global allocation stamp in high 32 bits, slot in low 32 bits, zero invalid ID; erased slots reused only after lifecycle cleanup |
| `EraseEntity`, `GetEntityDataIdx`, `NewLifecycle` | Immediate invalidation, deferred destruction, pending objects available to teardown lookup, stale IDs rejected after reuse |
| `AddToLayer`, `GetEntityIds(type)` | 32 layers; ascending layer index and stable ascending unsigned priority; duplicate insertion does not reprioritize; membership in different layers remains duplicated |
| `core_impl.cpp::Send_Message`, `ProcessExecute`, `ProcessRealize` | Invalid/pending destinations receive no normal messages or stage calls; invalid sends return zero; frozen layers skip stages but do not block direct messages |
| `core/include/vma.hpp::MakeHashValue` | Original ASCII case folding and 32-bit class hash |
| `core/src/message.cpp` | Owned `b w l u f d i c s` values; 32-bit longs/floats, 64-bit entity IDs, signed Dword return bits, checked cursor reads and rewind |
| `compiler.cpp::ProcessEvent`, `s_eventtab.cpp::FindEvent` | Case-insensitive ASCII event lookup, registration order, rewind before each native callback, last non-void result retained |

`storm-core` forbids unsafe code. Format `a` now uses a checked attribute handle;
raw `p/e` pointer formats remain rejected. See the [attribute port](ATTRIBUTES.md).
No C++ addresses are represented as Rust pointers.
Stamp exhaustion returns an error instead of wrapping into an old ID. Invalid
layer indices and message types return errors instead of unchecked memory access.
Class insertion currently accepts an already constructed native object. Factories,
`Init`, class-query caching/order, whole-world reset and teardown that
mutates the manager are not ported. The native callback dispatcher does not support handler mutation or recursive
dispatch. The separate [script event runtime](SCRIPT_EVENTS.md) supports immediate
registration/deletion while script handlers execute; nested events remain pending.

## First integration: sail damage events

Both native X shots and each individual `--sail-volley` segment now dispatch
through the bounded script runtime into newly written source handlers:

- `DoSailHole`, format `llssllllf`: attacker character, target character, `*`,
  original node name, original numeric sail group, current hole count, hole mask,
  maximum hole count (10/12), initialized sail weight / total initialized weight.
- `ShpSailDamage` (`SHIP_SAIL_DAMAGE`), format `lfffl`: target character, original
  world-space hit X/Y/Z, attacker character. Emitted once for the nearest sail
  of each segment, after the per-sail callbacks.

References: `rigging/src/sailone.cpp::SSailTrace/TSailTrace`,
`rigging/src/sail.cpp::Cannon_Trace/AddSailLabel/SetAllSails`, and
`shared_headers/include/shared/sea_ai/script_defines.h`.
Events also fire for repeated hits in already damaged sectors, as in the original.
No-hit segments emit nothing. Character indices are explicit preview inputs;
the default target `-1` suppresses `DoSailHole`, while `ShpSailDamage` still carries
the unknown character. The generated ship fixture has explicitly labeled
synthetic nodes and produces both events. The simpler `--sail-demo` fixture has
no locator identity and therefore only produces `ShpSailDamage`.

The handlers and observer are new diagnostic code, not original game scripts or
entities. The script writes sail masks and the observer counts attribute notifications.
The report keeps a total
count and the latest 256 payloads. `--event-report` saves these as JSON, without
requiring a GPU when combined with `--rig-report`; native X shots update the file.
`sail_volley_with` preserves individual segment boundaries instead of treating the
whole fixture volley as one shot. The existing no-observer API remains available.

```sh
cargo run -p storm-demo --locked -- \
  --ship-demo --sail-volley 3 --rig-report ship-layout.json \
  --event-report sail-events.json --shot-owner 7 --ship-character 19
```

For original `DoSailHole` payloads, use `--model` and `--game-dir` pointing to
separately supplied resources instead of `--ship-demo`.

These callbacks observe completed preview collision results. Original scripts,
handler side effects during collision traversal, free-sail/ship group lifetime,
delayed PostEvent, nested script events, projectile flight and combat integration
remain pending. Immediate source handlers now execute these payloads; complete
script VM and combat compatibility remain under development.

## Verification

Five core tests cover deletion/drop timing, slot reuse/stale IDs, priority ties,
cross-layer duplicates, freezing, direct delivery, numeric widths, cursor failures,
unsupported pointer formats, event rewind/results and retained C++ hash constants.
Integration tests cover the original sail payloads, repeated damage, unknown
characters, no-hit segments, per-sail callback order, nearest-hit selection and
bounded history. The hash constants were checked by compiling the retained
`vma.hpp` on the local Mac.

The workspace now has 106 CPU tests plus 13 separately enabled GPU tests. CI runs the
event-report fixture without commercial resources on all four native build hosts.
The [script source subset](SCRIPT_RUNTIME.md) and [object/reference subset](SCRIPT_OBJECTS.md)
now feed the [immediate event integration](SCRIPT_EVENTS.md). The generated
fixture executes 12 script handlers, with six script attribute writes and six
native damage callbacks. The observer performs no mask writes.
The generated ship fixture produces six `DoSailHole` and six `ShpSailDamage`
callbacks for three shots per sail. A local audit of all 40 original ship roots
checks 368 sails and 2,204 emitted events; every final per-sail event mask matches
the loaded ship state. Barkentine1 contributes 36 per-sail callbacks and 35
nearest-hit callbacks across the fixture segments. Its textured Metal capture
also succeeds. These resource-dependent reports and captures remain outside Git.

At commit `402d469ee72885832cc3f4cec2a305faeb3866ec`, both
[push CI](https://github.com/STG-44/storm-engine/actions/runs/39079242564) and
[PR CI](https://github.com/STG-44/storm-engine/actions/runs/39079245316) pass
formatting, Clippy, all 65 CPU tests, the event fixture and native builds on
Ubuntu, Windows, macOS Intel and macOS Apple Silicon. Linux additionally passes
all 13 explicit software Vulkan GPU tests and native-window fixtures.

The archived C++ CI skips PRs from the `rust-port` integration branch as it already
skips its pushes; PRs from other branches retain the existing C++ checks.
