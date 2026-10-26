# Native sail drive and sail-set queries

`storm_sim::sail_drive` ports SAILONE's capacity, damage and wind arithmetic,
the represented group normalization and the MSG_SAIL_GET_SPEED case. Triangle,
trapezoid and default coefficient families retain their original expressions.
MulToInvNorm uses matrix columns without normalization or translation. Capacity
uses original local hard points, including the third/fourth quad locator swap.
Holes reduce maximum contribution before directional and rolling factors.
Rolled sails still contribute to the damaged maximum, while current drive is zero.

The wind-strength multiplication is commented out in the retained method and
remains absent. Weather/game scripts must supply current Ship.MaxSpeedZ/Y;
the returned sail coefficient multiplies that speed through retained movement.
Default coefficient families come from the native SAIL constructor. Original
INI overrides and original weather/script providers are not loaded here.

The speed query writes the current group's speed before selecting the requested
sail set. Requests below 0.3 furl, above 0.7 select full, and the inclusive middle
band selects normal. Minimum-speed behavior uses the previous set. Missing groups
use the strict request > 0.3 fallback; deleted groups return zero; a null output
does nothing. Duplicate IDs select the first group. SetRolling calls retain sail
order, and fSpeedMul retains the original formula; it is not an extra multiplier
in CalculateSailWind. Non-finite inputs, overflow and oversized collections fail
explicitly in the checked Rust API. Raw script sail commands remain separate.

The fleet adapter feeds actual strength[0].speed.z. Effect-locator frames also
use pinned scalar trig, scoped to this mode; legacy diagnostics retain their
previous math. This frame is a viewer adapter, not native CMatrix construction.
Static node axes enter the
native dot products after inverting the viewer pose with pinned scalar trig.
Each fixed frame evaluates sails, runs the query, settles requested geometry,
then advances the ship. A changed request uses the preceding set's drive for
that query. This explicit order does not establish complete native entity
scheduling or Mount/Execute order.

SetRolling, DoRollingStep, forced yard turning, cloth geometry, sound and weather
services are not ported here. The geometry adapter switches immediately to a
settled sail set. W/S changes the first ship's request by 0.5; A/D controls its
rudder, with right taking precedence when both are held, as in SHIP. The second
ship stays independent. Restart reloads the initial set. The viewer enables
native drive automatically; batch scenarios opt in.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters --native-sail-drive --fleet-sail-setting 0.5 \
  --fleet-wind-direction 135 --time 3 --particle-fleet-report sail-drive-state.json
~~~

This wind direction also controls rendered cloth; its visual magnitude keeps the
existing default. Sail masks refresh from the model state. Fallen-mast sails are
represented by zero effective contribution with reference capacity retained;
native rehosting/group-capacity rebuilding remains pending. First-owner release
marks its group deleted and freezes further evaluation. A model without sails
has zero sail drive; rowing and other propulsion remain pending. Original resources,
captures and audit records remain outside Git.

The reference compiles unedited CalculateSailWind, GetSailSpeed, MulToInvNorm,
capacity expressions, group normalization and the speed case under UBSan with
FP contraction disabled. Ninety-six profiles compare 288 complete sail outputs
and 384 complete represented group/query snapshots as float bits. Matrix storage,
transport, construction and SetRolling call observation are adapters. Original
rolling/turning, renderer execution, full entity transport and native group
registration order are excluded. Tests also cover independent groups, null/error
boundaries, local capacity/node axes, actual motion with different wind/sets/holes,
one-query selection delay and owner release.


## Four-platform evidence, 2026-10-26

Code `9d573fdf20844382a3682c9d24d99500ed59ea18` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/40387887268) on macOS
Apple Silicon/Intel, Windows and Linux: formatting, warnings-denied Clippy,
native builds, 348 CPU tests (135 interpreter tests) and 24 standalone scenarios.
Host triples/counts are checked from logs. Apple Silicon passes a single-job
retry after GitHub could not acquire its initial hosted runner; the other three
successful jobs are retained. Four ZIP digests / 96 complete JSON
reports match local output. The preceding 23 reports remain unchanged.

Linux passes seventeen explicit software Vulkan render tests and three native
sail-drive fleet-window frames. Local Metal passes seventeen render tests and
six generated/original captures, including all three settled sail sets on the
original ship; those three frames were inspected. A local native-window check
was not repeated; the preceding Mac three-frame check remained unconfirmed.
Hosted Windows/macOS physical GPU and window checks remain open.

All forty original roots and 368 sails pass the drive audit: original metadata,
type/character values and independent owner lifetimes are retained. Native
formulas change actual motion. Boat has zero sail drive, with rowing pending.
Across 274 locators the surviving owner retains 548 systems / 822 finite world
rows / 12,243 living particles at three seconds. The 96 native profiles cover
288 sail outputs and 384 represented group/query snapshots under UBSan.

Static node frames, immediate settled geometry, diagnostic weather and explicit
stage ordering remain adapters. Original rolling/turning, group rehosting,
weather/scripts and full Mount/Execute/gameplay integration remain pending.
Commercial resources, captures and evidence stay outside Git; earlier manual
damage and whole-profile audits were not repeated for this increment.
