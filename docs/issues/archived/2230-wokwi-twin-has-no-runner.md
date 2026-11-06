---
id: 2230
title: "The Pico W's Wokwi twin has no runner package, so `nros run` refuses it"
status: wontfix
type: limitation
severity: low
area: [cli, runner, zephyr]
related: [phase-524, phase-529]
found: 2026-11-03
---

> **Closed 2026-11-06 as `wontfix`** by the maintainer ruling recorded in
> [RFC-0115](../../design/0115-emulator-first-board-policy.md) Amendment 1: a
> board's emulator twin must be free and open source, and Wokwi, a paid hosted
> service, gets no first-class support. The runner this issue asked for will
> not be written in tree. See "Resolution" below and
> [#2283](2283-wokwi-demoted-to-community.md) for the cleanup.

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md)** — homed 2026-11-04: W4b: the Pico W twin's `nros run` road; phase-529 is archived.

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
together. (Superseded, see "Resolution".)

## Resolution

`wontfix`, 2026-11-06. The maintainer ruled that Wokwi is a paid service and
nano-ros gives it no first-class support: a twin counts only if it is free and
open source (QEMU, Renode), and a paid hosted simulator is never a board's
twin, runner, probe track or SDK-index tool (RFC-0115 Amendment 1). So the
gap this issue tracked is gone rather than filled:

- `rpi-pico-w-zephyr` states no `[board.run]` any more. It has no twin
  and flashes by UF2, so `nros run` gives it the ordinary answer for silicon
  and points at `nros flash`.
- `run_plan::RUNNERLESS_EMULATORS` is deleted, and `wokwi` left the emulator
  kind vocabulary (`EMULATOR_KINDS`), so a descriptor naming it is refused
  like any unknown kind. `check-board-runner-resolves` has no exemption left.
- `scripts/probe/twin/wokwi.sh`, the `wokwi-pico` probe track and
  `[tool.wokwi-cli]` are removed (issue 2283).

Anyone who wants Wokwi can build it out of tree, as a runner package through
the RFC-0119 extension mechanism with their own account and token; nano-ros
does not test or ship it. Such a runner would also need an open emulator-kind
mechanism, since the descriptor schema now refuses `wokwi:`, and nano-ros does
not plan one. The W1 spike logs
(`docs/roadmap/data/phase-524-w1-wokwi-pico*.log`) stay as dated history.
