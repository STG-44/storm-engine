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

`StrCut` uses inclusive byte offsets for the original character name lists.
Invalid ranges return an empty string. A range that splits a UTF-8
character also returns empty so the VM keeps valid text. `Pow` converts
both operands to floats before exponentiation, including the integer
precision argument in the original interface's `FloatToString`.

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
`GetNextLineString` returns CRLF. `SystemDelay` waits synchronously for
the specified real milliseconds, with the original one-millisecond minimum.

`ExecuteTechnique` supports the location ambient presets from
`src/techniques/location/old_ambient.fx`. The static `MODELR` shader
uses the preset RGB as its ambient contribution, including in the desktop
window and PNG output. Both `inca_temple` and the game loader's
`amb_inca_temple` spelling select the temple preset. An empty technique
leaves the current lighting state intact. Unsupported names warn and leave
it intact as well. General Direct3D effect execution remains pending.

`script_interface_functions` exposes the configured language and common
translations. It resolves the selected directory and strings file through
`language.ini`, reads the original common string records, and preserves
case-insensitive first-match lookup and spaces inside quoted text. The
`LanguageOpenFile`, `LanguageCloseFile` and `LanguageConvertString` bindings
also support original brace-delimited catalogs in UTF-8 and Windows-1251.
Repeated opens retain the same ID until the last close, and new files reuse
the lowest free ID. Language switching and interface-specific helpers
remain pending.

`script_rigging_files` exposes attribute templates with decimal numeric
formats for UI strings and the original sail power loss calculation with
the initial 1.0 hole dependence.
`ShipSailState` retains the startup fallback. Connecting it to live ship
characters, their sail hole masks and the configured hole dependence remains
part of the SHIP integration.

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
The VM also supplies `Sin`, `Cos`, `Acos`, integer and float `Sqrt`, and
`ARGB` color packing.

The `locator` class handles the island initialization message `LM_LOCATE_I`.
It reads GM model labels without loading textures and writes reload points,
their ship berths, quest and multiplayer spawns, fire positions and named
`LoadGroup` entries to the original script object. Other locator messages
remain pending. The configured game startup has a separate fuel allowance
for `Main`, since its ship, island and weather tables initialize in one call.

