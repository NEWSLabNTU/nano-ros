---
id: 2302
title: "Tier 2 stops in the build: the `zephyr_self_pkg_rust` west fixture fails `nros dev codegen system` at configure (and a linux leaf misses `std_msgs`)"
status: open
type: bug
severity: high
area: [testing, zephyr, ci]
related: [phase-528, 2296, 2189, 1158]
found: 2026-11-06
---

## What

Since 2026-11-06, every `run-matrix.yml` run on `main` stops in `just build
tier2`, before any cell runs (`just matrix-triage`: "NO VERDICT: stopped in
the build"). One of the five west fixtures fails at configure:

```
CMake Error at zephyr/cmake/nros_system_generate.cmake:308 (message):
  nros dev codegen system failed (rc=1): ...
  Error: codegen-system: SystemModel ...
FATAL ERROR: command exited with status 1: /usr/bin/cmake ...
  -B.../build/west-fixtures/3.7/zephyr_self_pkg_rust -GNinja
  -DBOARD=native_sim/native/64 ...
west-fixtures: 1 of 5 fixture(s) FAILED to build.
```

The CI log truncates the message after `SystemModel`, so the reason is not
yet known.

Runs, all on `main`:

| run | event | head | stage reached |
| --- | --- | --- | --- |
| 41979554190 | schedule, 2026-11-05 06:18 | — | cells |
| 42155998964 | dispatch, 2026-11-06 03:21 | `a9e1941b13` | build (this fixture) |
| 42173661457 | schedule, 2026-11-06 06:19 | `658f2d8ac1` | build (this fixture) |

| 42195214776 | dispatch, 2026-11-06 09:27 | the phase-528 W5 step-2 branch | build (this fixture) |

From 42173661457 on, a second stage fails beside it: `linux` (rc=2),
`build-fixture-rust` with `error: failed to load manifest for dependency
std_msgs` (a leaf whose `generated/` tree was not produced). The tier-1 cells
job in every one of these runs fails the same two stages.

## Cause: issue 2296 (2026-11-06)

This is [issue 2296](archived/2296-fixture-model-stale-after-resolver-pin.md),
filed in parallel and fixed on `main` by its own pull request. Both reds are
one refusal. The `tier1-junit` stage logs name it:
`SystemModel … is stale: resolver pin changed (model b5243446b761 != ours
8d2c78d40671)`, for the `zephyr_self_pkg` model (zephyr stage) and the
`image_config_ws` model (linux stage). The linux stage's
`failed to load manifest for dependency std_msgs` follows from the same
refused configure. `nros configure`'s stamp recorded the resolver's path but
not its pin, so after phase 536's play_launch bump it said "up to date" over
models `codegen-system` then refused. The stamp now records the pin.

Reproduced locally on a fresh checkout and incrementally: the west fixtures
were built at the last green commit (`1d287ef110`) and then rebuilt at
current `main` with the build dirs and in-source models kept. Current `main`
passes 5/5. The step-2 branch run (42195214776) was cut before the fix.

What is left here is the acceptance: a tier-2 run that reaches the cells.
The before and after runs for phase-528 W5 were dispatched with the fix in
both (see below). This issue closes when they report.

## Why it matters

While it is red, tier 2 has no signal: no cell runs, so a regression
landing now looks the same as this red (issue 1158). Phase-528 W5's step 2
(the QEMU cells on `nros run --plan`) needed a tier-2 cell comparison
before and after the change, and it could only compare the stage reached.

## Leads

- Recent commits that touched this road: phase-536 W1/W4
  (`548ee74edd` "system.toml writes tasks, rlm v0.3.10", `f57516b629`).
  Issue 2189 (archived) is the last fix to these fixtures.
- Reproduce with the full message: configure the fixture with west, or run
  `nros dev codegen system` with the arguments
  `nros_system_generate.cmake` passes, against
  `packages/testing/nros-tests/fixtures/zephyr_self_pkg`.
