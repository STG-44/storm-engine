# Script SendMessage and opaque pointer values

The source subset now binds the original synchronous `SendMessage` builtin to
native Rust entities. It builds the message before looking up the destination,
rewinds its cursor, calls `Entity::process_message` through the manager's safe
lookup and returns the complete `uint64_t` result as `Value::Ptr(u64)`.
An invalid or pending-deletion destination returns a zero ptr without invoking
the entity. The host must explicitly bind dispatch; missing bindings fail.

## Script formats

These rules follow `COMPILER::CreateMessage` in retained
`src/libs/core/src/internal_functions.cpp`:

| Format | Script argument | Native field |
| --- | --- | --- |
| `l` | int, or ref resolving to int | signed int32 |
| `p` | ptr, or ref resolving to ptr | opaque uint64 token |
| `f` | float or int, including refs | float32 |
| `i` | object or initialized aref, including refs | complete entity ID |
| `a` | object or initialized aref, including refs | checked attribute handle |
| `s` | direct string value | owned UTF-8 string |

An `f` field converts an int to float. Through a ref this also changes the
caller's variable type, including when the destination has been deleted.
The `s` branch checks the stack type before dereferencing: a ref to string
fails, although a ref to the format string works. Fields are built in order;
earlier conversion effects survive a later field error. Surplus arguments
are evaluated but ignored. Empty messages are accepted. The subset bounds
calls to 2–130 arguments and messages to 128 fields.

The original `e` format passes a raw `DATA*` into native code. Its safe script
variable equivalent is pending. Unsupported formats, missing fields, invalid
types, expired/removed attributes and foreign attribute worlds fail before
native dispatch. This intentionally replaces unsafe native error paths and
unknown-format fallthrough with explicit errors.

Attribute payloads belong to the runtime's selected World. The native host
receives a mutable borrow of that store for the synchronous call, enabling
checked reads/writes through `a` fields. Handles are borrowed for the call;
retaining them requires checking them again before use. Re-entry is checked,
and script attribute budgets are checked on return. Native side effects before
an error are retained. Original game class dispatch and message codes remain
unfinished.

## Ptr values

`ptr` declarations, parameters, returns, storage, arrays and refs preserve all
64 bits. The owned core message now accepts `p` and exposes `pointer() -> u64`.
These bits never become a Rust memory address or grant memory access.

Retained DATA behavior is ported for signed int-to-ptr assignment (sign extend),
ptr-to-int conversion (low 32 bits), full-width boolean conversion, `!` after
int truncation, unchanged unary minus and comparisons with a ptr on the left
and int on the right. Ref conversions keep their original mutation behavior.
An int script function may return ptr with the original low-bit conversion;
other return mismatches still fail. Pointer arithmetic, pointer-to-pointer
comparison, ptr string concatenation and additional native memory operations
are outside this increment. Global literal initializer coverage is unchanged;
assign a ptr value after declaration.

## Verification

Nine focused Rust tests cover native dispatch, complete 64-bit results,
pending deletion, conversion side effects, direct/ref format restrictions,
aref fields, checked attribute stores, native errors, source locations, limits
and pointer storage/return behavior.

The [message oracle](../rust/crates/storm-script/reference/messages/generate.py)
compiles unedited `CreateMessage`, the live `FUNC_SEND_MESSAGE` case and DATA
ptr setters/getters under UBSan. All 53 accepted cases match Rust, including
full-width IDs, mixed fields, float bits, references, surplus arguments,
invalid destinations and the 128-field bound. Stack, MESSAGE storage, scalar
getters/conversion and entity lookup are instrumented adapters; this does not
execute the full original interpreter or original game classes.

The [ptr oracle](../rust/crates/storm-script/reference/messages/generate_ptr.py)
reuses the existing scalar extraction scaffold without rewriting its fixture.
It compiles unedited DATA conversion, comparison, unary and boolean methods
under UBSan with the retained fast_float 3.4.0 headers. All 208 pointer cases
match, including ref mutations, sign extension and high bits with zero low
bits. Generated fixtures include the extracted methods' SHA-256 digests.

```sh
python3 rust/crates/storm-script/reference/messages/generate.py
python3 rust/crates/storm-script/reference/messages/generate_ptr.py \
  --fast-float-include /path/to/fast_float-3.4.0/include
cargo test -p storm-script --locked entity_api::messages
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/entity_messages.c \
  --report message-script-state.json
```

The eleventh standalone diagnostic sends `lfsiap`, echoes
`0xfedcba9876543210` through `p`, checks a zero result after deletion and reports
all bits as the JSON integer `18364758544493064720`. The JSON artifact preserves
the integer; consumers using floating-point numbers must avoid rounding it.
The ten earlier standalone reports remain unchanged locally.
