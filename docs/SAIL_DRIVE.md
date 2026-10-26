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

The fleet adapter feeds actual strength[0].speed.z. Static node axes enter the
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
