# Particle projects and atlas selection

The port can now read original particle PRJ descriptors and select their atlas
and profile membership for ship presentation. Existing previews without the
new option retain their previous selected profiles and report shape.

~~~sh
# Native default project: discover root-level XPS names, use particles_list.tga.
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --game-dir /path/to/game --particle-project default --fire

# Named project under RESOURCE/Particles (case-insensitive, .prj appended).
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --game-dir /path/to/game --particle-project Example \
  --particle-profile smoke --sail --sail-script /path/to/startup.c

# Descriptor inspection needs no game installation or GPU.
cargo run -p storm-formats --bin storm-assets --locked -- decode /path/to/Example.prj
~~~

An absolute --particle-project path explicitly selects an external descriptor.
Relative names stay under RESOURCE/Particles; default.prj selects a file,
while the exact token default selects native default discovery.

## Native contracts

ParticleService::Init calls OpenDefaultProject, not the supplied AoP.prj.
OpenDefaultProject selects particles_list.tga and enumerates root-level XPS
files. OpenProject reads Textures/MainTexture (default none), then numbered
Manager/System_0000 through System_9998. The first missing or valueless key
ends that sequence. A missing profile file does not end it. The preview reports
declared names, available files and missing files separately.

The reader retains native INI behavior: only spaces/tabs are trimmed; CR, LF
and NUL separate lines; a leading semicolon denotes a comment. Inline
semicolons and quotes remain literal. Section names preserve their interior
whitespace. Section/key lookup ignores ASCII case, with the first section and
first matching key winning. Values preserve their bytes, with UTF-8 or
Windows-1251 display. This is a bounded reader for the project subset, not a
replacement for every INIFILE API or encoding.

Project input is capped at 1 MiB / 32,768 split lines; consumed strings must
fit the native 8,192-byte buffer. Default discovery is capped at 9,999 files
and sorted by portable cache key, since filesystem enumeration order varies.
Profile/project references retain relative ASCII path checks and both separator
styles. Absolute descriptor selection does not relax references inside it.
Case collisions and directory escapes fail explicitly.

SetProjectTexture passes the original color name to the renderer. The checked
global resolver uses RESOURCE/Textures and appends .tx unless already present,
including a supplied resource/textures prefix. Ship texture overrides and
basename fallback do not apply. Native stem(name) + nm determines the normal
request at the texture root; e.g. dir/atlas.tga requests atlasnm.tx. Existing
normal maps are decoded and retained, but the current billboard shader still
uses color only. Missing optional normal maps are recorded, without inventing
another spelling.

Named projects restrict the available presentation profiles. Existing ship
defaults are included only when available in that project; extra requested
profiles must be members. A gap cannot be bypassed by finding a later key.
Missing color atlases, malformed/unsupported selected data and unsafe references
fail loading rather than displaying a substituted image.

## Scope and verification

The catalog describes available files. GPU resources and native fire handles
still use the explicitly selected billboard subset (up to the preceding
thirty-two-profile preview limit); unselected files are not claimed as loaded
DataSources. Complete global cache/GeomCache registration, lazy arbitrary
profile creation, live project switching, native normal/blend techniques,
model/attached particles, raw Targa/DDS fallback, renderer TexPaths, manager RNG
and full gameplay remain pending.
Selection occurs before startup scripts and scene creation.

The reference generator compiles unedited native KEY_NODE/SECTION storage,
IFS formatting/lookup/ReadString and OpenProject/OpenDefaultProject/
SetProjectTexture under UBSan. Seventeen complete snapshots cover ordering,
gaps, missing/empty keys, duplicate sections/keys, literal quotes/comments,
legacy bytes, atlas/normal requests, missing projects and default discovery.
I/O, renderer handles, cache writes, reset/geometry-cache hooks and output are
adapters; no commercial files are committed.

CPU tests cover those snapshots, checked limits, path/extension/stem rules,
project membership, missing files, global texture overrides and empty-locator
ships. A generated XPS/TX fixture verifies a red project atlas against the blue
default through the GPU, repeat draws and restoration of default pixels.
Its fixture contains no original artwork. PRJ inspection is the seventeenth
resource-free CI scenario.

In the local resource set, AoP.prj declares thirty entries,
seven absent profile files and an unavailable Particles_3.txx color request
(the renderer would request particles_3.txx.tx). It is inspectable but cannot
render from this resource folder; the loader reports that missing atlas.
The original default-project mode uses the available particles_list.tga atlas.
