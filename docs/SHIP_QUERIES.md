# Native SHIP character and fireplace queries

Two more original `SHIP::ProcessMessage` cases now run in Rust:

| Code | Payload | Output |
| --- | --- | --- |
| `50307`, `MSG_SHIP_GET_CHARACTER_INDEX` | `le` | Current character `index`, written as DATA integer. |
| `50306`, `MSG_SHIP_GET_NUM_FIRE_PLACES` | `le` | Current fireplace-record vector length, written as DATA integer. |

Both return zero on success. The source cases are retained in
`src/libs/ship/src/ship.cpp`; `GetIndex` is in the location include
`character.h`. Its original assertions require a character and an `index`
attribute. Rust returns checked errors for those missing values; it does not
silently invent index zero. An existing index with no value still uses the
retained `GetAttributeAsDword` default. Signed indices preserve the low 32-bit
conversion, and decimal-prefix parsing follows the existing attribute subset.
Native `e` output setters retype the caller's DATA; plain arguments update
temporary stack cells. Invalid/pending entity destinations return zero without
writing outputs, as in the existing message binding.

## Live data and callback context

`ShipQueries<F>` holds a checked, borrowed character attribute ID and the actual
vector of fireplace records `F`. Each index query reads the selected runtime
store again. It uses the explicitly bound character root, which can differ
from `Entity::AttributesPointer`. Changes to the index are visible on the next
query; clearing/replacing native fireplace records changes the next count.
Invalid rebinding leaves the previous character binding intact. Unbound,
foreign and stale character handles fail explicitly.

The core adds an opt-in `process_message_with_attributes` callback and
`send_message_with_attributes` dispatch. These pass the already borrowed
script store into native code, avoiding nested borrows or copied trees. The
default callback delegates to existing `process_message` implementations;
full-width results and the current message cursor are preserved. Active
entities with stale/foreign `AttributesPointer` bindings fail before entering
the callback. Invalid/pending entities still return zero before that check.
Reentrant access remains checked. The original dispatch method remains
available for callbacks that do not require attribute access.

## Preview embedding

The startup host now registers the diagnostic `preview_ship_sails` factory.
`CreateEntity` invokes its native Init with the real caller attributes already
bound. Init binds the character, preserves an existing `index`, or supplies a
missing index from `--ship-character`. That initialization policy is a preview
adapter; it is not the original `SHIP::Init`, Mount or `SetACharacter`.
Separate instances have separate character bindings and fireplace vectors;
they share the single preview sailing simulation by design.

The application supplies `FirePoint` records from the loaded model's
`ShipAsset::fires`, not a fabricated count. The loader already collects the
original case-sensitive `fireplace`/`fireplaces` groups. These are locator
records for the preview, not ported C++ particle/sound `FirePlace` objects.

```sh
cargo run -p storm-demo --locked -- --ship-demo --sail \
  --wind-speed 0 --wind-direction 0 --wave-height 0 --ship-character 19 \
  --sail-script rust/apps/storm-demo/fixtures/ship_queries.c \
  --sail-script-report ship-queries-script-state.json
```

This executes without a GPU or commercial resources. The source initializes
the adapter, reads index/count, changes the index to 27 and -7, verifies plain
temporary output behavior, then deletes the entity and verifies untouched
outputs. Query attributes are included only when a query was used, preserving
all previous complete scenario reports. Replace `--ship-demo` with
`--model /path/to/ship.gm --game-dir /path/to/game-data` to query real locators.

## Reference checks and remaining scope

`rust/crates/storm-sim/reference/ship_queries/generate.py` compiles the unedited
two switch cases, `GetIndex`, `GetAttributeAsDword` and `DATA::Set(int32_t)`
under UBSan. Its 56 sequences / 224 complete snapshots check initial and changed
character values, live fireplace vector resizing, integer retyping, errors and
zero results. MESSAGE transport, character/tree lookup, DATA allocation and
assertion-to-error handling are adapters. Numeric inputs stay within signed
32-bit C long: overflow behavior of `atol` remains outside this reference.
Hashes of retained bodies and constants accompany the generated fixture.

Ten added Rust tests cover those sequences, missing/stale/foreign data,
malformed payloads, actual record destruction on vector clearing, a character
root distinct from entity attributes, legacy callback delegation, cursor and
full-width return preservation, Init reentry, separate entity bindings and
loaded-model record counts. The standalone query fixture is included in each
native CI job.

This adds two queries to the existing two sail handlers. Full SHIP mounting,
original `SetACharacter` side effects, service initialization, particle/sound
objects, `MSG_SHIP_SAFE_DELETE`, damage/fire activation, original physics and
commercial program startup remain pending. The preview keeps its existing
fire/crew/damage presentation; querying locators does not port those native
game systems.


## Four-platform evidence, 2026-10-19

Code `84e76583740f02a939be1c5d54c2a4ca8cda12a8` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39289799260) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39289786087):
241 CPU tests, including 135 interpreter tests, formatting, Clippy with warnings
denied, native builds and all fourteen standalone scenarios on each host.
The four verified Rust host triples are `aarch64-apple-darwin`,
`x86_64-apple-darwin`, `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`.
Linux also passes 13 explicit software Vulkan GPU tests and native windows.
Four downloaded script-state ZIP digests and all 56 complete JSON reports
match local results; all thirteen preceding reports remain unchanged.

Ten added CPU tests cover the retained 56 sequences / 224 complete query
snapshots, context delegation and invalid bindings, cursor/full-width results,
Init reentry, live character indices, separate instances and actual records.
The standalone source performs five deliveries and two ignored sends after
deletion; its native Init preserves an existing index or fills a missing one.

The final code reloads all 40 original ship roots and queries 274 fireplace
locator records, matching their previous model-layout counts. Across these
runs, 200 delivered messages and 80 ignored sends verify live index changes,
record counts and unchanged outputs after deletion. This is a new query audit;
the prior 80 damage/layout reports were not regenerated for this increment.
Original ArabellaShip1 also executes the new source, returns its seven real
fire locators and renders a 12-second sailing state offscreen through local
Metal. The image was visually inspected. Commercial resources, images and
local reports remain outside Git.

The preceding phase's local native-window attempts timed out; that check was
not repeated for this data/query change. Hosted macOS/Windows jobs establish
CPU behavior and builds; physical GPU/window coverage there remains pending.
Full SHIP mounting, original particle/sound objects, SAFE_DELETE and remaining
native game messages remain unfinished.
