# Scalar SEA weather attributes

This increment connects the retained [scalar surface](SEA_SURFACE.md) to the
numeric branches of original `SEA::AttributeChanged`, using existing attribute
trees, entity notifications and the bounded Storm source runtime. It does not
port the complete weather service, original weather programs or full SEA entity.

## Original ordering

- `Sea2.Amp1/2` change pending `_fAmp1/2`, without changing the effective wave
  amplitudes. A `MaxSeaHeight` notification reads the bound root's value
  (default 50), calculates the original shared scale and then publishes both
  effective amplitudes. Writing the root without notifying does not apply it.
- `Sea2.AnimSpeed1/2`, `Scale1/2` and `MoveSpeed1/2` update their represented
  fields immediately. MoveSpeed keeps all three native components, including
  the Y speed and accumulated Y movement unused by scalar WaveXZ. It retains
  scanf's partial assignment: unmatched components keep their previous values,
  and the literal comma must appear immediately after each parsed float.
- `Stop` reads the bound root's DWORD, rather than the changed leaf's value.
  A stopped Realize returns before time conversion, animation or map preparation.
  Ships may still move through that static surface. Resume preserves phase.
- `MaxSeaDistance` also reads the root; finite signed distances are retained
  because the native cutoff squares the value.
- The **parent** named `isDone` requests Realize(0) before subsequent root-name
  checks. A leaf named `isDone` by itself does not trigger that condition.
  Stop also blocks this zero-time preparation. Sea2 and fog branch boundaries
  prevent unrelated leaves from falling through into root controls.

The controller applies finite surface-compatible changes transactionally.
Invalid/stale/foreign bindings, non-finite values, negative animation speeds,
unrepresentable amplitude limits and invalid animation steps are rejected.
Attribute-store writes remain owned by the caller and are not rolled back.
The move-vector scanner supports defined finite decimal spellings; hex floats,
incomplete exponents and non-finite spellings are explicitly rejected rather
than asserted to match all platform-specific scanf behavior. Surface snapshots
and prepared maps already handed to the renderer remain independently owned.

Visual-only Sea2 attributes, fog, UnderWater, block dimensions and texture
rebuilds are not implemented by this numeric controller. Query camera binding,
original global registration and Execute/Realize ordering remain separate work.

## Integration fixture

Use `--native-sea-weather` alongside `--particle-fleet-demo
--native-ship-motion --native-ship-rocking --native-sea`. An external original
frame directory can also be supplied with `--sea-frames-dir`.

The newly written `fixtures/sea_weather.c` obtains an explicitly supplied sea
object, writes the original attribute names and schedules six events. At 0.5s
the sea becomes calm, at 1s it becomes rougher, at 1.5s it stops, at 2s it
resumes; at 2.5s raw amplitudes change and at 2.75s the height limit applies them.
These are diagnostic transitions, not original game weather scripts or a new
physical rule tying waves to wind. Weather parameters must come from original
contracts as the surrounding services are connected.

The preview adapter inserts a `sea` entity with an explicitly bound script
attribute root. Script writes pass through entity attribute notifications to
the retained surface controller. Posted events are delivered once per fixed
fleet step before that step's surface animation. The report identifies this
adapter and its incomplete native entity scheduling. The water and both ships
observe the same resulting surface. Dropping a ship does not drop the sea;
viewer restart constructs a fresh script, attribute tree and surface.

Weather-mode world-particle pose histories opt into pinned trigonometry and
scalar transform/interpolation arithmetic. The initial cross-host comparison
exposed a one-float-ULP difference in two world-particle bounds on Linux and
Apple Silicon. These preview frame calculations now use an explicit shared
policy; existing modes retain their preceding history behavior. This is a
presentation adapter, not the full original matrix/entity service.

## Validation

`storm-sim/reference/sea_weather/generate.py` compiles the complete unedited
C++ AttributeChanged method, retained animation block and real ATTRIBUTES.cpp
numeric parsing with UBSan. Codec interning and visual/render callbacks are
adapters. The 32 sequences contain 1056 complete numeric snapshots, covering
pending/effective amplitude order, root reads, case-insensitive names, partial
vectors, signed distance, stop/resume, zero-time preparation and branch shadowing.
Regeneration must be byte-identical. The preceding 64 map / 6144 query oracle
remains unchanged.

Additional checks exercise invalid-update atomicity, preserved snapshots,
script/entity dispatch, 24 callbacks and six delivered events, a calm live
surface, stopped maps while ships continue moving, phase-preserving resume,
fixed-step partitioning and fresh-session restart. The GPU agreement probe now
sets layer amplitudes and signed distance through the same attribute controller;
192 height/normal queries use a 0.0001 component tolerance.

Full weather/day-night visuals, camera/entity integration, foam/reflection and
playable naval combat remain on the [marine checkpoint](MARINE_INTEGRATION.md)
and [weather roadmap](WEATHER_ROADMAP.md). This increment does not change the
whole-engine estimate of about 20% (15–25%).


## Four-platform evidence, 2026-11-01

Code `2a49e717a1cfae5f76b76de0cdf3fcb053d636fb` passes [PR run 41313148151](https://github.com/STG-44/storm-engine/actions/runs/41313148151)
on Windows x86_64, Linux x86_64, macOS Apple Silicon and macOS Intel. Each host
passes formatting, warnings-denied Clippy, native builds and 393 CPU tests
(135 interpreter). Four archive SHA256 digests and all 132 complete JSON reports
(33 scenarios per host) match local output. All 32 preceding reports remain
unchanged. Target triples and test counts are checked from complete job logs.

Linux passes 24 software Vulkan tests and 21 fleet-window frames, including
three paused weather-mode frames. Local Metal passes 24 GPU tests and three
paused weather-mode window frames. The 192-query GPU agreement probe applies
amplitudes and signed distance through attribute callbacks before comparison.
The weather C++ oracle regenerates byte-identically: 32 sequences and 1056
complete numeric snapshots. The preceding scalar map/query reference is retained.

The new script fixture delivers six events and 24 attribute callbacks. Five
parent-isDone callbacks request Realize(0); four apply, while Stop blocks one.
Tests check a calm live surface, unchanged prepared maps while stopped, continued
ship movement, phase-preserving resume, pending amplitudes, partition-independent
state, repeated presentation and fresh-session initialization. Rendering is
also exercised with original sea frames and ArabellaShip1 in calm, rough and
stopped states; calm/rough captures were visually inspected. The preceding
40-root surface/bounds audit was not repeated for this attribute-only increment.

This validates the numeric controller, notification adapter and stated preview
schedule. It does not establish original weather program startup, complete
native entity/camera scheduling, visual/fog/resource callbacks, complete sea
rendering or naval combat. Hosted Windows/macOS physical GPUs and manual
keyboard/restart actions remain untested. Game resources, captures and local
reports stay outside Git. The whole-engine estimate remains about 20% (15–25%).
