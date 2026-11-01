# Ship destructor requests and cascading entity cleanup

The opt-in `--native-ship-cleanup` mode requires `--native-ship-stages` and its
dependencies. It retains the SHIP destructor's service request order and logical
group-deletion behavior, and connects them to the fleet's owner/effect cleanup.
It does not port complete Mount services, geometry allocation or GPU compaction.
All 35 preceding diagnostic modes retain their earlier report structure.

## Registry teardown boundary

Core entities now have a checked `destroy(EntityDestroy)` callback, separate from
Rust Drop. NewLifecycle removes layer memberships, calls destroy while the
invalid entity's registry lookup and attribute binding still exist, then releases
the entry and storage slot. Erasures queued by a destructor drain in the same
LIFO lifecycle. A model/blots dependency cannot be reused before its cleanup.

The context exposes the ID, attribute-root handle and manager. Re-borrowing the
currently executing entity is rejected. Recursive NewLifecycle and Init-time
cleanup defer to the outer boundary. This is a checked Rust policy, not a claim
that unsafe native reentrancy is supported. `new_lifecycle()` now returns Result;
callers observe teardown errors. A failing callback is not retried: its entry is
released, queued dependencies are drained and the first error is returned.
Other entities/services and partially completed teardown are not rolled back.
Original C++ destructors do not supply this Result-based error contract.

## Retained ship and group contracts

The destructor sequence is:

1. Erase the model entity.
2. Send SAIL DEL_GROUP with the ship ID.
3. Send ROPE and FLAG DEL_GROUP with the model ID.
4. Send DEL_GROUP to VANT, VANTL and VANTZ with the model ID.
5. Erase blots.
6. Find ShipTracks, check its pointer and remove the ship if present.

Message codes and `li` payloads are retained. Invalid or pending-deletion service
IDs receive no ordinary message, matching the core's safe lookup. Group handlers
mark the first matching owner as deleted and set the service's pending flag;
they do not compact group storage in the notification. Wrong owner IDs do
not match. Model erasure invalidates it immediately while its pending lookup
remains available during the ship's request sequence.

The fixture binds each ship to explicit model/blots registry proxies and shared
sail/rope/flag/vant group services. These proxy IDs differ from preview owner
numbers. ShipTracks uses a typed adapter, rather than an invented message or
an unchecked C++ pointer. Before erasing a ship, the fixture sends the existing
MSG_SHIP_SAFE_DELETE to its bound fire controller. After the checked destructor
and registry cleanup, the preview releases that owner's particles/controller
and applies the SAIL deleted flag to the retained drive group. The survivor's
groups, model, blots, tracks and effects remain active.

The first_released snapshot now follows lifecycle completion in this new mode.
Repeated erasure is idempotent; rebuilding a fresh session reconstructs all
bindings. Numeric bodies and cloth retain their preceding values and fixed-step
behavior. The retired asset remains stored for diagnostics and is excluded from
drawing. Original GPU buffer release/compaction, MODEL geometry destruction,
complete rigging service updates, native track geometry and full game startup
are still pending. Reports explicitly identify these adapters and omissions.

## Native reference and integration checks

The oracle compiles the unedited complete SHIP destructor, native delete-group
branches, FLAG::GroupSTORM_DELETE and ShipTracks::DelShip, with the original
EntityManager/cache/Entity/VMA sources under UBSan. Message transport, model
destructors, track nodes and service storage are explicit probes. All 64 service
presence masks produce 512 complete snapshots, including ownership marks,
pending lookups, layer removal, nested model deletion, unavailable services,
track removal, repeated erasure and reuse of a freed slot. It does not execute
original geometry/rendering or destructor-triggered allocation.

Core tests additionally check attribute bindings, checked self-borrow, recursive
cleanup deferral and error propagation. Fleet tests cover actual controller /
particle ownership, sail-drive deletion, surviving owners, native numeric and
cloth output, fixed-step partitions, fresh sessions and repeated cleanup.

This advances the [marine checkpoint](MARINE_INTEGRATION.md). Original service
updates/rendering, complete Mount and broader naval gameplay remain unfinished.
Whole-engine coverage, including land, remains about 20% (roughly 15–25%).
