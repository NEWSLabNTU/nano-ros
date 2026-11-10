---
id: 2389
title: "A Zephyr image whose network never answers stays silent past its connect budget instead of failing"
status: resolved
type: bug
severity: medium
area: [zephyr, rmw-zenoh, testing]
related: [phase-466, 2384, 2207]
found: 2026-11-10
resolved: 2026-11-11
resolved_in: "zenoh-pico fork f84c6be5 (the budget bounds each attempt's handshake) + the budget line in nros-rmw-zenoh; regression cell tests/zephyr_connect_budget.rs"
---

**Home: [phase-466](../../roadmap/phase-466-scheduled-lanes-produce-verdicts.md)**.
Split from [issue 2384](2384-zephyr-a53-smp-slirp-boot-flake.md), whose two
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

## Resolution (2026-11-11)

### Reproduced on native_sim, not on the a53

The flaky original (e1000 frame loss on qemu_cortex_a53 SMP) was not
re-run. The same zenoh-pico client open path was driven deterministically on
`native_sim/native/64` (NSOS host sockets), with the image's budget at
`connect_timeout_ms = 3000`:

| case | before | after |
| --- | --- | --- |
| A: nothing listens on the port (connect refused) | `ConnectionFailed` 3.0 s into the open | same, plus the budget line |
| B: a listener accepts the TCP connect and never sends a byte | `ConnectionFailed` **10.1 s** after the accept | **3.03 s** after the accept |

Neither case HUNG. Case B overran the budget by a whole handshake deadline,
and in both cases the console said only `zpico Session -> ConnectionFailed`,
naming no budget.

The same shape on Linux (`zenoh_integration`'s
`client_open_against_a_silent_listener_fails_within_a_bound`, built with
`NROS_ZENOH_CONNECT_TIMEOUT_MS=3000`): 10.13 s before, 3.06 s after.

### Where it waited

`_z_open_connect_locator` (zenoh-pico `src/net/session.c`) checked the budget
only BETWEEN attempts (`_z_backoff_sleep`). Inside one attempt the TCP
connect is bounded by the stack (`CONFIG_NET_SOCKETS_CONNECT_TIMEOUT`, 3000 ms
on Zephyr's own IP stack) and the handshake read by
`_z_unicast_handshake_open`'s own deadline, `Z_TRANSPORT_CONNECT_TIMEOUT`
(10 s), which ignored the budget. The read itself does time out on Zephyr:
`SO_RCVTIMEO` is honoured because `CONFIG_NROS_RMW_ZENOH` selects
`CONFIG_NET_CONTEXT_RCVTIMEO` (issue 2105), and `_z_link_recv_t_msg` checks
the deadline after each receive timeout.

### Fix

- zenoh-pico fork, `nano-ros-1.10.1` branch, commit `f84c6be5`: a positive
  budget becomes an instant passed down `_z_open_inner` -> `_z_new_transport`
  -> `_z_unicast_open_client`/`_open_peer` -> `_z_unicast_handshake_open`,
  whose receive deadline is the earlier of its own and the budget's. Budget
  `0` (one attempt) and `-1` (`connect_forever`) are unchanged.
- `nros-rmw-zenoh` (`zpico.rs`, `report_open_budget_spent`): a failed open
  prints `zenoh open: no session with <locator> within connect_timeout_ms=<N>
  (refused, unreachable, or no answer to the handshake)`.

### What this does NOT explain

The a53 boot printed nothing for 80 s. Under the old code a 10 s budget plus
one attempt is at most about 23 s (3 s connect + 10 s handshake past the
budget), and every wait in it is bounded, so this fix shortens the a53's open
but cannot by itself account for 80 s of silence. Candidates the code reading
left open, none measured:

- the open SUCCEEDED over the lossy link and a later blocking `send` stalled:
  Zephyr's `zsock_sendto` waits with `SO_SNDTIMEO` unset (`K_FOREVER`) while
  TCP retransmits, and Zephyr's retransmission budget
  (`CONFIG_NET_TCP_RETRY_COUNT` with exponential backoff) runs well past 90 s;
- the failure line was printed and lost (the console path on SMP was issue
  2384's class B, fixed by `CONFIG_NROS_CONSOLE_WHOLE_LINES`).

The acceptance's "on a boot where the e1000 is dropping frames" was not
re-measured. If the a53 shape recurs, the gdb dump will now distinguish a
send stalled after the session opened from an open, because an open that
gives up says so with the budget.

### Sibling sweep (the zenoh-pico open on each platform)

The handshake bound is shared code, so it holds wherever the receive returns
on its timeout. The connect step is per platform:

| platform | TCP | handshake read times out? | connect bounded? |
| --- | --- | --- | --- |
| Zephyr native IP | `tcp_zephyr.c` | yes, `NET_CONTEXT_RCVTIMEO` selected | yes, `CONFIG_NET_SOCKETS_CONNECT_TIMEOUT` (3000) |
| Zephyr native_sim | `tcp_zephyr.c` over NSOS | yes, NSOS honours `SO_RCVTIMEO` | refused is immediate; a SYN-dropping peer waits on the HOST's connect (Linux about 127 s), not the budget |
| POSIX / NuttX | `tcp_posix.c` | yes, `SO_RCVTIMEO` | refused immediate; SYN-drop is the OS connect timeout, not the budget |
| FreeRTOS lwIP | `tcp_lwip.c` | yes, `SO_RCVTIMEO` (and `SO_SNDTIMEO`) | yes, non-blocking connect + `select` |
| ThreadX NetX BSD | `nros-platform-threadx/src/net.c` | yes, `SO_RCVTIMEO` | no: `nx_bsd_connect` blocks with `NX_WAIT_FOREVER` until NetX's own SYN retries end |
| bare-metal smoltcp | `nros-smoltcp` | yes, `SOCKET_TIMEOUT_MS` in `read_exact` | yes, `timeout_ms` / `CONNECT_TIMEOUT_MS` (30000) |

So a peer that drops SYNs is still bounded by the stack, not the budget, on
native_sim, POSIX, NuttX and ThreadX. Not fixed here: it is per-platform
connect code (a non-blocking connect with the remaining budget), and nothing
in the issue measured it.

Other transports: XRCE's session create (`uxr_create_session_retries`,
`XRCE_SESSION_CREATION_RETRIES = 3`, each waiting
`UXR_CONFIG_MIN_SESSION_CONNECTION_INTERVAL`) is bounded. Cyclone has no
connect step: `dds_create_participant` does not wait for a peer.

### Regression cell

`(ZephyrNativeSim, Rust, Zenoh, ConnectBudget, Example)`, consumed by
`tests/zephyr_connect_budget.rs`, fixture row
`zephyr-rust-talker-connect-budget-zenoh` (the Rust talker's
`[image.zephyr_connect_budget]`). Both cases assert the budget line names the
locator and `connect_timeout_ms=3000`, the open ends in `ConnectionFailed`,
and the time is at least the budget and at most the budget plus 1.5 s
(case B from the accept; case A from the spawn, plus NSOS's 2 s network
wait). Measured: after, both pass (5.4 s and 5.2 s per case, boot
included); with the fixture rebuilt against the pre-fix pin `cf228437` the
silent-listener case fails, `gave up 10.509s after the TCP connect`.
