# Multi-file script source loading

`Program::compile_with_loader` and `Runtime::compile_with_loader` load source
through an explicitly supplied SourceLoader. `DirectorySource` reads UTF-8 files
inside one selected program directory. Ordinary `compile(text)` supports typed
constants in memory and reports an error for includes without a loader. Runtime
execution itself has no implicit filesystem or native library access.

Run the new diagnostic without game assets or a GPU:

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/multi_ship/Main.c \
  --frame-ms 0,0 --report multi-script-state.json
```

By default, the entry file's directory is the program root. To use an explicit
root, supply a root-relative entry name:

```sh
cargo run -p storm-script --locked -- \
  --script-root /path/to/PROGRAM init.c --report script-state.json
```

## Include order and file names

The retained compiler appends included files to the end of the program, in
encounter order. With root including A then B, and A including C, registration
order is root, A, B, C. Includes are resolved from the program root, including
includes written inside a subdirectory's file. They do not use that file's
parent directory. Function calls can refer forward across the complete file set.

Names normalize ASCII case, Windows separators and dot components. Duplicate
and cyclic references to the same normalized virtual path are loaded once;
the entry file participates in this set. The directory loader detects ambiguous
case variants on filesystems where both can exist. Canonical resolved paths
must stay inside the selected root, including symlinks. Absolute paths, parent
components, empty components, colon/control characters and non-ASCII virtual
names fail explicitly. The physical root itself can have a non-ASCII name.
Normalization of slash/dot aliases is a portability adaptation beyond the
retained string list's case folding.

The loader accepts regular UTF-8 files, with 256 KiB per-file and 1 MiB total
source budgets, at most 64 unique files and 16,384 significant tokens across
all files. Source files are lexed independently; partial strings/comments cannot
span file boundaries. There is no implicit shared-engine-directory remapping.
Custom loaders can explicitly provide additional virtual sources. Compile and
runtime diagnostics carry normalized file names with source line/column.

## Typed constants

Examples of supported declarations:

```c
#define SHIP_COUNT 2
#define NEGATIVE_LIMIT -7;
#define DAMAGE_SCALE 1.0
#define DAMAGE_EVENT "Damage"
object Characters[SHIP_COUNT];
```

Defines store a single integer, float or string literal. Names are ASCII
case-insensitive and redefinition fails. An optional semicolon is accepted.
Expression reads first resolve local/global variables and then constant names;
strings and literal attribute names keep their contents. Constants are not
textual replacement rules. Functions can read constants declared later in the
file set because their bodies compile after registration.

Array dimensions are different: the integer define must already be registered
before the global/local declaration. The retained compiler checks this while
registering declarations. Consequently, a global array in the entry file cannot
get its size from a define in an include appended later. The dimension may be
an integer literal or an earlier integer define, with the existing 4,096-element
cap. Runtime size expressions remain unsupported. Global initializers still
require typed literals; define names in those initializers fail explicitly.

There are at most 1,024 constants. Function-like macros, aliases to other
constants, constant expressions in define declarations, boolean defines,
function-scope defines and conditional directives remain unsupported. Dynamic
segment loading/unloading, native library directives, legacy source encodings,
additional language syntax and saved/cached programs remain separate work.

## Diagnostic and independent checks

The new four-file fixture includes mixed-case Windows paths, duplicate/cyclic
includes, dimensions from an earlier define and string constants for event
names. Its load order is main.c, data/characters.c, handlers/sails.c,
helpers/bits.c. Two handlers persist the original masks 548/274 in global
Characters objects. After Main returns, a posted handler commits them and six
holes to the visible native ship on the second supplied frame.

The result, complete attribute tree, three ordered entity notifications and
event counters match the preceding single-file global-character fixture. The
CLI adds only `script_sources` for multi-file inputs, leaving single-file report
shapes unchanged. This is newly written diagnostic source; it is not an original
commercial game program. Commercial program sources are not included in this repository; complete
original game startup remains pending.

`reference/sources/generate.py` copies six retained TOKEN, STRINGS_LIST and
S_DEFTAB source/header files unedited. It extracts the compiler's AppendProgram
method and include/define case bodies verbatim. A small registration scan and
virtual file provider supply platform/debug/cache scaffolding. C++20 executes
21 file graphs under fail-fast UndefinedBehaviorSanitizer: 17 accepted graphs
and four duplicate-define failures. Rust compares normalized file order and
numeric constant names/values/float bits; rejected graphs fail compilation.

The reference covers source registration with numeric defines, not the complete
C++ compiler/interpreter. String constants have separate source tests; they are
excluded from this C++ harness to avoid the retained table's mismatched string
deallocation. Golden data records core source and extracted-body hashes. Source
tests also cover constant shadowing, array-dimension chronology, errors and file
locations, path/case rules, include cycles, aggregate bounds and native queued
entity writes. C++ is only needed to regenerate golden data.
