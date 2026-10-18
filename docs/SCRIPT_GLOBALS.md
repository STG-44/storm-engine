# Persistent script globals and arrays

`storm-script::Runtime` now keeps source globals, their object attribute trees
and checked references between script calls, immediate/nested events and posted
frames. The first entry creates storage in the World supplied by Host; later
entries reuse it. Separate runtimes have separate declarations and values.
`Program::call` remains a one-shot helper: globals start fresh for each call.
Script side effects before an execution error are retained, including globals.

The compiler collects top-level declarations before resolving function bodies.
Global names are ASCII case-insensitive; function locals take precedence over
globals. Locals still must be declared before their first source use. Functions
and variables have separate name tables. Types include int/bool, float, string,
object, ref and aref. Global scalar initializers are matching typed literals;
int/float/string arrays accept literal brace lists with zero/empty defaults for
omitted elements. Negative numeric literals and comma declarations work.
Initializers are not arbitrary executable expressions.

## Indexed data and lifetime

One-dimensional global and function-local arrays support integer index
expressions, assignment, compound arithmetic, increment/decrement and object
attributes such as `Characters[i].ship.sails.hd`. An assignment's index executes
once. Each invocation and recursive frame gets fresh local arrays. `makeref`
and `makearef` can select indexed cells/objects. A direct scalar element load
copies its value. As in the retained compiler's ref-variable read path, reading
an element through a ref to the array produces a ref; arithmetic/conversions
can therefore mutate the original cell. Direct object loads use the existing
checked deep-copy rules; object/ref/aref arrays retain checked identities.

`GetArraySize(ref)` returns the current size. `SetArraySize(ref, positive_int)`
keeps the prefix, discards the tail when shrinking and creates default values
when growing. Both require an explicit ref to an array, for example:

```c
int Masks[2] = {548,274};
int Main() {
    ref masks;
    makeref(masks, Masks);
    SetArraySize(masks, 3);
    Masks[2] = 1;
    return GetArraySize(masks);
}
```

The retained `DATA::SetType` treats declarations `[0]` and `[1]` as scalars:
indexing them fails. An actual array resized down to one remains an array.
Refs to retained cells remain valid across resizing in Rust; removed cells and
objects expire and regrowing does not revive their old refs or arefs. The C++
implementation uses vector element addresses, which can dangle on reallocation;
Rust deliberately never reproduces undefined pointer access. An lvalue already
selected before its RHS removes that element remains a checked temporary cell
until the instruction finishes. Its write does not reinsert it into the array.

Array declarations are capped at 4,096 elements, with a shared 16,384 live array
cell budget across globals, local frames and retained arrays. There are at most
128 global declarations. Shrinking, failed allocations and unwound calls release
their charges. Existing string, object tree, call depth, operand and instruction
limits still apply; lvalue selections count toward each frame's operand limit.

## Executable diagnostic and retained reference

Run without game resources or a GPU:

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/global_ships.c \
  --frame-ms 0,0 --report global-script-state.json
```

This newly written fixture declares two Characters objects and arrays of masks,
node names and group names. Two immediate handlers store masks 548/274 and six
holes in persistent globals. A posted Commit handler reads those globals after
Main has returned. It fires on the second supplied frame and writes hd=548,
hd=274 and holes=6 to the native preview entity, in that order. Expected totals:
three completed handlers, one posted event and three entity notifications. It is
not an original game handler or a full character/battle system.

`reference/arrays/generate.py` extracts 13 unedited retained DATA constructors
and methods, including SetType, SetElementsNum, GetElementsNum, GetArrayElement,
GetVarPointer and scalar Set variants. C++20 runs them under UBSan with fail-fast
undefined-behavior checks. Only scalar storage/error/platform scaffolding is
substituted; no original interpreter or complete game program runs. Twelve
sequences produce 444 full snapshots of array/scalar status, size, values and
exact float bits. Rust consumes the committed snapshots on every host.

Source tests additionally cover persistent and isolated globals, forward global
resolution and shadowing, partial initializers, recursion, single-evaluation
indices, object copies, array/element refs, shrinking and expired aliases,
allocation bounds/error recovery and state shared by nested/posted events.

## Remaining scope

Dimensions must currently be integer literals; named define constants await the
preprocessor. Multidimensional arrays, local array initializer lists, whole-array
value loads/copies and boolean array initializer lists are explicit errors.
Boolean global scalars work; the retained compiler's erroneous scalar Set calls
for boolean array initializers are not emulated. Mismatched initializer types
fail explicitly rather than reproducing silent ignored initializers.
Reference-array container edge cases are not covered by the scalar C++ oracle;
the checked Rust element behavior is tested separately. Modules/includes/defines,
additional syntax, saved/cached programs, original native gameplay bindings and
unmodified original game handlers remain pending. This phase adds no game assets.
