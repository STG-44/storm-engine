# Native ship-type parameters and numeric mounting

`storm_sim::ship_parameters` reads all retained LoadShipParameters fields from
the ship-type ATTRIBUTES record: class, speed/turn rates, crew bounds, capacity,
weight, dependency coefficients, waterline and six inertia coefficients. Missing
or valueless fields default to zero. Named lookup is case-insensitive and literal;
DWORD results retain signed 32-bit field bits. Existing decimal-prefix parsers
are reused. Non-finite floats and stale/foreign bindings produce checked errors.

The original Y-acceleration/braking, X, Z lookup order is retained. Applying
loaded inertia preserves position, velocity, mass and unrelated motion state.
The separate numeric Mount subset derives width/length from root GM bounds,
sets nominal speed Z to SpeedRate and Y to TurnRate/444, computes Capacity/10000,
and initializes load zero, mass inertia 0.10, weight and gravity 9.81. The fixed
mass coefficient is retained; no cargo formula is invented. Configuration
updates represented inertia/mass and model waterline without resetting motion.
Other mounted values are returned separately, in native units.

Nominal type speed is independent from current character Ship.MaxSpeedZ/Y.
In parameter mode, character limits refresh each fixed frame through the native
lookup adapter. Publishing ship.speed/pos/ang does not overwrite those limits.
Type parameters latch at initialization/restart; limits and immersion stay live.

~~~sh
cargo run -p storm-demo --locked -- --particle-fleet-demo --native-ship-motion \
  --native-ship-rocking --native-ship-placement --native-ship-presentation \
  --native-ship-parameters \
  --ship-parameter-file rust/apps/storm-demo/fixtures/ship_parameters.json \
  --fleet-window
~~~

The viewer enables parameter loading automatically with its previous default
dynamics. Batch mode opts in with --native-ship-parameters. The example file
intentionally changes actual inertia and character limits; type SpeedRate 18
and character MaxSpeedZ 10 demonstrate their independence.

The file has optional ship_type and character objects, containing strings,
numbers or null. Ship-type keys are literal names; character keys are dotted
paths. File entries override diagnostic seeded attributes, including the
command-line WaterLine seed. Character Ship.Immersion remains separate and may
also be supplied explicitly. One file applies to both ships; their handles,
motion and later character updates remain independent. R reloads on restart.

Input is bounded to 64 KiB, two sections, 128 entries per section and 4096 bytes
per value. Invalid shapes/names fail before writes. Subsequent allocation/numeric
errors may retain import writes and stop startup. Unknown attributes are retained
but ignored by these named readers; null represents a valueless attribute.

These are chosen/external records, not calibrated commercial ship parameters.
Original PROGRAM definitions are not included. Full Mount geometry/services,
lights, cargo, force providers, saves and gameplay remain pending. The sea is
still diagnostic; no sail/wind/cargo force script is added here.

The oracle compiles unedited LoadShipParameters, the numeric Mount slice, full
ship_t/SHIP_STATE declarations and CVECTOR under UBSan with FP contraction off.
Sixty-four distinct profiles compare 192 complete represented snapshots, including
missing fields, reload, signed DWORD bits, raw coefficients, constants and large
capacity rounding. Values/construction/box info/output are adapters; native
string parsing is covered separately by existing attribute references. Further
tests cover defaults, live/latched lifetimes, invalid numeric setup, file bounds,
unchanged default dynamics and actual body changes from external attributes.
This is standalone scenario twenty-three.


## Four-platform evidence, 2026-10-26

Code `53a83b2c7ab852470b65d0706866a9b7f8e3166b` passes the
[PR run](https://github.com/STG-44/storm-engine/actions/runs/40381752291) on macOS
Apple Silicon/Intel, Windows and Linux: formatting, warnings-denied Clippy,
builds, 342 CPU tests (135 interpreter tests) and twenty-three standalone
scenarios. Logs verify actual host triples and test counts. Linux also passes
seventeen explicit software Vulkan render tests and three frames of the fleet
window with parameter loading enabled.

Four ZIP digests / 92 complete JSON reports match local output. The preceding
22 batch reports remain unchanged. The new native parameter oracle covers 64
distinct profiles / 192 complete represented snapshots under UBSan.

Local Metal passes seventeen explicit render tests. Four generated/original
captures at 1.5/3 seconds exercise external type and character records; original
frames were inspected. The native Mac window initializes the GPU and scene, but
its three-frame check did not finish before termination; current physical window
frame presentation and manual interaction remain unverified.

All forty original model roots use the external parameter fixture, with root
dimensions, type values and independent live character limits checked. Actual
motion changes from the preceding presentation fixture. First-owner removal
freezes its state while the second continues. Original fire/mast/hull/crew and
waypoint metadata remains unchanged. Across 274 locators the surviving owner
retains 548 systems / 822 finite world rows / 12,243 living particles at three
seconds; the model without fire points also moves. Commercial resources,
captures and audit reports remain outside Git.

The fixture is explicitly diagnostic/external. Original PROGRAM ship definitions,
full Mount/Execute services, cargo/force providers, calibrated sailing and the
original SEA remain pending. Hosted Windows/macOS GPU/window coverage also
remains open. Earlier whole-profile/manual-damage audits were not repeated.
