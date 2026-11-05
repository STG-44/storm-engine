# Waves and hull response

The sailing prototype now has heave, pitch and roll driven by a shared analytic
sea. All loaded hull, mast and sail meshes receive the same rigid transform;
cloth deforms in that moving frame. These are new approximate dynamics, not a
port of Storm's original sea or buoyancy code.

```sh
cargo run -p storm-demo --locked -- \
  --model /path/to/game/RESOURCE/MODELS/Ships/Barkentine1/barkentine1.gm \
  --game-dir /path/to/game --sail --wind-speed 12 --wind-direction 180 \
  --heading 180 --wave-height 3 --wave-direction 115
```

Sailing defaults to a 1.5 m crest-to-trough envelope. `--wave-height` accepts
0–4 m; zero starts with a flat surface and no hull rocking. This is the maximum
envelope, not a statistical significant wave height. Wave direction defaults
to the initial wind direction, and remains independent of later wind controls.
The static model preview continues to use a flat surface with shading ripples.
Waves and cloth use simulation time, including after live catch-up is capped;
Space pauses all three. The camera follows horizontal travel with a level
horizon, without following hull heave or rotating with pitch/roll.

Add `--time 12 --headless waves.png --state-json waves.json` for a replay.
State JSON includes heave in metres, pitch positive bow up, roll positive
starboard up, their rates, and the wave envelope/direction. The original ship
and its captures remain outside Git. CI uses a small upstream cannon fixture
with a 0.15 m envelope to exercise transforms and rendering only.

## Model

Two deep-water sine components have wavelengths 64 m and 26 m, envelope weights
70%/30%, and directions separated by 0.65 radians. Angular frequency follows
`sqrt(9.81 * wave_number)`. CPU sampling returns height and its analytic X/Z
gradient. The renderer receives these exact coefficients in a uniform buffer;
WGSL uses the same height and gradient for water depth and normals.

At each 1/120 s tick, five points sample the sea: centre, bow, stern, port and
starboard. Longitudinal samples are at 40% of configured length; beam is
approximated as 25% of length. Their weighted mean and opposed height
differences set heave/pitch/roll targets. Damped springs respond with natural
frequencies 1.2/1.0/1.4 rad/s and damping ratio 0.85. The model starts level;
response builds smoothly rather than jumping to the initial wave height.

After the loader's Z reflection, model yaw is heading + pi, positive local X
points port, and renderer roll is the negative of simulated starboard-up roll.
The transform is `translation * Ry * Rx * Rz`; normals rotate with it. Cloth
uses the inverse full rotation of apparent wind, projected into local X/Z.
Its current wind interface does not model the vertical component.

The water pass brackets the camera ray between crest/trough envelope planes
and performs 18 bisections. It writes displaced depth before drawing geometry,
so a crest can hide a surface that reappears in a trough. Shading gradients
fade at distance to reduce horizon aliasing. This preview assumes an above-water
camera; grazing rays can have multiple roots, and this method does not guarantee
the nearest one. There is no underwater view, foam, breaking waves, refraction,
physical wake, wind-induced heel, horizontal wave force, or fluid-volume solver.
The chosen hull dimensions and response constants need calibration against
original game data. Planar sailing forces are still independent of hull tilt.

## Validation

Local macOS Intel / Radeon Pro 555X passes 24 CPU tests, six Metal GPU tests,
formatting and Clippy. Wave checks cover analytic derivatives, bounded height,
calm water, equal state under different frame partitions, an hour at 4 m,
settling after waves stop, and reflected pose signs. A GPU compute probe compares
128 height/gradient samples with CPU results, with tolerance 0.0002. A rendered
red plate disappears under a crest and emerges above a trough. Existing GPU
checks now also exercise heave, pitch and roll in a persistent scene.

The original Barkentine1 loads 20 GM files, three original textures and all
12 sails (including two triangles), renders through Metal, and presents three
paused native frames with waves. The local animation samples 12–21.75 s at
0.25 s intervals and plays 2.5 times faster; it stays outside the repository.

The [CI run for code commit c3b2520e](https://github.com/STG-44/storm-engine/actions/runs/39010786290)
passed all four jobs on 2026-10-17: Windows 2025, Ubuntu 24.04, macOS 15 Apple
Silicon and macOS 15 Intel. Each passes 24 CPU tests, formatting, Clippy and native
builds. Linux passes all six GPU tests on Vulkan/Mesa llvmpipe, wave capture/state
export and three paused native wave frames. The downloaded wave artifact contains
nonzero heave/pitch/roll at 12 s with a 0.15 m envelope; its PNG was inspected.
Windows and hosted macOS establish CPU/build compatibility; original ship
rendering is verified separately on local Metal with supplied game resources.
