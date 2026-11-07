---
id: 2357
title: "The two rv-virt-threadx images of a pair could open zenoh sessions with one ZID, and the router refused the second"
status: resolved
type: bug
area: [threadx, rmw, boards]
severity: medium
found: 2026-11-07
related: [2340, 2355, phase-548, phase-535]
resolved: 2026-11-07
resolved_in: "phase-548 W4: the rv-virt-threadx RNG seed folds in the virtio device MAC and the CLINT counter"
---

## What happened

`test_rtos_service_e2e` for `(ThreadxRiscv64, C)` went red on the phase-548
W4 branch, and the red followed any edit to
`packages/boards/nros-board-common/c/threadx_hooks.c`, even an edit to code
that runs only after the app returns. Main passed. The client printed:

```
zpico: z_liveliness_declare_token failed: -100 for '@ros2_lv/0/756a…/0/0/NN/%/%/add_client'
[TX] TIMEOUT: no completion
```

A packet capture of the client (`-object filter-dump`) shows the router
answering every `OpenSyn` with a `Close` whose reason byte is `0x04`
(MAX_LINKS), then FIN. The client reconnects at once, so it sends about 2,500
frames in a few seconds. That flood exhausts the virtio TX path's
100,000-iteration completion spin, which is where the `[TX] TIMEOUT` lines
come from. They are a symptom, not the cause.

The server's capture shows the cause: its `InitSyn` carries the same ZID as
the client's (`f2683359bd81…`). Both images even used the same ephemeral
source port. The router already had a session with that ZID, so it refused
the client's.

## Why the ZIDs matched

On ThreadX, `zpico_fill_session_zid` (`zpico-sys/c/zpico/zpico.c`) mixes three
things:

- the address of the session object;
- the 100 Hz tick at open time;
- the platform RNG. This is a deterministic xorshift, seeded once by
  `nros_board_compute_rng_seed`.

The rv-virt-threadx seed came from the configured IP and MAC alone. Both
images of a pair use the board's default address, by design (each QEMU has
its own SLIRP network), so both images had the same seed. Only the other two
terms could tell the images apart. The session address depends on link
layout, and the tick is often identical, because both images boot the same
way. A change to any linked file can move the session address, so it can make
those two terms agree. That is why the failure looked like a layout bug.

The QEMU default virtio MAC is also the same for every instance. So two
instances of the same image started without an explicit `mac=` had the same
problem, and so did a rebooted instance whose old session was still inside
the router's lease.

## Fix

`nros_board_compute_rng_seed` on rv-virt-threadx now folds in two more
values:

- the MAC the launcher gave the virtio-net device. It is read through the new
  `virtio_net_nx_read_mac()`, which shares the slot scan and config read with
  the driver's own init and does not change device state;
- the CLINT `mtime` counter (`hwtimer_now()`). It differs between boots.

Measured: the pair answers `Result of add_two_ints: 5`, and the cell passes
3 of 3 runs, 1.8 s each.

## Scope of the class

Hosted ports (threadx-linux, freertos-posix) take the ZID from `getrandom`
before they reach the RNG path. mps2 FreeRTOS images get distinct addresses
from `board-net-NN.toml`. Of the boards that are not dormant, only rv-virt
gave both images of a pair one seed.

Issue 2355 (`critical_section` restore token) was checked first, as a
candidate cause, and ruled out: this client image links no `critical_section`
symbol.
