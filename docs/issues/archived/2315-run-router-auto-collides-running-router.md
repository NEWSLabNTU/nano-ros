---
id: 2315
title: "`nros run` with the default `--router auto` starts a second zenoh router over the one a chapter told the reader to start, and the run stops"
status: resolved
type: bug
severity: medium
area: [cli, run, docs]
related: [2314, phase-524]
found: 2026-11-07
resolved: 2026-11-09
resolved_in: "nros run --router auto probes each TCP listen port with a zenoh InitSyn; a router answering InitAck is reused (a serial link's router connects to it), anything else is refused"
---

**Home:** [phase-524](../../roadmap/phase-524-emulator-twins.md)
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

## Resolution (2026-11-09)

`run_exec::plan_router` decides, before anything starts, what the router
`nros run` would start meets on each TCP listen endpoint. It decides "is a
zenoh router" by measurement: it sends the first message of a zenoh 1.x
session, an `InitSyn` (2-byte little-endian length, header `0x01`, protocol
version `0x09`, an 8-byte id as a client), and reads the reply within 1 s. A
zenoh node answers an `InitAck` (header `0x01` with the A flag `0x20`) whose
id byte carries its whatami in the low two bits; `0b00` is a router. The
probe closes before the `OpenSyn`, so no session is admitted.

- A zenoh router on the link's own endpoint is reused, and `nros run` says
  so: `reusing the zenoh router already listening on tcp/<addr>:<port> (it
  answered a zenoh handshake as a router); starting none`.
- A serial link still starts its router, since the pty is the runner's and
  made for this run, so no router can already hold it. A zenoh router on its
  extra host endpoint (`tcp/127.0.0.1:7447`) is CONNECTED to
  (`connect/endpoints`) instead of collided with, so host nodes still see the
  board's topics.
- Anything else on the port (no answer within 1 s, a reply that is not an
  `InitAck`, a zenoh peer or client) keeps #3306's refusal, naming what it
  saw, `--router tcp/<address>:<port>` and `--router none`.

The starters that start `rmw_zenohd` on 7447 and then run `nros run`
(`starters/mps2-an385-freertos.md`, measured below with a locator on 27447;
`nuttx-arm.md` and `nuttx-riscv.md`, the same board-default path, not
measured) now work as written: their router is reused. None needed an edit.
`starters/mps2-an385-zephyr.md` never collided: a Zephyr image gets no board
default (issue 2301), so `nros run` starts no router for it.

## Verified

In the `ros2` box, 2026-11-09, a router started by hand on
`tcp/0.0.0.0:27447` and images whose `locator` dials 27447:

```
nros run: reusing the zenoh router already listening on tcp/127.0.0.1:27447 (it answered a zenoh handshake as a router); starting none
nros run: matched after 10.5 s: [INFO] fr_node: [   10.358547] Publishing: 'Hello World: 8'
```

on mps2-an385-freertos (Rust), and the same reuse line then `matched after
10.2 s` for `zs_node` on Zephyr `native_sim` (Rust). A `ros2 topic echo
--once /chatter` connected to the hand-started router printed
`data: 'Hello World: 1'` for both. On mps2-an385-baremetal (serial) with a
hand-started router on 7447: `starting the zenoh router on
serial//dev/pts/0#baudrate=115200, connected to the zenoh router already
listening on tcp/127.0.0.1:7447 ...`, matched, and the echo received. A
plain listener on 27447 refused both boards:

```
nros run: something that is not a zenoh router listens on tcp/127.0.0.1:27447 (it gave no zenoh InitAck within 1 s), so the router `nros run` starts there would exit at once. Free the port, name a router of your own with `--router tcp/<address>:<port>`, or start none with `--router none`
```

Unit tests (`run_exec::tests`): a fake answering the `InitAck` bytes captured
from `rmw_zenohd` is reused, a free port starts a router, a serial link
connects to it, and a silent listener, an HTTP reply and a zenoh peer are
refused.

The tests hold every port they probe until the probe is done. The free case
goes through a probe seam (`plan_router_with`), not a port bound and dropped:
#3306's `a_taken_router_port_is_refused_with_the_remedy` asserted a dropped
ephemeral port was free, which a parallel test could take in between, and it
failed once that way in a full parallel run on arm-a100. That test is replaced.
