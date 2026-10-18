# Original attribute tree in Rust

storm-core::AttributeStore ports the native tree operations from retained
src/libs/core/src/attributes.cpp and the ASCII name codec from
src/libs/core/src/string_codec.h. It is the data foundation for script objects;
it does not execute scripts or load an original savegame.

## Preserved behavior

- Ordered children, unconditional append by CreateAttribute, and first-match
  lookup/update when duplicate names exist. Names compare without ASCII case.
  The codec preserves the first interned spelling and the original
  (hash & 511) << 16 | bucket_index name codes.
- Missing value (None) and an existing empty string (Some("")) differ.
  VerifyAttributeClass creates an empty string; CreateSubAClass creates a
  node without a value.
- SetAttribute addresses a literal child name, including dots. Path creation
  and lookup are separate operations. FindAClass("") returns the current node;
  creating an empty path creates an empty-name child. Consecutive and trailing
  dots retain the source's different creation/lookup semantics.
- FindAClass interns names up to the first missing component, even on a miss.
  Literal child lookup does not intern misses. lookup_path is an additional
  read-only native helper that does not change the codec.
- Deep copy preserves names, values and child order and clears debugger break
  flags. Deleting a descendant preserves sibling order. Deleting a root through
  DeleteAttributeClassX(root, root) clears its children, retains the root value
  and returns false, as in the original.
- SetValue signals the debugger break counter, including writes of the same
  value and clearing a value. Literal SetAttribute writes bypass that signal.
  Explicit subtree release signals flagged nodes.
- Decimal-prefix integer/float reads distinguish missing/unset values from
  malformed or empty values: the former return the caller's default, the latter
  convert to zero. Float reads parse as double before narrowing to float.
  Float writes use the source %g convention with six significant digits.
  Named typed setters preserve the zero-based-index-to-bool quirk: writing the
  first child returns false even after a successful write.

## Native safety and limits

Attribute references are checked 64-bit handles, not C++ addresses. IDs are never
reused, including between stores. Null, released and foreign handles cannot
resolve to another node. Message field a now carries an AttributeId; receivers
resolve it through the store. Raw p pointers and e script-variable references
remain unsupported.

Parent handles in copies point into the new tree, fixing the dangling C++ move
temporaries rather than reproducing that memory error. Entity bindings borrow
roots/subtrees and do not destroy them with the entity. drop_root is the
explicit owner release.

Names currently require ASCII without NUL, at most 1,023 bytes. Trees are limited
to depth 256 and one million live nodes; name buckets reject exhaustion instead
of wrapping their 16-bit index. Rejected paths are validated before creating
partial nodes. Values are owned UTF-8 strings, including empty strings.
The name codec and raw handles are process-local and are not a save format.

Integer reads deliberately use portable low-32-bit decimal arithmetic. Outside
the host C long range, the original atol behavior is undefined or platform
dependent; no parity is claimed there. The first float parser covers decimal
prefixes, infinity and NaN; C hexadecimal-float spelling, locale-specific numeric
rules and NaN payloads remain pending. Raw pointer-valued attributes, non-ASCII
name/legacy-encoding rules and the compiler's complete attribute API also remain
pending.

SetBreak models debugger notifications, not an entity callback. As in
CoreImpl::Entity_SetAttribute/Entity_AttributeChanged, assigning an entity
attribute and notifying its entity are distinct operations. Attribute access
and explicit notifications still work during deferred entity deletion, while
ordinary entity messages use the safe valid-only lookup.

## Sail damage integration

The native damage observer has a bound attribute root. It writes each target's
data under a new diagnostic characters.[index] container. The subtree inside
that container follows the original character layout:

~~~text
index = "19"
ship
  sails = "22"          # maximum possible hole count, as SetAllSails initializes
    rey_b2
      1
        hd = "548"     # SetAllHole mask for this node/group
    mast1
      1
        hd = "274"
~~~

The example is generated entirely from the resource-free ship fixture.
DoSailHole writes the hd leaf and then explicitly calls the native entity's
AttributeChanged. Restoring masks follows SetAllSails/FindSailFromData:
case-sensitive original node name, numeric sail group, then hd read and
SetAllHole's 10/12-bit mask. Unknown nodes/groups are skipped. The native
0-key repair clears the target character's persistent sail subtree as well as
the visible damage, leaving other character trees intact.

--event-report now also exports the ordered tree, notification count, number
of restored sails and masks reconstructed from attributes. For example:

~~~sh
cargo run -p storm-demo --locked -- --ship-demo --sail-volley 3 --ship-character 19 --shot-owner 7 --rig-report ship-layout.json --event-report sail-events.json
~~~

This remains a native observer, not the game's original DoSailHole script.
Original script variables, complete compiler/VM execution, script handlers and savegame persistence
are separate work. The characters wrapper and JSON report are new diagnostic
code and do not claim compatibility with an original save file.

The separate [executable script subset](SCRIPT_RUNTIME.md) now exercises this
tree through explicit native preview bindings. Its original object/attribute
syntax and game damage-event integration remain pending.

## Verification

rust/crates/storm-core/src/attributes/reference/generate.py compiles the
unchanged retained ATTRIBUTES.cpp, attributes.h, STRING_CODEC and string
comparison headers. Only assertion/platform wrappers and the debugger callback
are stubbed. Checked-in fixtures cover five operation sequences (ordered trees,
name codes, missing values, path edge cases, copies, deletion, break counters and
return values), 39 numeric prefixes and 2,051 finite float-format samples.
No game resources or compiler are needed to consume these fixtures in Rust CI.

Native tests additionally cover stale/foreign/null handles, bounded paths,
copied parent chains, shared borrowed entity roots, explicit notifications,
damage-mask roundtrips, high mask bits and repair isolation between characters.
The workspace currently has 73 CPU tests plus 13 separately enabled GPU tests.
CI checks the generated attribute/mask roundtrip on all four native build hosts
and uploads the JSON reports.

Verified at commit 67d0324d8ae797436e68d43530385aa892d7a44b on 2026-10-18:

- [PR CI](https://github.com/STG-44/storm-engine/actions/runs/39117143046)
  passes all four native jobs: formatting, Clippy, 73 CPU tests, native builds
  and attribute roundtrips. Linux also passes 13 Vulkan GPU tests and windows.
- The downloaded attribute reports from all four jobs contain identical trees,
  notification counts and reconstructed masks. Canonical selected-state SHA256:
  9490c2b11d21fcdab6e54545370a21357b590a582493a4ab40ef779dc8e80548.
- The local original-resource audit covers all 40 ship roots and reconstructs
  all 368 sail masks exactly, with 1,105 explicit attribute notifications.
  The full reports and original resources remain outside Git.
