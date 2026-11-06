---
id: 2283
title: "Wokwi is a paid hosted service, so it is nobody's twin: the Pico W keeps no twin and the Wokwi tooling leaves the tree"
status: resolved
type: tech-debt
severity: low
area: [boards, cli, probe, sdk, docs]
related: [phase-524, 2230]
found: 2026-11-06
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md)** (W4b's Pico W half).

## What

From phase-524 W1's rerun (2026-11-01) the Raspberry Pi Pico W was the book's
field solutions board, with Wokwi as its emulator twin. Wokwi is a hosted,
metered service: every run needs an account token and spends that account's
simulation minutes. The free plan's quota already ran out once (ruling
2026-11-02 took the twin out of CI), and the twin's `nros run` road never got
a runner (issue [#2230](2230-wokwi-twin-has-no-runner.md)).

The tree still presented Wokwi as a supported twin in many places:

- the board descriptor's `[board.run] emulator = "wokwi:pi-pico-w"` and its
  `[board.run.wokwi]` wiring;
- the support registry row (`runs = "emulator"`) and its book notes;
- the `wokwi-pico` probe track, `scripts/probe/twin/wokwi.sh`, and the
  `just probe wokwi-pico` recipe;
- `[tool.wokwi-cli]` in `nros-sdk-index.toml`, installed by
  `nros setup rpi-pico-w-zephyr`;
- `wokwi` in the descriptor's `EMULATOR_KINDS` and the resolver's
  `RUNNERLESS_EMULATORS` table, with an exemption in
  `check-board-runner-resolves`;
- RFC-0114/0115/0116/0118/0119/0124 and the book's board pages.

## Ruling

Maintainer, 2026-11-06: Wokwi is a paid service, and nano-ros gives it no
first-class support. A board's emulator twin must be free and open source
(QEMU, Renode). A paid hosted simulator can never be a board's twin, runner,
probe track, or SDK-index tool. Recorded as
[RFC-0115](../../design/0115-emulator-first-board-policy.md) Amendment 1.

## Resolution

- RFC-0115 gains Amendment 1; RFC-0114, 0116, 0118, 0119, 0124 and the design
  README stop listing Wokwi as a twin, each keeping the history ("was, until
  2026-11-06").
- `rpi-pico-w-zephyr` states no `[board.run]`; it flashes by UF2 through
  `west flash`. Its registry row is `runs = "none"`, with no `cadence` or `last_run` (maintainer
  decision 2026-11-06): the 2026-11-01 Wokwi run is history, not evidence,
  and no bench run exists yet (phase-524 W5, deferred), so its board page
  reads "Not supported". `hardware`, `real_hardware` and the flash facts stay.
  `nros run` on it gives the
  ordinary answer for silicon and points at `nros flash`.
- `wokwi` left `EMULATOR_KINDS` in the descriptor schema and in
  `scripts/lib/board_registry.py`, `BoardRunWokwi` is gone, and
  `run_plan::RUNNERLESS_EMULATORS` is deleted (it held only `wokwi`). An
  unknown emulator kind is still refused by the descriptor's own check.
  `check-board-runner-resolves` has no exemption left.
- The `wokwi-pico` probe track, its twin script and its recipe are removed;
  `scripts/probe/probe-tracks.json` is regenerated.
- `[tool.wokwi-cli]` leaves the SDK index, and the Pico W's setup set is the
  Zephyr SDK alone.
- Issue #2230 is closed `wontfix`.
- The book's Pico W pages say it has no twin, with at most one line
  noting that a reader can try Wokwi with their own account and that
  nano-ros does not test it.

Anyone who wants Wokwi can build a runner out of tree through the RFC-0119
extension mechanism. The phase-524 W1 spike logs
(`docs/roadmap/data/phase-524-w1-wokwi-pico*.log`) stay as dated history.
