# First executable Storm source subset

`storm-script` compiles a bounded subset of the retained Storm source language
to private stack instructions and executes it natively. It has no graphics
dependency and runs without commercial resources. This is the first executable
slice of the script engine, not a complete replacement for the original VM.

Run the new diagnostic fixture from the repository root:

```sh
cargo run -p storm-script --locked -- \
  rust/crates/storm-script/fixtures/sail_damage.c --report script-state.json
```

The script calls another script function to count set bits in sail masks `548`
and `274`, checks a floating-point lost-area calculation, then calls explicit
native bindings to write `ship.sails.rey_a1.1.hd`, its hole count and
`ship.sails.rey_b1.2.hd`. A native read checks the first mask. The result is
integer `6`, with three attribute writes in an ordered JSON tree. Both masks
have three holes. The fixture is newly written diagnostic code; it is not an
original game handler or a physics simulation.

The tree is the already ported `storm-core::AttributeStore`, including its
case-insensitive path lookup and first spelling. `PreviewSetAttribute` and
`PreviewGetAttribute` are deliberately named diagnostic bindings. They are not
original native function names, entity notifications, or savegame persistence.
The separate `storm-demo` sail observer continues to report `script_vm: false`:
these scripts do not yet consume its damage events or drive the rendered ship.

## Source and execution rules

The implementation follows `src/libs/core/src/token.cpp`, `expression.cpp`,
`compiler.cpp` and `data.cpp`, rather than treating the language as C:

- Functions with `int`/`bool`, `float`, `string` and `void` declarations, named
  parameters, forward calls, recursion and strict return types. Names are
  ASCII case-insensitive. Native declarations dispatch only through `Host`.
- Function-scope locals, comma declarations, optional initial assignment,
  scalar assignment, `+=`, `-=`, `*=`, `/=`, integer statement `++` and `--`.
  Scalar assignment and argument copy can change the declared scalar type,
  matching `DATA::Set/Copy` and the disabled argument type check in C++.
- Blocks, `if`/`else`, `while`, `break`, `continue`, `return` and call statements.
- Decimal integer/float literals, raw quoted UTF-8 strings, line/block comments,
  arithmetic, comparisons and short-circuit logical operators. Strings do not
  interpret C escapes. Floats use IEEE binary32 throughout arithmetic.
- `&&` and `||` share a precedence level and associate left to right. All six
  comparisons also share a level. Unary signs bind before power. Chained powers
  require explicit grouping. Grouped expressions start at the original L1
  comparison level: a logical expression nested inside another pair of
  expression parentheses is currently rejected, as in the retained compiler.
- Conditional jumps convert to integer, truncating floats; `BoolConvert` tests
  the full value. This preserves surprising original cases such as
  `0.5 && 0` yielding `1` after taking the short-circuit path. `!0.5` yields `1`.
- Integer division truncates towards zero. A float left operand with `%` yields
  `floor(left / right)` as a float. An integer `%` float floors the divisor before
  integer remainder. Negative powers preserve the original loop, including
  exponent `-1` leaving the base unchanged and `2.0 ^ -3` yielding `0.5`.
- String/string comparisons fold ASCII to uppercase. Integer/string addition
  concatenates decimal spelling. String-left comparisons with integers use
  `from_chars`-style decimal prefixes: no initial space or `+`, invalid or
  overflowing input becomes zero. The original `DATA::Neg` string bug is
  preserved: its numeric conversion overwrites the intended logical result,
  so `!"12tail"` yields `12` and `!""` yields `0`.

## Bounds and deliberate differences

Source is limited to 256 KiB, 16,384 significant tokens, 128 functions,
128 locals/parameters per function, 64 levels of compiler nesting and 4 KiB
per string. Default execution allows 100,000 instruction steps, 32 call frames
and 1,024 operands per frame. Call/operand limits can be reduced but not raised
above those bounds; `--instructions` changes the step budget. Each multiply or
divide in the original power loop also spends a step. The preview host limits
its attribute tree and writes. Native implementations remain responsible for
their own work and allocations.

Diagnostics identify source line/column and the calling function. Errors abort
the current call and release its stacks. Native side effects already performed
are not rolled back. Unlike the C++ VM's inconsistent error continuation, bad
operations fail explicitly. Division by zero, integer `MIN / -1`, and non-finite
or out-of-range float-to-int casts fail rather than invoking C++ undefined
behavior. Integer add/subtract/multiply/negation explicitly wrap in 32 bits;
the C++ reference excludes signed-overflow inputs. Integer literal bits accept
the unsigned 32-bit range and are interpreted as signed values.

The compiler rejects unsupported grammar. Some type combinations can only be
detected during execution and produce a located error. Unknown callees and
argument counts are checked after compiling the function table. An embedding
host must register behavior for each native declaration; there is no implicit
FFI, process execution, file access or network API in the language runtime.

## Independent reference and remaining scope

`rust/crates/storm-script/reference/generate.py` extracts the retained
`DATA::Set`, `Convert`, arithmetic, comparison and boolean method bodies
verbatim. It compiles them with storage/platform scaffolding, the retained
string comparison header, and UndefinedBehaviorSanitizer. `fmt` and
`fast_float` paths outside the tested subset throw. The committed golden cases
are consumed by Rust tests on every platform; C++ is needed only to regenerate
them. This checks scalar results and exact float bits, not full original
compiler/program execution. Source-level tests separately exercise precedence,
short-circuit effects, calls/returns, loop control, native attributes and bounds.

Still pending: globals, arrays, `object`/`ref`/`aref`/`ptr`, original attribute
expression syntax, original native functions, includes/defines, segment loading,
`for`, `switch`, `goto`, dynamic calls, events/delays, debugger, saved or cached
programs and the full game scripts. String/float mixed conversions and
concatenation, string-left float comparison and legacy non-ASCII comparison
rules also remain unsupported. Private Rust instructions are not a reader for
the original compiled-program cache or a stable bytecode file format.

The next integration step is object/attribute expressions and original native
bindings, followed by original script event handlers. Full game startup and
gameplay remain separate work.