`Rand` and `frnd` consume one shared stream using the Windows CRT sequence
on every target. The runtime seeds it from Unix time in milliseconds as
the original compiler did. A fixed seed also reproduces Microsoft's
published sample. [Microsoft CRT rand](https://learn.microsoft.com/en-us/cpp/c-runtime-library/reference/rand?view=msvc-170)

`xinterface` initializes its viewport from `interfaces.ini` and the current
window size, loads the cursor texture, and reads the script's initial
`GameTime` attributes. Its synchronous initialization events obtain the
quest text files and the live script reference for the mouse wheel. The
`obj_strservice` messages and language functions share the open catalog
IDs and reference counts. The configured widget pass supports window
groups, static pictures and titles.

`ILogAndActions` sends the synchronous `SetWindowSize` event with the
current window dimensions during initialization. It reads the log and
action settings, maintains fading and pinned entries, and emits
`BI_FastCommand` through the original named control. `BITimer` reads its
layout from the script, handles start and cancel messages, and emits the
configured quest event during the Realize stage. The expiry handler can
restart the timer in the same frame. Battle HUD drawing remains pending.

The core `SetTimeScale` function and the battle log message of the same
name update the next frame's simulation delta. Posted events, native
services, and entities use that delta. The control tree advances on the
measured interval, including when the simulation is paused. The native
TIMER's fixed delta still overrides scaling for simulation steps.

`SetGlowParams` records the legacy renderer's settings and returns zero.
It reads arguments in native stack order and treats inactive numeric slots as zero.
The retained renderer disables regular postprocessing when it loads
`engine.ini`. Drawing the glow and underwater postprocessing passes remains
pending. Object copies reserve space only for the source subtree and reuse
the interned names in the current script world.

`Sound` registers on the native Realize layer and shares a stereo mixer
across script entities. The service handles WAV, Ogg Vorbis and MP3 assets,
sound aliases, ambient scheme scheduling, volume, and immediate or faded
stop. Resetting a scheme clears its schedule. Sounds already playing can
be stopped independently, including with the original master ID zero.
The desktop audio device opens on the first playback request. Linux
builds of the runtime require ALSA development headers (`libasound2-dev`
on Debian and Ubuntu). Positional sound remains pending.

`CAviPlayer` registers the original video layers and resolves clip names
under `RESOURCE/VIDEOS`. For a missing clip it posts `ievntEndVideo` with
the original one-millisecond delay. Its Execute fallback also signals
completion while simulation time is paused. The script handles cleanup
and schedules the next logo. A clip present on disk requires WMV playback,
which remains pending.

`InfoHandler` loads a script-selected backdrop and foreground using the
engine's TX texture reader. With a foreground, it uses the optional
`RESOURCE/Textures/Interfaces/int_border.tga.tx` overlay by default.
`picborderfilename` selects a different frame or suppresses it when empty.
The backdrop fills the window and the renderer
fits the centered foreground to its own aspect ratio. Realize layer
membership and message 1 control when the pictures are submitted. Both the
desktop window and `--headless` use the same compositor.

`infoStr` displays a centered caption, wraps long messages, and respects
the optional `scale` attribute. With `fontfilename` the script selects a
project-provided TrueType or OpenType font by game-relative path. The
default path is `RESOURCE/Fonts/Interface.ttf`. A missing font leaves the
configured pictures visible. Complex-script shaping remains pending.

`ReloadProgressStart`, `ReloadProgressUpdate`, and `ReloadProgressEnd`
are available through `scriptLocationLibrary`. They show, advance, and
clear a circular activity indicator over the last loading picture. The window
can present an update during synchronous script execution. Texture assets
are optional. Updates are limited to one every 50 milliseconds of
monotonic time, independently of the simulation speed.

`SetReloadProgressBackImage` selects the centered foreground for
`ReloadProgressStart`. The script can select a picture without creating an
`InfoHandler`. An empty path or a missing texture omits the foreground,
while a backdrop and caption from `InfoHandler` remain visible. Changes to
the selection leave the active loading screen intact, and
`ReloadProgressEnd` clears the selection.

`Fader` supports timed scene transitions through messages
`FADER_OUT` (33000) and `FADER_IN` (33001). Both use `lfl` for the
message ID, duration in seconds, and auto-delete flag. Fades follow the
simulation clock on the original Execute and Realize layers. The four
`FaderEvent_*` notifications use `li` for direction and entity ID and
dispatch through the native event queue. `FADER_PICTURE` (33003) and
`FADER_PICTURE0` (33004) take an `ls` message with a texture name and
select an aspect-fitted foreground and a full-screen backdrop, respectively.
Both selections also feed the next `ReloadProgressStart`. An empty name or
a missing texture clears its layer. A fade without a backdrop fills that
layer with black. The desktop and headless paths share the compositor.
`FADER_STARTFRAME` (33002, format `l`) requests a snapshot on the
transition's first Realize pass. Send it after starting a fade and before
that pass. The renderer captures before applying the fader's own overlay
and reuses the snapshot through the completion frame. PNG output evaluates
earlier capture frames on the GPU before reading back the final image.

A positive root `ProgressFrame` value in `engine.ini` enables the
`Fader` tips overlay and defaults to one. With it enabled, `FADER_PICTURE`
loads `interfaces\\int_border.tga` and selects it for the next
`ReloadProgressStart`. `FADER_PICTURE0` shares the pending overlay
selection. The compositor blends the overlay's alpha over the foreground.
Missing overlay assets leave the foreground visible. The original
`SetReloadNextTipsImage` call advances the shared random stream after an
enabled `Fader` initializes. The retained engine uses a single overlay
texture for rendering regardless of that call.

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

With the original `RESOURCE/Sounds/MUSIC/Military.ogg` available, check
playback through the default desktop audio device with:

~~~sh
STORM_AUDIO_GAME_DIR=/path/to/installed-game cargo test -p storm-runtime --locked \
  original_ogg_reaches_the_desktop_audio_device -- --ignored
~~~

For an external, pinned upstream checkout, configured startup uses:

~~~sh
cargo run -p storm-engine --locked -- --game-dir /path/to/sd-teho-public \
  --script-encoding utf8 --frame-ms 0,1,16,16,16,16,16,16,16,16,16,16
~~~

The pinned Russian checkout compiles all 201 startup source files and enters
`Main` in `seadogs.c`. The original `controls\init_pc.c` completes its first
control setup. With installed island locator models under `resource/models`,
`IslandsInit` reports 34 islands and `WeatherInit` reports 34 weather presets.
Startup completes the town and fast-reload tables and initializes
`xinterface`, `obj_strservice`, `ILogAndActions`, and `BITimer`.
`Main` returns successfully, and the first frame runs the weather update.
The next timed frame starts the logo sequence and resets `Sound`. With
the logo files absent, the script advances through AkellaLogo, Blackmark,
and Title. Before the picture compositor was added, startup stopped as it
attempted to create `InfoHandler` at `interface/interface.c:1737`.
The compositor is verified with independently generated picture fixtures.

With the installed interface and language data in the startup directory,
the original `Main_Start` now completes the post-logo control setup and
`InitGame`. It reports 1,001 locations, 498 items and 271 static NPCs,
then applies the controls and display settings and loads the selected
language. The configured runtime reserves 32 million instructions for the
top-level `frame` handler `Main_Start` and keeps the ordinary frame and
standalone limits. The configured attribute store also has room for the
full startup tables.

`GetEntity` and `DeleteEntitiesByType` use the native entity registry.
`DeleteEntities` synchronously destroys all scene entities, including
those already queued for deletion, before the calling script creates the
next scene. It preserves class registrations and layer settings. Rust
keeps allocation stamps monotonic to prevent stale script references
from resolving to replacement entities.
The startup particle entity supports the empty-system lifecycle and missing
profiles. World-space emission and the debug teleport UI remain pending.
The options reader loads the original dotted paths into script attributes.
An absent file preserves defaults and an empty file dispatches
`evntOptionsBreak` immediately. The display ramp applies to the engine's
window and rendered PNGs. Language changes reload common strings and retain
the IDs and reference counts of localized catalogs available in the new
language.

On frame 10, `LaunchMainMenu` loads the original `interface/mainmenu.c` and
enters `InitInterface`. It creates `scrshoter`, selects a weather preset and
creates `sea` and `ShipLights` inside `CreateSea` in `sea_ai/AISea.c`. The
pinned run also creates `Weather`, blends the fog and sun colors, and
completes `WhrCreateRainEnvironment`, `WhrCreateSunGlowEnvironment`,
`WhrCreateAstronomyEnvironment` and `WhrCreateSkyEnvironment`. The script
VM now supplies the original `Cos` builtin for the weather's particle wind.
Startup completes `CreateRiggingEnvironment`, creating `Sail`, `Rope`,
`Flag`, `Vant`, `VantL` and `VantZ` and assigning their layers. It also
loads the `MainMenu` coastal foam profile and schedules `CoastFoam`.
`CreateBackEnvironment` now sets the original background geometry, camera
and menu models through `InterfaceBackScene` before calling
`MainMenu_CreateShip`. With an ordinary weather preset it generates the
menu character, mounts the selected ship and completes the background
setup. The original `InitInterface` opens `mainmenu.ini` through
`MSG_INTERFACE_INIT` and creates the version string returned by the script.
The pinned startup rendered the menu with its logo, items and Cyrillic
version string on Metal at 1280 × 720. Add `--headless menu.png --width 1280
--height 720` to the startup command above to capture the final frame.
A randomly selected thunderstorm can stop earlier at the missing
`Lightning` class in `weather/whrlightning.c:34`. These probes use the
original source files as supplied.

A keyboard replay selected New Game and activated it with Space. The
original `InterfaceDoExit` destroyed the menu scene and recreated
`XINTERFACE`. The runtime then loaded the original `selectCharacter.c`
and entered its `InitInterface`. The compiler accepts the standalone
integer before a branch's `break` in that source, matching the retained
compiler and VM. Loading `selectCharacter.ini` passes the window,
background, title, exit button and both initial image collections.
It stops at the unported `FORMATEDTEXT,HERO_NAME`. A separate partial
layout capture uses the original `EI_CreateFrame` and `EI_CreateHLine`
script functions to render both the portrait frame and the setup frame.

`Sail` reads the original `[SAILS]` settings in `rigging.ini`. Its attachment
message groups the loaded MODELR hierarchy's GM sail locators, including
`sail0` depth limits, and creates a separate cloth draw beside the static
hull and spars. Execute advances the wind and rolling calculations for all
three sail sets. Realize uses the full or rolled grid and the original
common canvas. The speed and capacity messages support checked float
outputs. Removing an attachment or changing its model revision retires its
cloth. This path currently uses fixed yards, the near grid for deployed
cloth and the default canvas. Per-ship textures and emblems, damage,
detached sails and camera LOD remain pending in this path.

`Rope` reads the original `[ROPES]` settings and texture. Its attachment
message pairs the ship's `rope`, `fal`, and `stay` locators through the
MODELR hierarchy. It advances the five-sided rope geometry using the
original section counts, swing and sag, and applies the configured
view-distance test. Ends tagged with sail groups follow the cloth points
published by the attached and rolled sail calculations. Individual ropes
and whole groups can be removed without retiring the model or cloth.
The constraint that uses rope lengths to limit yard rotation remains pending.

`Flag` loads the original `[FLAGS]` atlas and attaches ordinary and special
flags to the GM locators of ship or town models. On the first scheduled
Execute, it requests each atlas cell from the original `GetRiggingData`
handler. The VM reads the character's attribute reference from the native
message. Single columns, column/row pairs and the hide response are
supported. Realize draws a separate model part using the existing
wind-driven flag mesh. The original flag-wave integrator, live atlas
replacement and transfer to detached mast models remain pending.

`Vant`, `VantL` and `VantZ` use separate INI sections, textures and GM
locator prefixes. Each group builds its tapered head, beam, textured
ratlines and five-sided vertical ropes in MODELR coordinates. The configured
renderer follows the model transform and applies the original distance
test. Shrouds can be removed by model group or by a checked mast-node name.
The loader accepts multi-digit locator numbers such as `vanx10` in the
Battleship model. Independent moving mast anchors and automatic detachment
from the native SHIP damage path remain pending.

`CoastFoam` loads the location's original INI and textures. It subdivides
authored endpoint pairs into eight-row strips and retains both the staggered
row motion and the one- or two-pulse coast animation. Visible strips sample
the current SEA surface and draw the original texture with its wave-height
offset and doubled alpha modulation. Distance and side-frustum culling run
against the configured render camera before wave sampling. Animation advances
in Realize even outside that view until the native camera is shared with the
simulation. The coast editor, open-water `Sea2_Foam` and `ShipTracks` wake
draws remain pending.

`InterfaceBackScene` loads the background GM, its locator companion and
the active and passive GM menu items with their original language texture
paths. Its authored camera sets the shared view and projection for the
background, sea, sky and coastal foam. The window uses the same projection
to test pointer rays against menu triangles. Script controls move selection
and post `backgroundcommand` with the selected item's event name. Ship
placement reads a locator's position and yaw when the locator exists.
Light sources, animated props and the optional static-model messages remain
pending. A separate capture exercises the original background and all six
menu items through the configured renderer.

The configured `SHIP` takes the character and real ship attributes from
`MSG_SHIP_CREATE`. Script callbacks set the original texture path and sea
layers around mounting. SHIP owns a MODELR and attaches sails, ropes, flags
and the three shroud classes when available. It uses the native default
stage priorities, mounts from the root GM size and publishes the combined
node box on the character. Execute reads the sail drive, uses the preceding
SEA wave state for the six-by-six hull samples, applies inertia and the
original waterline and immersion, and publishes post-move position and
speed. Realize draws the preceding simulation pose with its turn heel.
Destruction releases the model and rigging groups. The scope covers living
vessels. Collisions, sinking, ship-driven lights, damage, blots, tracks
and menu cannon effects remain pending. The Barkentine capture exercises
this configured path with original assets and wave frames.

`XINTERFACE` opens the configured dialog INI and resets the preceding
dialog's strings. The original `mainmenu.ini` has no active nodes in its
`[MAIN]` section because its interactive items are the GM models owned
by `InterfaceBackScene`. A missing INI posts the original `exitCancel`
event. Dynamic strings retain the script's `GameInterface.strings` values,
positions, alignment, visibility and color. `WINDOW` applies the native
visibility and activation messages to its groups. `PICTURE` draws a
texture or an entry from `pictures.ini` and accepts texture, position, UV
and tint changes. `TITLE` resolves the script's localized label and tiles
the original three-part atlas plate. `BUTTON` renders atlas faces, shadows
and pressed offsets. An eligible button receives pointer or focused
keyboard edges and emits the original `ISound`, `ievnt_command` and
configured events. Window locks and visibility are respected. A new
dialog drops queued actions from its predecessor. Held-input repeat and
double-click handling remain pending. `IMAGECOLLECTION` draws its authored
atlas pieces in order. The script can bind an atlas, append, recolor,
replace and clear pieces. The configured screen origin applies to
relative pieces, and the collection's offset applies to scripted appends.
Unsupported widgets and video or blinking pictures report explicit
errors. The renderer reads the
original bitmap fonts, including PC scaling, Cyrillic glyphs and the
native shadow. The desktop window, frame capture and PNG output use the
same overlay pass.

`ShipLights` loads the script-owned light catalog and corona textures on the
first attachment request. The native attachment API follows the original GM
`lights` and `flares` locators on the hull, masts and yards. CPU checks cover
transforms, brightness oscillators, the reset switch and point-light
selection. The extracted models contain 243 locators in those groups across
40 ship directories. Connecting the mounted `SHIP` owner to this service,
drawing coronas and point lights, and sun-road reflection remain pending.

`Weather` receives wind, fog, sunlight and time attributes, applies the
ambient color on `isDone`, and publishes sun and moon positions. Its native
Execute stage advances time and dispatches the floating-point
`WeatherTimeUpdate` event at the original cadence. The script VM implements
the signed integer masking and shifting used to blend packed colors.

`rain` generates the original four staggered blocks from the script
attributes, using the shared CRT random stream and the wind from `Weather`.
Each block drifts, recycles and fades on Realize, and the visibility
message pauses its animation. The renderer reuses the line geometry for
all four blocks, including the 50,000-drop storm presets, and blends it
additively against the scene depth in the window and PNG renderer. The
vertical band currently follows the fallback orbit camera. Collision and
sea splashes and rainbow still require their own passes.

`SunGlow` resolves the original sun, moon phase, halo, glare and lens flare
atlas textures at `isDone`. Execute updates their fade values and Realize uses
the direction supplied by `Weather`. The window and PNG renderer draw the
depth-tested sun or moon disc, clip the disc and halo to the sea horizon, and
blend the glare and lens flares additively. The flare and glare visibility
use the current camera and open-water fade values. Sail and terrain traces
and the sun-road reflection still need their original passes.

`Astronomy` reads the packed HIC catalog and spectral colors and loads the
original star and planet textures. It derives the vertex data from the
catalog instead of relying on the C++ structure layout in `star.dat`.
Realize updates the weather-hour fades, rebuilds the star brightness when
the fade bucket changes, and keeps the original fixed planet placements and
camera-relative sprites. The GPU scales the star point sprites by distance
and uses the camera field of view to interpolate the magnitude limit.
The extracted catalog contains 114,257 stars and renders in the isolated
window and PNG fixture. The Sky stage invokes its Realize step when the
active sky schedule blends two weather slots.

`Sky` reads the five original TX faces for each active entry of the script
sky schedule. Realize advances rotation and picks the adjacent sets using
the Weather hour. The renderer draws the blended background, composites
celestial sprites with the two textures' cloud alpha, and samples the same
transmittance for solar glare. The sky stage evaluates `WhrCalcFogColor`
at the 513 original hemisphere vertices after `isDone` or `TimeUpdate`.
The configured sea fades into the same sky and fog drawn above its horizon.
The renderer continues the sky's bottom texels and the fog's bottom ring
behind the water so a fixed haze color cannot form a stripe at the join.
The original environment-map reflection remains pending.

`scrshoter` captures the current scene at its Realize layer in the desktop
window and PNG renderer. The thumbnail uses the original 128 × 128 integer
sampling and the optional `EmptyBorder.tga` blend. The script can retrieve the
capture through `MSG_SCRSHOT_MAKE`; `makescrshot` fires after the image is
available, or after a failed attempt so the script can proceed. The thumbnail
reader accepts the original compressed save preview and releases cached images
on request or when the entity is destroyed. The PNG and window fixture checks
capture, preview reading and cache release independently from menu startup.

The Rust geometry reader can decode separately installed `MainMenu.gm`
(20 meshes) and `MainMenu_locators.gm` (nine labels). The extracted game
resources supply the 64 raw BGRA sea height frames to the configured `SEA`
entity. The demo and configured engine share the TGA decoder and retained
wave solver. Configured scripts control the two wave layers, maximum wave
height, query distance and Stop/Realize behavior. The runtime publishes the
surface to the same window, PNG and thumbnail renderer. It uses the existing
water shader, depth-tests against registered models and supports an isolated
sea draw before static models are available. The original view, reflection,
sun-road, foam and underwater passes remain pending, along with the
background scene and the menu interface services.
