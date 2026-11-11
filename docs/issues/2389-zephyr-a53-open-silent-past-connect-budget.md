---
id: 2389
title: "A Zephyr image whose network never answers stays silent past its connect budget instead of failing"
status: open
type: bug
severity: medium
area: [zephyr, rmw-zenoh, testing]
related: [phase-466, 2384, 2207, 2437, 2438]
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

## Handshake budget landed (2026-11-11)

The issue stays OPEN: what landed bounds the open's handshake, which is one
of the waits this issue named, but the symptom (about 80 s of silence on the
a53 under frame loss) is not explained by it and the acceptance (an a53 boot
with the e1000 dropping frames) was not re-run.

### Fixed: the handshake now honours the budget

Reproduced deterministically on `native_sim/native/64` (NSOS host sockets),
the same zenoh-pico client open path, image budget `connect_timeout_ms = 3000`:

| case | before | after |
| --- | --- | --- |
| A: nothing listens on the port (connect refused) | `ConnectionFailed` 3.0 s into the open | same, plus the budget line |
| B: a listener accepts the TCP connect and never sends a byte | `ConnectionFailed` **10.1 s** after the accept | **3.03 s** after the accept |

Linux, same shape (`zenoh_integration`'s
`client_open_against_a_silent_listener_fails_within_a_bound` built with
`NROS_ZENOH_CONNECT_TIMEOUT_MS=3000`): 10.13 s before, 3.06 s after.

Where it waited: `_z_open_connect_locator` (zenoh-pico `src/net/session.c`)
checked the budget only BETWEEN attempts, and `_z_unicast_handshake_open`
used its own `Z_TRANSPORT_CONNECT_TIMEOUT` (10 s) whatever the budget said.
Each receive does return on Zephyr (`SO_RCVTIMEO` is honoured because
`CONFIG_NROS_RMW_ZENOH` selects `CONFIG_NET_CONTEXT_RCVTIMEO`, issue 2105;
NSOS honours it too), so that wait was bounded, just not by the budget.

- zenoh-pico fork, `nano-ros-1.10.1`, `f84c6be5`: a positive budget is passed
  down `_z_open_inner` -> `_z_new_transport` -> `_z_unicast_open_client` /
  `_open_peer` -> `_z_unicast_handshake_open` as an instant; the handshake's
  receive deadline is the earlier of its own and the budget's. Budget `0` (one
  attempt) and `-1` (`connect_forever`) are unchanged.
- `nros-rmw-zenoh` (`zpico.rs`, `report_open_budget_spent`): a failed open
  prints `zenoh open: no session with <locator> within
  connect_timeout_ms=<N> (refused, unreachable, or no answer to the
  handshake)`.
- Regression cell `(ZephyrNativeSim, Rust, Zenoh, ConnectBudget, Example)`,
  `tests/zephyr_connect_budget.rs`, fixture row
  `zephyr-rust-talker-connect-budget-zenoh`: both cases end in
  `ConnectionFailed` with the budget line, between the budget and the budget
  plus 1.5 s (case B from the accept, case A from the spawn plus NSOS's 2 s
  network wait). With the fixture rebuilt on the pre-fix pin `cf228437` the
  silent-listener case fails, `gave up 10.509s after the TCP connect`.

### Left: the a53's 80 s

Under the old code a 10 s budget plus one attempt is at most about 23 s
(3 s connect + 10 s handshake past the budget), and every wait in it is
bounded, so the open cannot account for 80 s of silence. The leading
suspect, NOT measured: the open SUCCEEDED over the lossy link and a later
blocking `send` stalled. Zephyr's `zsock_sendto` waits `K_FOREVER` when
`SO_SNDTIMEO` is unset (zenoh-pico's `tcp_zephyr.c` sets only `SO_RCVTIMEO`),
for as long as TCP retransmits, and Zephyr's retransmission budget
(`CONFIG_NET_TCP_RETRY_COUNT`, exponential backoff from
`CONFIG_NET_TCP_INIT_RETRANSMISSION_TIMEOUT`) runs well past 90 s. That
suspect stays here rather than in an issue of its own: it is this issue's
remaining explanation, and this issue's acceptance is what would confirm it.

Next: on the next hung a53 boot, the gdb dump now separates the two shapes,
because an open that gives up prints the budget line. No budget line and a
thread blocked in `zsock_sendto` / `net_context_send` would confirm the
suspect.

The TCP CONNECT step was bounded by the stack rather than the budget on
several platforms (a SYN-dropping peer). [Issue
2437](archived/2437-zenoh-tcp-connect-unbounded-by-connect-budget.md) fixed
that on the BSD-socket ports, Zephyr's included (zenoh-pico `6827fe1f`); ThreadX
and bare-metal smoltcp remain as [issue
2438](2438-threadx-smoltcp-tcp-connect-ignores-connect-budget.md). So on
Zephyr the whole open, connect and handshake, now ends within the budget, and
2389's remaining suspect is after the open, not in it.
