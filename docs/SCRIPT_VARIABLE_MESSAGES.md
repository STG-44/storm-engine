# Native script-variable message fields

Synchronous script `SendMessage` now accepts the original `e` format. Retained
`COMPILER::CreateMessage` calls `GetVarPointer()` and puts the resulting DATA
cell into the message. Native code reads it with `ScriptVariablePointer()`
and can write output parameters. For example, the original ship's
`MSG_SHIP_GET_SAIL_STATE` writes a float through this field; sound volume
queries write three floats, and character queries fill preallocated arrays.
These are reference contracts; those commercial classes are not yet ported.

```c
int value;
SendMessage(ship, "le", command, &value);
```

The field resolves a ref to its actual target while building the message.
Repeated refs to the same cell alias each other, including through ref chains.
Native reads/writes then observe that cell, including conversions made by
later fields. Plain arguments use independent stack cells; changing them does
not change the caller. Ordinary object-load copying remains in the compiler.

## Checked native API

`storm-core` exposes a VM-independent `ScriptVariable` handle through
`Message::script_variable()`. It supports:

- Exact int32, float32, UTF-8 string and opaque uint64 ptr reads. Getters do not
  perform script conversion. The resolved kind is available separately.
- Native scalar setters that retype the cell, matching DATA::Set rather than
  assignment conversion. NaN is stored before the original `NAN ERROR` is
  reported. Oversized/NUL strings fail before mutation.
- Array length and checked element handles. Scalar array writes, scalar
  indexing, negative indices and out-of-range indices fail.
- Attribute roots, complete entity IDs and entity-ID writes using the selected
  mutable attribute store. Existing object trees and aref snapshot identity
  are preserved. An ID written to a scalar produces an owned object.
- Cell identity comparison without exposing an address.

The implementation uses weak cell references. Retained global handles keep
working across sends and runtime calls, while local cells expire on function
return. Removed array cells and all cells of a destroyed runtime expire.
The message builder owns plain-argument temporary cells only until synchronous
dispatch returns, including error paths. Cloning a message or a native handle
does not keep those cells alive. Native errors retain prior reference writes.

Attribute operations validate the owning world, selected store and current
node. A stale owner/node or a foreign world/store returns an error. Variable
re-entry uses checked borrows. The native entity setter uses the borrowed
store, avoiding a nested mutable borrow of the World. Script attribute budgets
are checked on native callback return, as for the existing message binding.

## Scope

This is a checked subset of VDATA, not a raw C++ DATA pointer. Script event
`e` fields/GetEventData, native ref/aref rebinding and array resizing through
VDATA remain pending; existing script SetArraySize is unchanged. Scalar
GetAClass and entity-only uninitialized arefs are not represented by this
subset. Rust scalar retyping releases an object's old attribute ownership;
preserving C++'s dormant AttributesClass through scalar retyping remains pending.
Reachable object trees are preserved by native entity-ID assignment.

The core kind enum describes resolved Rust values, including an Array kind;
it is not the numeric C++ S_TOKEN_TYPE ABI. Pointer tokens grant no memory
access. The diagnostic class and message codes below are newly written
compatibility fixtures, separate from original commercial gameplay.

## Verification

Ten focused script tests and one core message test cover scalar retyping,
exact reads, aliases, ref capture, field order, objects/arefs, full IDs,
arrays and bounds, local/global/temporary lifetimes, runtime destruction,
foreign/stale attributes, re-entry, string budgets, NaN and native errors.

The [reference generator](../rust/crates/storm-script/reference/messages/generate_variables.py)
compiles unedited CreateMessage, the live SendMessage case, GetVarPointer and
ten DATA getters/setters under UBSan. All 120 cases match Rust's complete
result snapshots: initial/updated types and values, caller state, alias
identity, native call count and uint64 return. Stack/MESSAGE storage, safe
lookup and the native diagnostic are instrumented adapters; by-value stack
DATA is explicitly prepared. This oracle does not execute the complete
original compiler, object-copy routine, original game classes or gameplay.

```sh
python3 rust/crates/storm-script/reference/messages/generate_variables.py
cargo test -p storm-script --locked objects::native
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/variable_messages.c \
  --report variable-script-state.json
```

The twelfth resource-free scenario performs five native deliveries. It writes
int/float/string/ptr outputs, fills three array cells, leaves a plain argument
unchanged, retains a global across two sends and skips a pending-deletion
destination. It returns 19 with one pre-construction observer notification.
The eleven preceding complete standalone reports remain the regression baseline.
