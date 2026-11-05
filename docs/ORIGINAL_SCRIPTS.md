# Original game scripts and current compatibility boundary

Game scripts retain the Storm language and original source. The Rust engine,
compiler, runtime and native API are being ported; game programs are compatibility
inputs. Full original-game compilation and startup are not established yet.

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
implementation under UBSan with signed char and compares 26 lexical streams.
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

## Actual startup boundary and next work

The unchanged upstream root source now passes the earlier unsupported colon
and nonbreaking-space lexical boundaries and library-directive collection.
Configured engine startup currently stops at its reserved
storm-engine/layers.h include. The original C++ LoadFile resolves that prefix
from engine resource/shared, separately from the game program directory.
That shared-header provider and its native header-guard/empty-define behavior
are the next source-loading work.

Existing source limits remain 64 files / 1 MiB total / 256 KiB per file and
16,384 tokens, with bounded function/global/local counts. They are insufficient
for the whole corpus. Explicit bounded game-sized compilation, extern/linking,
remaining grammar, native functions/classes and services must follow. Raising
limits alone does not establish compatibility. This work extends the same
shared Engine/World used by the desktop model window, alongside native sea /
camera integration; it does not claim an original-game or playable land start.

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

The current missing shared-header error is an expected, explicit compatibility
boundary. It is not a successful game startup.
