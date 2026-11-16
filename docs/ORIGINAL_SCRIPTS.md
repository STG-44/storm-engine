# Original game scripts and current compatibility boundary

Game scripts retain the Storm language and original source. The Rust engine,
compiler, runtime and native API are being ported; game programs are compatibility
inputs. The pinned Russian startup program compiles and registers its
scripted events. Native game startup remains in progress.

## Upstream corpus

The primary source is [storm-devs/sd-teho-public](https://github.com/storm-devs/sd-teho-public),
published by the Storm team under GPL-3.0. Its README provides localized game
scripts/configuration and describes combining them with the separately installed
game assets. The current local compatibility checkout pins:

- Russian: 09f89728124f7181648fc39576e4e5b7fbfc6972.
- English: d897194748fb134b855fb5f85bd3b7333cf00e69.

The pinned Russian program contains 962 UTF-8 files, totaling 22,946,923 bytes,
including non-source text. Its version header identifies 1.7.3 (15.04.21) x64.
Compatibility with the separately extracted asset version remains unverified.
The corpus is kept separately from this engine repository. No commercial game
assets or programs are added to the engine fixtures.

The installed copy checked on Mac lacks PROGRAM; the user confirms the same
after a Windows installation. An installation search is no longer needed for
the script corpus. The public upstream checkout supplies the original inputs.

## Supported additions

The private VM supports switch selectors with single-token literal/define cases,
explicit final breaks and a trailing default label. It evaluates the selector
once, compares with the case value on the left as native STACK_COMPARE does,
and retains it across comparisons. Nested breaks and loop continues release
only exited switch values; early returns retain strict operand-balance checks.
Missing breaks, chained cases, non-trailing defaults and other unported label /
goto forms return checked errors. This is a bounded source-derived contract,
not a complete port of native switch compilation or C fallthrough semantics.

The lexer retains signed-char TOKEN::StartArgument boundary skipping, including
nonbreaking spaces present in unchanged seadogs.c. Quoted Unicode and control
bytes remain intact. A separate oracle compiles the complete unedited TOKEN
implementation under UBSan with signed char and compares 28 lexical streams.
Only platform-header and output scaffolding is supplied. This validates spacing
boundaries, not the complete original compiler.

The original spelling #libriary records named native library requests in append
order, retaining the first case-insensitive occurrence and source location.
Program and Runtime expose these requests. NativeHost::register_library binds
explicit Rust initializers, which can register actual native functions through
the existing API. Initializers run once per host before fresh script state and
Main; missing libraries and recursive initialization return checked errors.
Failed Runtime initialization does not repeat native side effects on later
calls or engine shutdown. Entity cleanup still runs after a startup fault.

This is an explicit registration adapter. Native C++ initializes libraries while
compiling and warns on missing classes; the current Rust pure-compilation path
records requests and resolves them before state initialization, with checked
errors on missing bindings. It does not load arbitrary DLLs or implement the
production renderer, Steam or general script-library function sets. Complete
native compile/load timing and dynamic segment behavior remain pending.

## Loading the original program

The configured engine bundles the 13 original C++ shared script headers and
resolves them through the reserved `storm-engine/` prefix. Native TOKEN treats
`#ifndef` and `#endif` as comments. Empty defines reserve names, and defines named
`true` or `false` coexist with the intrinsic boolean tokens.

The configured engine uses a compilation profile with limits of 2 MiB per
file, 32 MiB total source, 2,048 files and 2,097,152 tokens. It also allows
larger function, global, constant and local tables. The game profile permits
1,024 event names and 4,096 handler pairs across static registration and
runtime events. Standalone compiler APIs use the smaller profile by default
and can receive an explicit profile. VM execution limits are configured separately.

An `extern` declaration can resolve to a subsequent function definition in the
same compilation, including definitions in appended include files. Calls to
unloaded declarations report the function name when executed. Dynamic segment
loading is future work. A scalar global keeps its default value when initialized
with a recognized define or a literal of another type, following the original
compiler's initializer branches. Calls at the top level are checked at compile
time. Startup enters `Main` at its own offset. `GetEngineVersion()` returns the
retained script compatibility version, 54128. The compiler accepts separator
tokens between a function signature and its body, as used by
`GetColonyByIndex` in the original colony scripts.

Counted `for` loops use a postfix increment or decrement of the named
variable, following the original compiler. A `continue` runs the update
before reevaluating the condition. A `break` exits the loop immediately.

Attributes can use digit-leading names such as `.1`, `.01` and `.1x0`.
Leading zeros are part of the key, and alphabetic characters use the same
case folding as other attribute names. The compiler accepts plain assignments,
local declarations and direct calls before clear statement boundaries inside blocks, as
well as surplus closing parentheses in the assignment, return and `if` forms
used by the original scripts.

`call variable(arguments)` supports both standalone calls and expressions.
The VM evaluates the arguments first, reads the variable, and looks up the
resulting name case-insensitively. Dynamic and direct calls share the same
invocation and reference handling. Dynamic call expressions require a
declared string variable, as in the original compiler.

A standalone `else` tests the shared expression register used by the original
Storm VM. The interpreter restores a normal `if` condition after the body,
including when nested calls change the register. A loop test, switch comparison,
short-circuit test, value return, dynamic call, or attribute operation can
change the value seen by the next standalone `else`. The compiler accepts
the statement boundaries used by the boarding, cabin, island, sail
interface and world map scripts. These include an assignment followed by
a direct call or another scalar assignment, a colon or surplus closing
parenthesis after a direct call, and a value return immediately before
the block ends.

When a standalone direct call is followed by `=`, the original compiler
emits the call, discards its result and emits a MOVE to the current frame's
left operand. The Rust VM retains the destination of variable writes,
increments and attribute writes for this form. A missing destination reports
a source-located runtime error. A switch branch may end with `break`
immediately before the next label or the closing brace.

Function locals are registered before body compilation, including declarations
inside later branches. Each invocation starts with fresh local storage and
executes initializers when it reaches their statements. Switch tables use the
shared source and VM instruction budgets. This covers the original table of
464 character models and explicit breaks after the last labeled branch.
The compiler also recognizes assignments to indexed objects and attribute paths
after an omitted separator, and a comma between the condition and update in
the original island loader's counted loop.

An unbraced control body ends at the separator read by that body. A
semicolon consumed by a nested control body leaves the enclosing one open.
The island loader uses this behavior after a `continue` with an omitted
separator. The next nested conditional and the following assignment
remain inside the outer body. An `else if` ends after its first `if`,
following the original compiler's separate bound for that form.

A value return followed by `=` exits before the following assignment,
as in the original sail-power calculation. Reference binding commands can
hand the next declaration back to the body parser. The parser retains an
attribute target across extra closing parentheses and recognizes a
`break` immediately following a value return in a switch branch.

The compiler recognizes the 91 entries in the retained internal function
table. Implemented builtins use the VM's existing handlers. Other entries
resolve through registered native host functions and report missing bindings
when called. Standalone calls check their argument count during compilation.
Expression calls to defined script functions with parameters defer the arity
check to frame entry, matching the source acceptance of the original compiler.

Configured startup connects `script_libriary_test` to the original range,
clamp and degree-conversion operations. The portable performance counter
uses monotonic nanoseconds and retains the original low-32-bit wrapping
subtraction for elapsed ticks.

`dx9render_script_libriary` now binds `SetTexturePath`, `GetTexture` and
`ReleaseTexture` through the runtime host. Four path levels are tried from
highest to lowest before the base request. The resolver adds the original
texture root and implicit TX suffix, preserves case-insensitive request
identity and uses the original first-free slot and reference counts.
Native `MODELR` loads use the same path levels in the wgpu model pipeline.
Script texture IDs currently retain decoded 2D TX pixels. The UI texture
consumer, raw image fallback, glow and text drawing remain to be integrated.

`SteamApiScriptLib` uses the retained engine's Steam-disabled response profile.
The twelve bindings report the original offline values for achievements,
statistics, overlay and DLC queries.

`ScriptLocationLibrary` exposes the character and location ID lookups. The
two searches keep separate native-sized caches and check each cached index
against current array contents. `NativeFindLoadCharacter` reads the current
valid location entity's `index` attribute in storage order, and
`GetNextLineString` returns CRLF. Progress-screen rendering, technique
execution, and the delay service remain pending.

`script_interface_functions` exposes the configured language and common
translations. It resolves the selected directory and strings file through
`language.ini`, reads the original common string records, and preserves
case-insensitive first-match lookup and spaces inside quoted text. Text
files support UTF-8 and Windows-1251. User string files, language switching,
and interface-specific control helpers remain pending.

`script_rigging_files` exposes attribute templates with decimal numeric
formats for UI strings and the original sail power loss calculation with
the initial 1.0 hole dependence.
`ShipSailState` returns the original integer zero before a native SAIL entity
exists. Live SAIL state, configured dependence and script-driven changes to
sail hole masks still need the native SAIL service.

`LoadSegment` links source from the configured program directory into the
running script. A segment can use existing globals, defines and functions,
and supply definitions for outstanding `extern` declarations. Failed loads
return zero and leave the linked script symbols and handlers intact.
`UnloadSegment` invalidates functions and globals and removes their event
handlers immediately. `SegmentIsLoaded` continues to recognize the segment
until frame cleanup, after which it can be loaded again. Function names
and global slot indices stay reserved for code that already linked to them.

The PC controls service creates named bindings, accepts the original key
codes and flags, and runs timed control trees. It sends the original control
activation, deactivation and wheel events to scripts. The desktop window
feeds keyboard and mouse input to the same service used by the headless
tests. Text input and camera consumption of raw mouse axes remain pending.
The VM also supplies `Sin`, integer and float `Sqrt`, and `ARGB` color packing.

The `locator` class handles the island initialization message `LM_LOCATE_I`.
It reads GM model labels without loading textures and writes reload points,
their ship berths, quest and multiplayer spawns, fire positions and named
`LoadGroup` entries to the original script object. Other locator messages
remain pending. The configured game startup has a separate fuel allowance
for `Main`, since its ship, island and weather tables initialize in one call.

The segment-loading revision passed 478 CPU tests on each of four CI hosts.
Linux also passed 26 software Vulkan tests and the native-window check.
All four archive digests, 144 scenario reports and four startup traces
matched the baseline.
[GitHub Actions run 43761108599](https://github.com/STG-44/storm-engine/actions/runs/43761108599)

The common-translations revision passed 474 CPU tests on Windows, Linux,
and both macOS architectures. Linux passed another 26 software Vulkan
tests and the native-window check. All four archive digests, 144 scenario
reports and four startup traces were verified against the baseline.
[GitHub Actions run 43751892089](https://github.com/STG-44/storm-engine/actions/runs/43751892089)

The local-registration revision passed 462 CPU tests on each of four CI
hosts. Linux passed 26 software Vulkan tests and the native-window check.
All 144 scenario reports, four archive digests and four startup traces were
verified against the baseline.
[GitHub Actions run 43721815161](https://github.com/STG-44/storm-engine/actions/runs/43721815161)

The dynamic-call revision passed 455 CPU tests on each CI host. Linux passed
26 software Vulkan tests and the native-window fixture. All four archive
digests, 144 scenario reports and four startup traces were verified.
[GitHub Actions run 43691189014](https://github.com/STG-44/storm-engine/actions/runs/43691189014)

The digit-leading attribute revision passed 454 CPU tests on each CI host,
with 26 software Vulkan tests and a native-window check on Linux. All 144
archived scenario reports and four startup traces match the baseline.
[GitHub Actions run 43690632825](https://github.com/STG-44/storm-engine/actions/runs/43690632825)

The shared-header, `extern` and counted-loop revision passed 450 CPU tests on
each of the four CI hosts. Linux also passed 26 software Vulkan tests and the
native-window fixture. All four archive digests, 144 scenario reports and
startup traces were verified against the baseline.
[GitHub Actions run 43689748635](https://github.com/STG-44/storm-engine/actions/runs/43689748635)

The preceding switch, lexer and library registration changes passed 439 CPU
tests on each of the four CI hosts, plus 26 Vulkan tests on Linux. The archived
reports match the 36 baseline scenarios on all four hosts.
[GitHub Actions run 42077673030](https://github.com/STG-44/storm-engine/actions/runs/42077673030)

## Reproduce

From the engine repository root:

~~~sh
cargo test -p storm-script -p storm-runtime --locked
python rust/crates/storm-script/reference/token_spacing/generate.py
~~~

For an external, pinned upstream checkout, configured startup uses:

~~~sh
cargo run -p storm-engine --locked -- --game-dir /path/to/sd-teho-public \
  --script-encoding utf8 --frame-ms 0
~~~

The pinned Russian checkout compiles all 201 startup source files and enters
`Main` in `seadogs.c`. The original `controls\init_pc.c` completes its first
control setup. With installed island locator models under `resource/models`,
`IslandsInit` reports 34 islands and `WeatherInit` reports 34 weather presets.
Startup then reaches the `Rand` call in `InitTownTable` at
`battle_interface/fast_reload_table.c:987`. The script RNG is still pending.
