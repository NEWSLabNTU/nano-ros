---
id: 2282
title: "`nros new --board <board>` printed next steps that were not the board's: native_sim's `west build` line, then `nros run` for boards it cannot run"
status: resolved
type: bug
severity: low
area: [cli]
related: [1519, 2230]
found: 2026-11-05
---

## What

`nros new` ends with "Next steps". It printed them for the board the user named in two different ways, and each was wrong for some boards.

**As reported** (phase-524 W4b, measured on origin/main 5c2b018a0c):

```
$ nros new nucleo_ws --board nucleo-f767zi-zephyr --template talker --lang c
...
Next steps:
  # a stock Zephyr app: with ZEPHYR_BASE set (or from inside your west workspace)
  west build -b native_sim/native/64 nucleo_ws -- -DZEPHYR_EXTRA_MODULES="$(nros locate nano-ros)"
  # or let nros hand off to west: cd nucleo_ws && NROS_ZEPHYR_WORKSPACE=<dir with zephyr/> nros build
```

The package's own `system.toml` says `board = "nucleo-f767zi-zephyr"`, which is `west_board = "nucleo_f767zi"` with a Renode twin. `cargo_nano_ros::scaffold::next_steps(name, lang, platform)` took only the platform word, and its Zephyr arm hardcoded `-b native_sim/native/64`. Every Zephyr board except `zephyr` got that line.

phase-531 W3/W4 (b719d169ed) then replaced the single-package front door, while this issue was open. `nros new <ws> --board <b>` now writes a workspace and prints:

```
Next steps:
  cd <ws>
  nros run <node>
```

That is right for the Nucleo. It is printed for **every** board, though, and some boards cannot run (measured on origin/main fc98ba42f0):

- `rpi-pico-w-zephyr`: `nros run <node>` refuses with "board `rpi-pico-w-zephyr` runs under `wokwi:pi-pico-w`, and no runner package drives `wokwi` yet (issue 2230)". The board states `[board.flash]`, and `nros flash <node> --dry-run` resolves a plan (the `west` runner).
- `s32z270-freertos` states neither `[board.run]` nor `[board.flash]`, so `nros run` is refused ("states neither ... so nothing says how it runs").

`scaffold::next_steps` still hardcodes native_sim, but no CLI path calls it now: only `scaffold_package`, which nothing outside its own tests reaches. It is left as it is.

## Why

Both versions wrote the next steps without asking the board. The first used a constant west id. The second uses a constant verb. The board's descriptor answers both questions, and `nros run` / `nros flash` already read that answer through `run_plan::resolve`.

## Resolution

`run_plan::start_verb(descriptor) -> Option<Verb>` returns the board half of `resolve`'s choice, in `resolve_run` / `resolve_flash` order:

- `Run` for a host process, or for an emulator a runner drives (not a `RUNNERLESS_EMULATORS` kind);
- `Flash` for a board with a `[board.flash]` method or command, or a Zephyr board (its own west runner);
- `None` for a board that states neither.

`cmd::new::workspace_next_steps` prints it: `nros run <node>`, `nros flash <node>`, or `nros build` plus a line saying the board names no target `nros` can start the image on. Measured after the fix:

```
nucleo-f767zi-zephyr  nros run w_nucleo_f767zi_zephyr_node
rpi-pico-w-zephyr     nros flash w_rpi_pico_w_zephyr_node
s32z270-freertos      nros build   (+ the note)
linux                 nros run w_linux_node
mps2-an385-zephyr     nros run w_mps2_an385_zephyr_node
```

## Verified

- `run_plan::tests::start_verb_agrees_with_the_resolver`, over every board in the catalog:
  - if `start_verb` says run, `resolve(Run)` succeeds;
  - if it says flash, `resolve(Run)` fails and `resolve(Flash)` succeeds;
  - if it says nothing, neither resolves.
- `new::single_package_tests::the_next_steps_start_the_image_the_way_the_board_says`: every board's second line is its verb; the Nucleo runs, the Pico W flashes. Mutation (the flash arm printing `nros run`, the old behaviour): the test fails.
