---
id: 2230
title: "The Pico W's Wokwi twin has no runner package, so `nros run` refuses it"
status: open
type: limitation
severity: low
area: [cli, runner, zephyr]
related: [phase-529]
found: 2026-11-03
---

## What

`rpi-pico-w-zephyr` states `[board.run] emulator = "wokwi:pi-pico-w"`
(phase-524 W1's twin). Until phase-529 W2 the RunPlan resolver sent every
emulator on a west build to the `west` runner, which boots a Zephyr build
through the `run_<emulator>` ninja target its board declares. Zephyr
declares no Wokwi target, so the plan resolved and the runner refused it at
run time, after the build.

Since phase-529 W2 the resolver sends only the emulators Zephyr's own build
runs (`qemu`, `renode`) to `west`. Wokwi gets its own runner name, and
because no package provides it yet the resolver refuses at plan time
(`nros run --dry-run` already says so, exit 3). The refusal names this issue.
`check-board-runner-resolves` reports the board as this known gap instead of
failing, and fails when a package starts providing `wokwi` while the
exemption is still in place.

## Why it is not fixed here

A `nros_runner_wokwi` package is more than a command line:

- It writes a Wokwi project: `diagram.json` wired from `[board.run.wokwi]
  link` and `wokwi.toml` naming the ELF and UF2.
- It needs `wokwi-cli` from the SDK store in the plan's tools, which nros
  does not resolve for any runner but QEMU yet.
- `wokwi-cli --interactive` carries the zenoh UART on its stdio, so the
  runner must make a pty for the link and report it
  (`nros-runner: link <pty>`). The console UART is not wired, so stdout
  carries nothing.
- It needs `WOKWI_CLI_TOKEN`, and every run spends that account's simulation
  minutes.

`scripts/probe/twin/wokwi.sh` does all of this in shell for the twin probe
track today.

## Workaround

Run the twin through the probe track (`scripts/probe/twin/wokwi.sh`), or
flash the real board with `nros flash` (phase-529 W3) through `west flash`.

## Closing

Write `packages/runners/nros_runner_wokwi`, then remove `wokwi` from
`run_plan::RUNNERLESS_EMULATORS`. The gate fails until both are done
together.
