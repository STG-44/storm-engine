# Weather and the day/night cycle

Weather is an unimplemented gameplay/rendering milestone. Existing diagnostic
water, adjustable constant wind and approximate wave response do not provide
the original weather system. The retained reference is `src/libs/weather`:
weather state, sky, astronomy, sun glow, stars/planets, rain and lightning.

- Port the original typed weather parameters and game-time/day-night updates.
- Restore original script-selected weather presets and transitions; verify
  intermediate values and time changes against retained C++ behavior.
- Port sky/clouds, sun/moon/stars, ambient/directional lighting and fog/visibility.
- Restore rain, storms, lightning, thunder and their particle/audio resources.
- Connect weather wind to sail behavior and ship motion, and connect weather
  settings to the original sea once those providers are ported.
- Integrate location changes, gameplay script bindings and weather save/load.
- Verify representative calm, fog, rain, storm, dawn, daylight and night scenes
  on macOS, Windows and Linux, including transitions and resumed saved states.

Commercial sky textures, particle resources and sound stay external. Native
state/arithmetic checks should use retained C++ references; rendering checks
should include temporal transitions and scene visibility. This milestone is
separate from naval combat, land locations, player deck locomotion and boarding.
