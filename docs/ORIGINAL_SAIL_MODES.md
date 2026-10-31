# Original three sail sets

The original engine selects sail sets, rather than scaling every sail to the same
fraction. The Rust preview now preserves that selection for every loaded model.

- 0: all sails gathered (`ROLLTYPE_NONE`).
- 0.5: normal sails completely open; full-only sails gathered (`ROLLTYPE_NORMAL`).
- 1: all sails open (`ROLLTYPE_FAST`).

`SHIP::Execute` changes the request by 0.5 on a control activation. W/S now do
that too, ignoring repeats until release. In `SAIL::MSG_SAIL_GET_SPEED`, values
below 0.3 select none, above 0.7 select full, and the inclusive middle band
selects normal. The core preserves these thresholds; preview CLI accepts 0, 0.5, 1.

## Per-sail rules from the original C++

`SAIL::AddSailLabel` defaults to normal. On a node whose name starts with `rey`,
a sail is full-only when name character 4 is `a` (for example `rey_a2`), or its
immediate parent starts with `mast` and the numeric suffix is 1. Locator types
`sailv` and `sailn` override membership to full-only. This is a generic naming
rule, with no ship-name table. Node/parent names are retained through recursion.

Sources: [ship controls](../src/libs/ship/src/ship.cpp),
[set selection and AddSailLabel](../src/libs/rigging/src/sail.cpp), and
[ROLLTYPE values](../src/libs/shared_headers/include/shared/sail_msg.h).

Normal and full-only groups animate independently. A transition changes only
sails whose requested state changes. Four seconds is still the prototype's
chosen full gathering time; original per-character rolling speed, sail turning,
original folding shapes, damage and gameplay are not ported.

Drive uses the actual weighted deployment of both groups. Weights reproduce
`SAILONE::SetSail` local locator dimension products (including its triangular
side-length product), not simply sail counts or half of each cloth surface.
Absolute forces still use the approximate sailing model and configured total
area. A model without sails has zero effective sail area.

## Reproduce and inspect

```sh
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --game-dir /path/to/game --sail-setting 0.5
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --untextured --rig-report rig.json
```

The report exports each sail's group, node, parent, tier and size weight. It
fully decodes static geometry and exits without a GPU device. `--sail-setting`
chooses the initial set; `--sail-target` chooses a different target during
sailing replay. JSON includes both group deployments, mode and weighted area.

## All available ship models

40 model roots, 776 reachable GM files and 368 sails were checked locally.
Rust's full-decoding reports match an independent locator-only implementation
of the literal C++ rules, sail by sail, for all 40. There are 184 normal and 184
full-only sails overall. Boat has no sail locators. Counts below refer to this
resource collection; other games/mods have not been exhaustively tested.

The percentage is the normal-set share of original size weights, not original
game speed. Original game assets and rendered ship captures remain outside Git.

| Model | Open in normal set | Open in full set | Normal weight share |
| --- | ---: | ---: | ---: |
| ArabellaShip1 | 5 | 16 | 36.1% |
| Barkentine1 | 7 | 12 | 45.0% |
| Barque1 | 5 | 12 | 36.3% |
| Battleship1 | 3 | 8 | 36.2% |
| Boat | 0 | 0 | 0.0% |
| Brig1 | 11 | 13 | 69.6% |
| Brigantine1 | 6 | 10 | 49.0% |
| Caracca1 | 2 | 5 | 40.6% |
| Caravel1 | 2 | 6 | 34.8% |
| Corvette1 | 4 | 16 | 34.0% |
| Corvette_quest1 | 6 | 12 | 49.4% |
| EastIndiaMan1 | 3 | 10 | 33.9% |
| Fleut1 | 3 | 7 | 50.3% |
| Flyingdutchman1 | 8 | 13 | 36.4% |
| Flyingdutchman_l1 | 8 | 13 | 36.4% |
| Frigate1 | 3 | 8 | 32.7% |
| Frigate_h1 | 5 | 13 | 37.2% |
| Galeon_h1 | 6 | 9 | 58.3% |
| Galeon_l1 | 2 | 6 | 34.9% |
| LineShip1 | 4 | 8 | 39.6% |
| Lugger1 | 3 | 5 | 32.0% |
| LuggerQuest1 | 4 | 7 | 39.1% |
| Manowar1 | 10 | 15 | 53.9% |
| Manowar_e1 | 10 | 15 | 53.9% |
| Mayfang1 | 2 | 4 | 48.4% |
| Mirage1 | 4 | 11 | 35.7% |
| Navio1 | 5 | 9 | 49.5% |
| Pinnace1 | 3 | 8 | 34.7% |
| Polacre1 | 4 | 7 | 42.5% |
| Polacre_q1 | 4 | 7 | 44.0% |
| Santisima1 | 11 | 18 | 58.8% |
| Schooner1 | 7 | 9 | 53.3% |
| Schooner_w1 | 2 | 7 | 18.2% |
| Shnyava1 | 6 | 11 | 49.5% |
| Sloop1 | 2 | 8 | 19.8% |
| Tartane1 | 1 | 2 | 73.3% |
| Valciria1 | 6 | 13 | 44.6% |
| WarShip1 | 5 | 10 | 40.5% |
| WarTartane1 | 1 | 2 | 57.5% |
| XebekVML1 | 1 | 3 | 12.9% |

Local checks pass 30 CPU tests and eight Metal GPU tests. A persistent GPU
scene confirms that normal-mode cloth exactly matches full-mode cloth for the
normal tier and furled-mode cloth for the full-only tier. Fleet decoding and
classification are separate from visual inspection; not all 40 ships have been
rendered or checked for gameplay compatibility.

The [original-set CI run](https://github.com/STG-44/storm-engine/actions/runs/39026148582)
passes all four platform jobs with 30 CPU tests, formatting, Clippy and native
builds. Linux also passes eight software Vulkan GPU tests, captures the
normal-set fixture and opens its native window. Hosted Windows/macOS jobs
check CPU behavior and builds; local Metal supplies physical-GPU evidence.
