# Native factories and script entity creation

The Rust core now registers native class factories and runs `Entity::init`.
The source runtime exposes original `CreateEntity(destination, class)` and
`CreateClass(class)` without native declarations. This extends the
[entity lookup/deletion API](ENTITY_SCRIPT_API.md); original commercial ship
classes and complete original game programs are still pending.

## Lifecycle and identity

`EntityManager::register_class` matches ASCII names without case sensitivity.
The first registration wins; registering a duplicate returns false. Factory
lookup compares the complete name, so distinct names with equal Storm hashes
remain separate. Original class queries use the hash and can return both.
Their results now retain allocation-stamp order after slot reuse, matching
`EntityContainerCache::Add` rather than storage-slot order.

`create` constructs an instance, assigns its full 64-bit ID, registers it and
binds its externally owned attribute root before calling `init`. `EntityInit`
provides that ID, optional root, mutable attribute store and manager access.
Init can query its registration, populate attributes, add layers, delete
entities and construct children. Attribute population does not automatically
send a script notification. A successful Init returns the assigned ID even if
the initializer marked its own entity deleted, as in the retained source.

An Init returning false invalidates the new instance and returns zero. Storage,
attributes and layer memberships remain accessible for teardown until the next
lifecycle cleanup. Its attribute writes and separately created child entities
are not rolled back. Missing classes and constructor errors consume no ID.
Replacing a script object's entity ID does not erase its previous native entity.

`CreateEntity` returns integer 1 on success or 0 when Init returns false. The
destination's ID changes only after successful Init; failure preserves its
previous ID and tree, including native Init side effects. As in the retained
stack loader, passing an ordinary object deep-copies it. Use a `ref` to change
the caller's object or an actual indexed array cell. A plain aref shares its
tree but copies its entity ID; a ref to an aref changes that variable's ID,
without changing the owning object or previously copied arefs.

`CreateClass` passes no attribute binding to Init and returns a script object
with the created ID. Later accessing that object's attributes does not bind
them to the native entity automatically. The existing DATA null-source copy
rule can preserve an assignment destination's previous attributes.

## Checked boundaries

The host explicitly supplies `create_entity(class, store, optional_root)`; an
embedding without that hook reports an error. The diagnostic host registers
`script_attribute_observer` and `script_attribute_reject`, which are newly
written fixtures, not original ship classes.

The core limits the registry to 256 names, allocated slots to 4,096 and nested
Init to 32 frames. Class names are non-null ASCII, at most 128 bytes and have
nonzero Storm hashes. Pending deletion still occupies a slot. Existing direct
`insert` bypasses Init for already constructed instances.

Native instances use checked callback borrows. `get` now returns an `EntityRef`
view with identity and a fallible read borrow. It still finds pending or
initializing instances; borrowing an instance while its callback is running
returns an error. Stage dispatch now returns `Result` too. Lifecycle requests
during Init defer cleanup until a later call after Init returns, preventing
destruction or reuse of an active initializer. Arbitrary reentrant C++ pointer
access is not emulated.

Native Init operates on the selected attribute store. Re-entering the script
World or supplying a foreign root produces a checked error. Native callbacks
are trusted embedding code; the smaller script node/name budgets are checked
again after Init. If a successful native creation exceeds those budgets, the
unreturned entity is invalidated. Expired stack-copy attribute roots remain
checked stale handles rather than dangling C++ pointers.

A Rust Init returning `Err` queues the instance for cleanup. This is a safety
difference from a thrown C++ Init, which leaves the already registered instance
valid. Unknown classes and invalid types/aliases report located errors.
`CreateClass` with a false Init reports an error rather than continuing with
the original missing return value. The diagnostic C++ error-log text is not
reproduced. These boundaries are explicit; complete VM/cache/message-pointer
compatibility is not claimed.

## Verification

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/entity_creation.c \
  --report creation-script-state.json
python3 rust/crates/storm-core/src/entities/reference/generate.py
```

Ten native tests cover registry duplicates/hash collisions, registration and
attribute binding before Init, nested creation, rejection/error cleanup,
deferred destruction, identity reuse/order, recursive/capacity limits and
checked self access. Ten source tests cover object/ref/aref and indexed identity,
persistence across posted frames, null bindings, failed replacement, stale or
foreign roots, smaller script budgets and the resource-free diagnostic.

The independent native oracle compiles six unedited retained files: the entity
manager/header, class-query cache/header, Entity and VMA. It runs 15 sequences /
404 complete snapshots under UndefinedBehaviorSanitizer, including construction,
pre-Init visibility, rejection, nested creation, self-deletion, failed factories,
hash collisions, class order, layer memberships, attribute-write counts, pending
lookups and destructor order. The plf stack adapter, minimal attribute payload
and diagnostic class/Init bodies are scaffolding. It does not run the original
script compiler, commercial classes, or the unsafe throwing/reentrant paths.

The ninth standalone script scenario binds a new observer to the persistent
ship tree, checks a rejected replacement, creates a class with null attributes
and erases the old identities. It returns 19 with six notifications. Init
population remains visible in the tree without notifications; the replacement
is valid and the previous entity is invalid.

## Four-platform evidence, 2026-10-19

Code `044efa10faf3955e0552dc08fb11a680e3073386` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39175675359) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39175672054):
197 CPU tests, including 110 script tests, formatting, Clippy with warnings
denied, native builds and all nine standalone scenarios on each host. Verified
Rust host triples are `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`. Linux also passes all
13 explicit software Vulkan GPU tests and native-window fixtures. Hosted
Windows/macOS physical GPU coverage remains pending.

Four downloaded PR script-state ZIPs have verified SHA-256 digests. All 36
complete parsed JSON reports match local results. All eight preceding reports
are unchanged. The ninth returns 19 with six notifications and retains the
silent Init/rejected-Init attribute writes, successful replacement identity,
null-binding class creation and deletion of the previous identities.

The native C++ oracle passes 15 sequences / 404 snapshots under UBSan. All
40 original ship roots and 368 sail masks pass through 2,204 source handlers,
1,105 notifications and 1,099 native damage callbacks. All 80 complete fleet
layout/event reports match the preceding phase. The generated ship presents
three native Metal frames. Commercial resources and reports stay outside Git.
These results verify this creation framework and preserve existing previews;
original game classes, script message pointers and commercial startup remain
pending.
