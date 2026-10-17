# Original asset inspection

The first local resource sample is from Sea Dogs: To Each His Own. Resources
remain outside Git; only the inspection code and these results are published.
This records metadata compatibility, not game loading or rendering support.

## Rust header scan, 2026-10-17

```sh
cargo run -p storm-formats --locked -- scan /path/to/game/RESOURCE
```

| Resource | Result |
| --- | --- |
| GM files encountered | 4,136 |
| GM 1.05 headers accepted | 4,129 |
| Triangles declared by accepted GM headers | 23,705,727 |
| TX headers accepted | 5,732 |
| TX header failures | 0 |

The source archive contains 5,724 TX files. Its copy recipe produces 79
additional textures, of which eight match the final manifests directly. The
other 71 are intermediate loading images that still need the installer's
RTPatch reconstruction. Those generated copies are kept outside the resource
directory, rather than treated as validated original assets.

Completeness checks find all 43,398 files from the primary resource and common
music archives. With the eight verified copies, the directory has 43,406 files:
43,405 match a supplied manifest and one local emulator configuration is not
listed in either manifest. Per-path checks allow either supplied language
manifest and handle the legacy Windows-1251 filename. This is a resource sample;
full language/audio/video installation has not been reconstructed.

| TX format | Files |
| --- | --- |
| DXT1 | 2,979 |
| A8R8G8B8 | 949 |
| X8R8G8B8 | 914 |
| R5G6B5 | 308 |
| DXT5 | 255 |
| DXT3 | 192 |
| A4R4G4B4 | 109 |
| A1R5G5B5 | 26 |

Seven GM files are rejected by the current strict reader. Each of these seven
matches its supplied MD5 manifest; they are not extraction mismatches:

| Path relative to RESOURCE | Reason |
| --- | --- |
| `MODELS/big_sky.gm` | GM 1.02; reader currently supports 1.05 |
| `MODELS/sky.gm` | GM 1.02 |
| `MODELS/small_sky.gm` | GM 1.02 |
| `MODELS/light.gm` | GM 1.01 |
| `MODELS/sphere.gm` | GM 1.01 |
| `MODELS/Locations/Incas Temple/Alcove/Alcove_l.gm` | Infinite bounds in a model with no objects |
| `MODELS/Locations/Incas Temple/Labirint/lab1_l.gm` | Declared tables exceed file length by 18 bytes |

The scan therefore exits with status 1 and reports all seven files. Future
decoders must establish the older layouts and an explicit policy for these
locator headers. Passing header inspection does not validate individual
vertices, indices, string references, texture pixels, or BSP records.

The test suite uses the cannon mesh already present in the upstream repository,
plus malformed binary headers. Commercial assets are not required for CI.

The [CI run for inspector commit ca6a70b6](https://github.com/STG-44/storm-engine/actions/runs/39004859635)
passed on Ubuntu 24.04, Windows 2025, macOS 15 Apple Silicon, and macOS 15 Intel.
Each job builds the native inspector, runs the CPU format tests, and inspects
the upstream cannon through its CLI. The existing Linux Vulkan checks also
remain green. These checks do not establish original-game rendering support.
