# Script objects, references and native attributes

`storm-script` now supports `object`, `ref` and `aref`, original attribute
expressions and a first set of original attribute/conversion functions. It
extends the [bounded source runtime](SCRIPT_RUNTIME.md); it still does not load
the game's complete programs. The separate [event integration](SCRIPT_EVENTS.md)
now dispatches preview damage events to diagnostic source handlers.

Run the new resource-free compatibility fixture:

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/ship_attributes.c --report ship-script-state.json
```

The newly written script obtains a persistent host ship through the deliberately
diagnostic `native ref PreviewShip()`. Its `ref` parameter and `aref` locals
update `ship.sails.rey_a1.1.hd = 548` and `ship.sails.rey_b1.2.hd = 274` using
normal Storm attribute syntax. It counts three holes in each sail, enumerates
the tree, checks copied attributes and returns integer `6`.

The ship is bound to a real `storm-core::EntityManager` observer. Six attribute
assignments notify it: character index, two masks, two hole counts and the float
damage value `3.2500000`. A seventh notification comes from modifying a deep
copy: original object assignment copies the entity ID as well as the tree.
That callback reports `hd = 1`, while the persistent host ship still has `548`.
The report includes the complete host ship tree and all seven notifications.
This is a compatibility scenario, not an original commercial game handler.

## Retained language behavior

- Ordinary scalar `=` converts to the receiving variable's current type. For
  example, `int n = 3.9` stores `3`. Compound arithmetic can promote its type.
  The initial scalar implementation incorrectly treated ordinary assignment as
  unconditional `DATA::Copy`; the actual `MOVE` conversion is now implemented.
  The original disabled function-argument type check remains disabled here.
- `makeref(destination, source)` binds a ref to the actual source variable;
  assignment between two refs rebinds. Otherwise writing through ref changes
  the referenced variable. Ref arguments preserve that alias through calls.
- Plain object loads/arguments deep-copy attributes. Object `=` copies the
  entity ID and replaces the destination's contents while preserving its root
  identity. Old child aliases become stale; aliases to the root remain valid.
  Copying through ref from an object with uninitialized attributes preserves an
  already initialized destination tree, matching `DATA::Copy`'s null-source
  branch. A plain object load instead creates an initialized stack copy.
- `makearef(destination, object.path)` creates missing classes. Literal segments
  use `VerifyAttributeClass` (empty value); dynamic `object.(variable)` segments
  use `CreateSubAClass` (absent value). Dynamic variables can contain a complete
  dotted path. Reads use checked lookup and do not create missing classes.
- Attribute expressions read string values. Scalar attribute `=` writes decimal
  integers, strings or floats formatted with the original `%.7f`. Assigning an
  integer through ref can also convert the referenced variable to string, as in
  the original `MOVE` branch. Entity notification occurs after the write, not
  during path creation, copying, enumeration or deletion.
- Original internal functions are available without native declarations:
  `sti`/`makeint`, `stf`/`makefloat`, `CheckAttribute`, `DeleteAttribute`,
  `CopyAttributes`, `GetAttributesNum`, `GetAttributeN`, `GetAttributeName` and
  `GetAttributeValue`. Enumeration preserves order; `GetAttributeN` creates an
  aref without propagating an entity ID. `DeleteAttribute(root, "")` clears
  children and keeps the root value. `CopyAttributes` keeps the receiving node's
  name, parent and identity while replacing its value and children.
- Ref arithmetic follows the retained VM's side effects: a ref on the left of
  arithmetic, or under unary minus, can change the referenced scalar. Comparisons
  detach the left operand. Conditions and `sti`/`stf` can convert the original
  scalar's type; logical evaluation can replace it with `0` or `1`. Invalid `!ref`
  and ref exponents report errors. Tests cover these surprising cases explicitly.

## Ownership and limits

Variables use weak checked aliases; attribute references carry a weak owner and
a never-reused node handle. Cycles, uninitialized refs, expired local variables,
released objects and deleted/replaced child nodes fail with located errors.
Returning an object retains its owned tree; returning an aref to a destroyed
local object does not extend that object's lifetime. No raw C++ pointer escapes
the runtime. Deferred owner release also avoids re-entering attribute storage
while a native entity notification is reading it.

The single-threaded `World` shares one attribute store across calls through an
embedding host. It limits live nodes to 32,768 and interned names to 8,192, using
conservative reservations for copies/paths. Failed lookups can intern names, so
they reserve capacity too. Releasing objects frees their trees; names remain
interned as in the original codec. Source, string, call, stack and instruction
limits from the initial runtime still apply. Copying between different worlds
is rejected. Native handlers remain responsible for their own work and state.

[Globals and arrays](SCRIPT_GLOBALS.md) now persist across calls and frames.

[Entity lookup, deletion and notification control](ENTITY_SCRIPT_API.md) now
expose `IsEntity`, `DeleteClass` and persistent `EntityUpdate` through typed host
bindings. Attribute writes start with notifications enabled in each runtime.

Still pending: pointers, full entity/game API and entity-ID
operators, implicit object/ref conversions beyond this subset, compound
attribute assignments, numeric literal attribute segments and complete original
game programs. Float-to-string `gcvt`
conversion remains unsupported, including float ref values assigned directly
to attributes and floating dynamic path variables. Scalar float attribute
assignment uses its separate original seven-decimal path. There is no new save
format, compiled-program cache compatibility, or complete gameplay claim.

## Verification

The initial object/reference phase covered 97 CPU tests, including 23 script tests.
Current entity bindings and cross-platform totals are recorded in the
[entity API evidence](ENTITY_SCRIPT_API.md#four-platform-evidence-2026-10-19). The scalar golden
fixture now has 9,213 retained-C++ scalar/reference/conversion and attribute
format cases. `DATA::GetVarPointer` and the other retained method bodies are
compiled unedited under UndefinedBehaviorSanitizer. Decimal parsing uses the
actual `fast_float/3.4.0` dependency declared in `conanfile.py`; the attribute
format cases execute the original `%.7f` format. Unsupported `fmt` concatenation
still throws in the generator. Full C++ compiler/program execution is not the
oracle: source-level control flow, lifetimes and host binding have separate tests.

Regenerate the scalar fixture with installed pinned dependency headers:

```sh
python3 rust/crates/storm-script/reference/generate.py \
  --fast-float-include /path/to/fast_float-3.4.0/include
