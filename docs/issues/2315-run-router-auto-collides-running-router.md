---
id: 2315
title: "`nros run` with the default `--router auto` starts a second zenoh router over the one a chapter told the reader to start, and the run stops"
status: open
type: bug
severity: medium
area: [cli, run, docs]
related: [2314, phase-524]
found: 2026-11-07
---

**Home:** [phase-524](../roadmap/phase-524-emulator-twins.md)
-- W4. Found on the Zephyr chapter's `native-sim-rust` track once issue 2314
let the Rust talker build.

## What

`book/src/guide/platforms/zephyr.md` starts the router first (probe 60,
`probe-keep`):

```
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
    ros2 run rmw_zenoh_cpp rmw_zenohd
```

and then runs the talker (probe 70): `nros run rs_node`. The default
`--router auto` (`run_exec::host_side`) always plans `ZenohListen` for a
local zenoh locator, so `nros run` starts its own `rmw_zenohd` on
`tcp/127.0.0.1:7447`. That one fails on `Address already in use (os error
98)`, and `nros run` stops the target:

```
nros run: starting the zenoh router on tcp/127.0.0.1:7447
*** Booting Zephyr OS build 36940db938a8 ***
nros run: the router exited (code 1) after 0.1 s, so the target cannot work
```

Measured in the `ros2` box on 2026-11-07 (`tmp/w-bug4`). With
`--router none` against the same router the image publishes, so the image
and the router are fine.

The same "start the router, then `nros run`" shape is in other chapters
(`nuttx.md`, `freertos.md`, `threadx.md`, `bare-metal.md`), so this is a
class question, not one chapter's.

## Options

1. `--router auto` checks whether something already listens on the locator
   and, if so, uses it (reporting that) instead of starting one.
2. The chapters drop their router step and let `nros run` start it, keeping
   the router block for a run without `nros run`.

Either closes the `zephyr-rust` track's `awaits`.

## Ruling (user, 2026-11-07)

Option 1: `--router auto` REUSES a zenoh router already listening on the
locator instead of starting a second one, and says so. The refusal another
session's branch adds (`feat-board-derived-locator`, "b2") stays for a port
held by something that is not a zenoh router. Build on that refusal path once
b2 merges. The same collision likely hits every chapter that starts its own
router and then runs `nros run` (`nuttx.md`, `freertos.md`, `threadx.md`,
`bare-metal.md`, `zephyr-west-app.md`); only the Zephyr Rust case was
measured.
