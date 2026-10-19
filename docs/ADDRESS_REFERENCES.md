# Original address references in source expressions

The bounded source runtime now accepts the original `&variable` and
`&array[index]` spelling. It produces a checked `ref`, following
`expression.cpp::CompileExpression` and `compiler.cpp`'s
`SETREF`/`SETREF_BXINDEX` paths. Common source calls now work directly:

```c
object ship;
CreateEntity(&ship, "script_attribute_observer");
int values[2];
SetArraySize(&values, 3);
```

The example class is a registered diagnostic fixture; the native game classes
remain unfinished. See the [creation scope](ENTITY_CREATION.md).

The operator selects the actual variable or array cell without the ordinary
object-load deep copy. Applied to a ref, it follows `GetVarPointer` immediately.
Later argument evaluation can rebind the original ref without changing the
target already captured by `&`. Index expressions are evaluated once. Addresses
of objects, scalars, whole arrays and arefs use the existing reference semantics.

The retained grammar handles `&` at the start of a complete expression. Its
operand is a declared variable or one array cell. It rejects literals, function
results, attribute paths, grouped address operands and trailing arithmetic.
An address index starts at original L2 (addition and tighter arithmetic);
comparison within the index requires a group, for example `&values[(1 < 2)]`.
This preserves the original grouped L1 behavior. Existing `&&` tokenization is
unchanged.

Reference storage remains weak and checked. An address does not retain a local
variable, a removed array cell or a destroyed runtime. Runtime globals stay
alive across calls/frames, while fresh `Program::call` storage is temporary.
Uninitialized refs, expired aliases, invalid indices and foreign attribute
worlds retain their existing located errors and budgets. Numeric pointer values and script `SendMessage` are covered by the later
[message binding](ENTITY_MESSAGES.md).

## Verification

Six source tests cover scalar mutation, alias capture before later argument
side effects, indexed entity creation, aref identity, array resizing, single
index evaluation, index grammar, located errors and escaping/global lifetimes.
They include the complete creation report comparison below. The source and
SETREF contracts are checked against retained code; this is not execution of
the complete original compiler. The existing DATA oracle continues to compile
the unedited `GetVarPointer` method.

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/entity_creation.c \
  --entry AddressMain --report address-script-state.json
```

The tenth standalone scenario runs the same creation sequence using `&ship`.
Its complete report equals the ninth scenario that passes an existing ref:
return 19, six notifications, silent Init writes, failed replacement preserving
identity, null-binding class creation and deletion of the prior identities.
The nine preceding reports remain unchanged.

## Four-platform evidence, 2026-10-19

Code `f48256f14cacf4e9a968b051780c8f9b172eb95e` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/39202728588) and
[push run](https://github.com/STG-44/storm-engine/actions/runs/39202725517):
203 CPU tests, including 116 script tests, formatting, Clippy with warnings
denied, native builds and all ten standalone scenarios on each host. Rust host
triples verified from logs are `aarch64-apple-darwin`, `x86_64-apple-darwin`,
`x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`. Linux additionally passes
all 13 explicit software Vulkan GPU tests and native-window fixtures. Physical
Windows/macOS GPU coverage remains pending.

Four downloaded PR script-state ZIP archives have verified SHA-256 digests.
All 40 complete parsed JSON reports match local results. The tenth address
report equals the ninth creation report, and all nine preceding reports remain
unchanged. Both creation spellings return 19 with six notifications and the
same complete persistent tree and identity checks.

The local resource-dependent audit still checks all 40 ship roots and 368 sail
masks through 2,204 source handlers, 1,105 notifications and 1,099 native damage
callbacks. All 80 complete layout/event reports equal the preceding phase.
The generated ship presents three native Metal frames. Commercial resources
and dependent reports stay outside Git. Original native game classes and complete commercial startup remain pending.
The later [message binding](ENTITY_MESSAGES.md) adds opaque numeric pointer
values and script SendMessage; raw script-variable message pointers remain pending.
