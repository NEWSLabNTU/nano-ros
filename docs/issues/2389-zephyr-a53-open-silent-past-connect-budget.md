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
and bare-metal smoltcp followed in [issue
2438](archived/2438-threadx-smoltcp-tcp-connect-ignores-connect-budget.md)
(`nros_platform_tcp_open_within`). So on
Zephyr the whole open, connect and handshake, now ends within the budget, and
2389's remaining suspect is after the open, not in it.

## Post-open send stall: reproduced and bounded (2026-11-11)

The suspect above is real, and worse than suspected: a client write to a
router that keeps the connection open and stops reading blocked FOREVER, on
every BSD-socket port, and took the whole session with it, silently. It is
fixed. The issue stays OPEN because the a53 boot itself was not re-run, so it
is not proven that this is what that boot hit; see "What is left".

### Reproduction

A host TCP proxy between the image and a real `rmw_zenohd` relays both ways
through the handshake, then stops reading from the image while keeping both
sockets open. The image publishes `RELIABLE` (congestion control BLOCK) in a
tight loop with a subscriber on the router, so the write filter passes the
puts.

- **native_sim** (`nros-bench/stress-zenoh-zephyr`, NSOS, 1 KiB puts): the
  proxy stopped reading 8 s after the accept; the image printed nothing more
  for the rest of a 60 s run and was still alive. `gdb` (the image started
  under it, SIGINT at the end):
  - the tx-flush thread: `_zpico_tx_flush_task_fn` -> `zp_batch_flush` ->
    `_z_transport_tx_send_finalized_locked` (TX link mutex held) ->
    `_z_link_send_wbuf` -> `zsock_sendto` -> `nsos_sendto` ->
    `nsos_poll_if_blocking` -> `k_poll` with no timeout;
  - zenoh-pico's executor thread (read, lease and keep-alive futures):
    `_zp_unicast_read_task_fn` -> `_z_tcp_read` -> `zsock_recvfrom` ->
    `k_mutex_lock` on the socket's fdtable lock, which the blocked send holds,
    so not even a receive ran;
  - `main`: `zpico_publish` -> `z_publisher_put` -> `_z_send_n_msg` ->
    `_z_session_transport_mutex_lock`, waiting on the flusher.
- **Linux** (zenoh-pico built standalone from the fork, `tcp_posix.c`, same
  proxy): the publisher thread sat in `send()` under
  `_z_transport_tx_send_finalized_locked` for the whole 90 s run. The read task
  still ran (Linux does not serialise send and receive per fd), but the router
  hung up at 16 s (its lease) and the client's lease task could not close the
  session: the close takes the same transport mutex.

So a stalled write is never reported: no lease expiry, no error, no line.
Zephyr waits `K_FOREVER` in a send when `SO_SNDTIMEO` is unset, and
zenoh-pico's `tcp_zephyr.c` and `tcp_posix.c` set only `SO_RCVTIMEO`.

### Fix

- zenoh-pico fork (`nano-ros-1.10.1`): the BSD-socket write
  (`tcp_posix.c`, `tcp_zephyr.c`) goes through one shared helper,
  `src/link/transport/tcp/tcp_write_within.h`: a non-blocking `send`, and a
  wait for room bounded by one `Z_CONFIG_SOCKET_TIMEOUT` (100 ms on Zephyr, so
  the per-fd lock is released between attempts and the read task runs).
  Progress resets the clock; when the socket takes no byte for
  `Z_TRANSPORT_TX_WAIT_BEFORE_CLOSE` (new, default 5000 ms, the same rule and
  number as zenoh's `congestion_control.block.wait_before_close`) the write
  fails. It never returns a partial count. `tcp_lwip.c` (FreeRTOS) already
  bounded each select at the socket timeout but failed on the FIRST empty one,
  100 ms into a full window; it now applies the same stall bound.
- A failed or short write on a stream link may leave the peer's byte stream
  mid-frame, so the link is marked broken (`_z_link_t::_tx_broken`) and every
  later write on it is refused; zenoh-pico's unicast read task sees the mark
  and closes the session through the same path a lease expiry takes
  (reconnecting where `Z_FEATURE_AUTO_RECONNECT` is built in). The session
  counts these closes (`zp_session_link_tx_failures`).
- The zpico shim prints `zpico: session closed: a write to the router took
  no byte for 5000 ms (the router stopped reading) or the link broke
  mid-write; reconnecting (close #N)`, once per close, from whichever of the
  four `z_publisher_put` failure paths, the spin and the tx-flush task sees
  the count move. (The first run of the Linux test failed with the report on
  `zpico_publish` only: the Rust talker publishes through
  `zpico_publish_with_attachment`, so its puts failed for 52 s with no line.)
- ThreadX and bare-metal smoltcp reach TCP through the platform ABI. Its
  `nros_platform_tcp_send` contract is now: wait at most about the socket's
  timeout, return the bytes taken (possibly fewer, `0` for none), error only
  for a broken connection. The POSIX, Zephyr, FreeRTOS and ThreadX C ports set
  `SO_SNDTIMEO` from the socket's timeout and report a timed-out send as `0`;
  smoltcp returns what it took instead of `usize::MAX` after 100 ms without
  progress. The zenoh alias (`platform_aliases.c`'s `_z_tcp_write`) loops on
  it with the same `Z_TRANSPORT_TX_WAIT_BEFORE_CLOSE`, read from the generated
  zenoh config header, never a copy.

### Measured

| case | before | after |
| --- | --- | --- |
| native_sim, proxy stops reading | silent for the rest of the run (60 s), still alive | the close line 13.9 s after the proxy stopped (socket buffers filling, then the 5 s bound), then a reconnect attempt |
| Linux, zenoh-pico standalone | blocked in `send()` 90 s | `put` fails 5.1 s after the last byte moved; `link_tx_failures` 1; reconnected, stalled again, closed again |
| `tests/zenoh_send_stall.rs` (Linux, Rust `stress-zenoh` talker, real router) | not run on the pre-fix pin (the standalone Linux row above is that shape) | the line 5.18 s after the proxy stopped reading |
| `nros_platform_tcp_send` (POSIX C port) to a peer that never reads | blocked until the test's 60 s timeout | each call returns within the 200 ms socket timeout, `0` once the buffers are full |

Regression tests: `tests/zenoh_send_stall.rs` (Linux, the `stress-zenoh`
fixture row behind the same stop-reading proxy against a real router: the line
must come no sooner than the 5 s bound after the proxy stops and within 60 s; measured 5.18 s),
and `c_port_posix_net`'s
`tcp_send_to_a_peer_that_stops_reading_returns_within_its_timeout`.

A side effect seen on native_sim: NSOS's blocking send paid a full poll round
per call, so the stress bench moved 56 KB in its first 8 s before the fix and
about 50 MB after, with the same image.

### What is left

Whether this is what the a53 boot hit. It fits the shape: the core-pin marker
is printed after the session opens and the node declares its entities, so a
declaration whose send stalled under the e1000's frame loss would leave exactly
"boot banner, network lines, nothing", and Zephyr's own TCP retransmission
budget runs past the 90 s cell timeout. But no a53 run has been made with the
fix, and the hung boot left no backtrace. With every wait in the open and
after it now bounded and reported, the next hung a53 boot prints either the
budget line or the close line, or else the cause is elsewhere. Acceptance is
unchanged.