```

The attribute oracle adds a sixth operation sequence using the actual C++ move
assignment from `source->Copy()`. It checks preserved destination name, replaced
contents, self-copy and debugger break behavior. Rust separately checks repaired
parent handles, overlapping source/destination, depth rejection and stale aliases.
CI runs both script fixtures on every native host and publishes both JSON reports.

## Four-platform evidence, 2026-10-18

Code `27222bf33586a6e63b71aa76cd370e4b98fbbcd8` passes the
[PR workflow](https://github.com/STG-44/storm-engine/actions/runs/39120759769)
and [push workflow](https://github.com/STG-44/storm-engine/actions/runs/39120756269)
on Ubuntu 24.04, Windows 2025, macOS 15 Apple Silicon and macOS 15 Intel.
Each host passes formatting, Clippy with warnings denied, 97 CPU tests, native
builds and execution of both script fixtures. Linux additionally passes all 13
explicit software Vulkan GPU tests and the native-window fixtures. Hosted
Windows/macOS jobs do not establish physical GPU compatibility.

All four downloaded `script-state-*` archives have verified SHA-256 digests.
Both complete JSON reports match their local counterparts on every host:
`script-state.json` keeps the earlier result of 6 and three native writes;
`ship-script-state.json` returns 6, emits seven entity notifications and leaves
host sail masks at 548 and 274. The copied object emits its own `hd = 1`
notification without changing the original mask. These are newly written
diagnostic scripts; original game handlers and preview collision integration
remain pending.

Canonical JSON SHA-256 (sorted keys, compact separators):

- Legacy fixture: `019aaddb5fe945aca58edd5c5d5dc7205249e3292de702437eb158c81454a3cf`.
- Object fixture: `6646bebfe318ee2f388ceb9da03d7e98979cb453c8d8e4eb1f74605198f18266`.
