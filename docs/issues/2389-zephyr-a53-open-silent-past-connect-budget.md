---
id: 2389
title: "A Zephyr image whose network never answers stays silent past its connect budget instead of failing"
status: open
type: bug
severity: medium
area: [zephyr, rmw-zenoh, testing]
related: [phase-466, 2384, 2207]
found: 2026-11-10
---

**Home: [phase-466](../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**.
Split from [issue 2384](archived/2384-zephyr-a53-smp-slirp-boot-flake.md), whose two
failures are fixed; this is the shape left over.

## Symptom

Tier 2's `CorePinPlacement/zephyr/c` cell (`sched_dims_applied_e2e`), on the
qemu_cortex_a53 SMP image with both of 2384's fixes: once in 50 runs under
`taskset -c 0-3` the guest printed its boot banner and network lines, then
nothing for the rest of the 90 s cell timeout. No core-pin marker, no
`ConnectionFailed`, no error. QEMU printed `Slirp: Failed to send packet, ret:
-1` 16 times; in the 19 passing runs of that set it printed none.

The image states `[image.zephyr_smp.knobs.zenoh.limits] connect_timeout_ms =
10000` (`CONNECT_TIMEOUT_MS = 10000` in the built `nros-rmw-zenoh`). Under that
budget a router that never answers should end the open with
`zpico Session -> ConnectionFailed` about 10 s after boot, and the image then
reports it. It did not, so something blocked past the budget.

## Why this is ours

A lossy or dead link is the environment. Never reporting it is not: issue 2207's
contract is that a client connect gives up at its budget, and a board that
waits silently for 80 s past it reads in a test log as a hang with no cause.
Wherever the open is blocked (zenoh-pico's TCP connect, its handshake read, or
the net stack beneath them), the budget has to bound it.

## Upstream note (not ours)

Zephyr 3.7's `drivers/ethernet/eth_e1000.c` sets up a ONE-descriptor receive
ring (`RDLEN = 1*16`) and copies a frame out only on the receiver-overrun
interrupt (`IMS_RXO`). A guest that is slow to run that ISR therefore loses
frames by design, and under host load it does. `Slirp: Failed to send packet`
is what QEMU prints when it cannot hand a frame to the guest NIC, most likely
this. That explains why frames are lost; it does not excuse ignoring the budget.

## Not known yet

Where the open is blocked. It recurred 0 times in the next 30 runs under the gdb
wrapper (`-gdb`, a per-thread dump at 45 s; the dump script is the one 2384 used
to find its `main` thread dead), so no backtrace exists. Next steps:

- keep the dump wrapper on for later loaded runs and read the
  `posix_thread_pool` threads (zenoh-pico's) and `main` from the first hang;
- read zenoh-pico's open path for a blocking call that the connect budget does
  not cover: a TCP connect under `CONFIG_NET_SOCKETS_CONNECT_TIMEOUT` (3000 ms
  here) is bounded, but a handshake read with no timeout would not be.

## Acceptance

- A Zephyr image whose router never answers ends the open with
  `ConnectionFailed` within its connect budget plus one attempt, and says so on
  the console, on a boot where the e1000 is dropping frames.
