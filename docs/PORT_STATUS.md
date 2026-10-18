# Port status estimate

Planning snapshot, 2026-10-18:

| Target | Approximate functional coverage |
| --- | --- |
| Native engine capable of running the original game on macOS, Windows and Linux | 15–20% |
| Ship presentation and sailing demonstration, including remaining polish and platform verification | About 70% |

These are subjective engineering estimates with substantial uncertainty. The
full-game estimate gives considerable weight to the large subsystems still
pending; completing a small milestone does not imply a fixed percentage gain.
The [milestone checklist](RUST_PORT.md#milestones) and reproducible checks describe
the actual implemented scope.

Working foundations include native builds, GPU rendering, original ship assets,
sail sets/rigging/flags and damage presentation, approximate sailing, entities,
messages, attribute trees and a bounded source-language runtime with events, persistent globals and one-dimensional arrays.
Original resources have been exercised on all 40 available ship roots. CI covers
Windows, Linux and both macOS architectures; physical GPU coverage remains
limited.

The largest remaining blocks are complete source/module loading and language
support, the original native game API and handlers, locations and their collision,
interface, sea/land gameplay and AI, complete animation/audio, save/load and
representative end-to-end gameplay on all target platforms. Current ship physics
and water are still a prototype. The full original game cannot yet be launched
through the Rust implementation.
