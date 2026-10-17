# Ship flag selection

The preview can select nations and special ship flags using rules from
[`procGetRiggingData` and `GetShipFlagTextureNum`](https://github.com/storm-devs/sd-teho-public/blob/d897194748fb134b855fb5f85bd3b7333cf00e69/program/battle_interface/BattleInterface.c#L2454).
The nation constants are in [globals.c](https://github.com/storm-devs/sd-teho-public/blob/d897194748fb134b855fb5f85bd3b7333cf00e69/program/globals.c#L73).
This is the published TEHO profile, not a universal table for every Storm game
or mod. Original atlas pixels remain in the user's game installation.

The atlas has four columns: Holland 0, England 1, France 2, Spain 3. These are
not the script's numeric nation IDs. Ordinary `flag` locator groups use row 0.
Special `sflag` groups use a row selected from explicit ship context:

| Context | Row |
| --- | ---: |
| Squadron commander | 1 |
| Naval patrol / escort | 2 |
| Merchant | 3 |
| Surrendered non-pirate ship | 4 |
| Main character / companion / shore boat | 6 |
| Pirate | 7 |

Pirates use the supplied pirate variant as their column, on all flag groups;
surrender and ship role do not override their pirate row. The preview defaults
that variant to zero. The original script may obtain it from character/group
attributes or choose a random group value. Those lookups are not running here.
Quest row 5 is not selected by this ship rule.

`storm-sim` exposes the pure selection rules and the C++ event-result contract:
no reply selects (0,0), a scalar selects (column,0), a pair selects (column,row),
and either coordinate -1 hides the flag. Malformed replies and cells outside
the loaded atlas are errors. A complete multi-flag change validates before
applying, so a bad cell cannot partly change a scene.

## Use

```sh
cargo run -p storm-demo --locked -- --rigging-demo \
  --flag-nation france --flag-role merchant
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --game-dir /path/to/game --flag-nation england --surrendered
cargo run -p storm-demo --locked -- --model /path/to/ship.gm \
  --game-dir /path/to/game --flag-nation pirate --pirate-flag 2
```

No flag option retains the original C++ (0,0) fallback. Roles are personal
(default), commander, naval and merchant. `--hide-flags` hides flag surfaces;
poles, ropes and sails retain their state. `--rig-report` now exports each
flag's node, group, special marker, atlas grid, selected cell and visibility.

Physical F cycles nations, G cycles roles, H hides/restores flags, J toggles
surrender. These controls work with any keyboard layout and ignore key repeats.
The title reports the current selection. Changes reuse the existing GPU scene
and buffers, including when animation is paused; hiding preserves the selection.

Flag UVs stay half a base-level texel inside the selected cell. Flag materials
use base-level filtering to avoid sampling adjacent countries through coarse
atlas mips. The generated fixture uses a four-by-eight color atlas, one ordinary
quad flag and one special pennant, without game textures.

## Validation and remaining scope

Local Intel Mac validation passes 41 CPU tests, ten Metal GPU tests, formatting
and Clippy. Tests cover national columns, special-role precedence, surrender,
pirates, event replies, metadata, UV bounds, atomic failure, hiding/restoring
and repeated nation changes in one persistent GPU scene.

This does not load character saves, execute script events, infer ship roles
from encounters/groups, implement deception or diplomatic reactions, or port
town/fort flag rules. Original flag motion, damage and tension simulation also
remain outside the preview.
