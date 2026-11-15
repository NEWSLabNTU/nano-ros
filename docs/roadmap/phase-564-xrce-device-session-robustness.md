# Phase 564 — XRCE on the device: a session that stays responsive and recoverable

**Status (2026-11-15). IN PROGRESS: W1–W4 done; the book's re-open recipe is with the book owner, then this phase archives.**

The device side of the XRCE backend (`packages/rmw/xrce/nros-rmw-xrce`, the
Micro-XRCE-DDS client it drives, and what a board needs around it) has no
owning phase: phase-519 owns the host-side Agent, not what the device does.
An outside reporter, wicanr2, brought up a bare-metal STM32F427 (RTIC) over
serial XRCE against Agent v2.4.3 and ROS 2 Jazzy, and filed four issues from
it. This phase homes them. Each item is measured on main before it is fixed,
and each GitHub issue is closed, citing the commit, once its fix is on main.

Each item is claimed with `just claim phase-564-wN` before it starts.

| Item | Issue | GitHub |
| --- | --- | --- |
| W1 | [2577](../issues/archived/2577-xrce-reply-token-leaked-on-full-stream.md) — a reply that fails on a full stream leaks its token | #1076 |
| W2 | [2578](../issues/archived/2578-xrce-session-flush-unbounded-under-inbound-traffic.md) — a session flush never returns under steady inbound traffic | #1075 |
| W3 | [2579](../issues/archived/2579-serial-only-baremetal-port-quiet-gaps.md) — a serial-only bare-metal port meets three quiet gaps | #1049 |
| W4 | [2576](../issues/archived/2576-xrce-agent-restart-leaves-node-silently-dead.md) — after an Agent restart the node stays silently dead | #1084 |

### W1 — A reply the session cannot buffer releases its token

- **Status:** DONE (2026-11-15).

`xrce_service_send_response_locked` returned on a failed `uxr_buffer_reply`
(the reliable output stream full) without releasing the reply token it held.
The executor sends each reply once, so after `XRCE_MAX_PENDING_REPLIES` (4)
such failures every `take_request` answered WOULD_BLOCK for the life of the
process.

Acceptance: a test drives more failed sends than there are tokens and every
request is still taken.

Done. The failure branch releases the token; the service drops that one reply
(an idempotent client retry recovers) instead of dying. No other site holds a
token across a failed buffer (sweep: `grep -n 'in_use = true' src/*.c`).

Measured with `tests/reply_token_failed_send.c` (no Agent: a zeroed session
makes every `uxr_buffer_reply` fail, the full-stream branch), 8 rounds:

| tree | rounds taken | tokens left in use |
| --- | --- | --- |
| main | 4 (round 4 refused, WOULD_BLOCK) | 4 |
| fixed | 8 | 0 |

### W2 — Every session run is bounded in total time

- **Status:** DONE (2026-11-15).

The pinned Micro-XRCE-DDS-Client (v3.0.1) implements `uxr_run_session_time`
as `while (!timeout) timeout = !listen_message_reliably(s, ms);`, and any bytes
the transport returns count as received. Under steady inbound traffic every
window starts another, so the call never returns and the executor dispatches
nothing while the traffic lasts. The backend called it at 17 sites.

Acceptance: a flush under traffic that outlasts its window returns within the
window; the XRCE runtime cells stay green against a real Agent.

Done. `internal.h` carries the backend's only two ways to run the session,
both bounded in total:

- `xrce_session_flush(st, ms)` — `uxr_run_session_until_confirm_delivery`:
  push buffered output, wait at most `ms` for the reliable streams to be
  confirmed, return early once they are. Used at every site that has just
  buffered something (creates, requests, replies, samples).
- `xrce_session_pump(st, ms)` — `uxr_run_session_timeout`: service the session
  for `ms`. Used where the backend waits: the executor's drive
  (`xrce_session_drive_io`) and the graph probe's retry.

The reporter's suggested swap was `uxr_run_session_timeout` at every site. It
is bounded, but it always waits out its whole window, so each flush on an idle
link would cost the full 100 ms; the flush sites take
`until_confirm_delivery` instead.

Measured with `tests/flush_bounded_under_traffic.c` (no Agent: a custom
transport returns bytes every 10 ms for 3 s, faster than any window, and
acknowledges nothing):

| tree | `send_response` took | flush window |
| --- | --- | --- |
| main | 3101 ms (until the traffic stopped) | 100 ms |
| fixed | 101 ms | 100 ms |

### W3 — The serial-only bare-metal port's three gaps

- **Status:** DONE (2026-11-15). The code is 03eb529eff; the porting page,
  the book's "A bare-metal app that owns its main loop", is c90242ce57.

1. A custom-transport-only build still compiles `transport_nros_udp.c`, so a
   board with no IP stack must stub nine `nros_platform_udp_*` symbols.
2. `Executor::open` claims the static backing once (`backing::take()`'s latch);
   a retry while the Agent is not up allocates ~87 KB from the heap and panics.
3. A heap in a `NOLOAD` section (CCM) is not zeroed by cortex-m-rt startup, and
   `FreeListHeap` assumes it is.

Acceptance: each is either fixed or documented where a porter reads, with a
measurement of the failure on main first; the porting page names the library
path (install transport, `register()`, `Executor::open_in`, register nodes,
`spin_once`) and `NROS_INTERFACE_SEARCH_PATH`.

1. Fixed. `session.c` reaches the UDP stack only through
   `transport_nros_udp.c` (init, readiness handle, rx notify), and
   `xrce-sources.txt` compiles that TU under a new `ip` condition, or a
   stand-in, `transport_nros_udp_none.c`, under `no_ip`. Both lanes answer
   the tokens. The stand-in refuses `udp://` with NROS_RMW_RET_UNSUPPORTED
   and the session logs that the image was built with `NROS_LINK_IP=0`.
2. Fixed. `open`, `open_sized`, `open_multi` and `open_with_rmw` run through
   `with_default_backing`: on `Err` the static's latch is re-armed
   (`backing::give_back`), or the heap block is freed. An `Err` leaves
   nothing over the backing (each `open*_in` fails before `from_session_in`,
   or drops the executor it built, whose `Drop` clears `wake_ctx` last).
3. Documented, on `FreeListHeap::new` and in the internals implementation
   notes: zero a `NOLOAD` heap before the first allocation.

Measured:

| gap | main | fixed |
| --- | --- | --- |
| 1: undefined `nros_platform_udp_*` in the backend archive, thumbv7m-none-eabi, `NROS_LINK_IP=0` | 9 | 0 (IP build: still 9) |
| 2: `a_failed_open_gives_the_static_back` (3 failed opens, then a take) | fails at attempt 0, the static kept | passes, heap arm included |

The porting page: the library-path sequence, `NROS_INTERFACE_SEARCH_PATH`
(colcon-style `src/` roots, read before `AMENT_PREFIX_PATH`) and the `no_ip`
build, with a snippet checked against `thumbv7m-none-eabi`, went to the book
owner as a brief, and landed as `book/src/guide/platforms/bare-metal-own-main-loop.md`.

### W4 — A lost session is visible and can be re-opened

- **Status:** DONE (2026-11-15). The re-open recipe for the book went to the
  book owner as a brief.

After an Agent restart the node keeps spinning, publishes fail, no callback
fires and nothing logs; a new Agent never sees a `CREATE_SESSION`.

Acceptance: measured on main first (kill the Agent mid-run, start a new one,
watch the inbound counter); then a session-loss signal a board or node can act
on, and a re-open path, proven by the same rig recovering without a reboot.

Measured on main with a host rig (UDP through a submessage-logging proxy;
Agent A killed at 6 s, Agent B started at 9 s): `spin_once` stayed OK, Agent B
saw no CREATE_CLIENT, and Agent B answered every session ping "available, no
session", which the client decodes and `uxr_ping_agent_session` reports as a
timeout. Detail in the issue.

Rulings (user, 2026-11-15), recorded in full in the issue:

1. The signal, all three parts: a third ping answer, SESSION_LOST; a probe of
   a quiet link from `drive_io` itself, latching only on the Agent's own "no
   session"; and WOULD_BLOCK, not MESSAGE_TOO_LARGE, for a full window.
2. Re-open by the application (drop the executor, open again), including
   `open()` giving its default backing back on drop. Transparent in-backend
   recovery is deferred.
3. `XRCE_SESSION_PROBE_MS` defaults to 1000 ms; 0 turns the probe off.

Done:

- `NROS_RMW_RET_SESSION_LOST` / `TransportError::SessionLost`, mapped through
  `nros-rmw-cffi`, `nros-c` (NOT_FOUND, beside the connection failures) and
  `nros-cpp` (TRANSPORT_ERROR; report class 33).
- The backend: a receive tap stamps `last_rx_ms`; `xrce_liveness_tick` after
  every run in `drive_io` latches the loss and probes a silent link; `drive_io`,
  `publish` and `ping_session` then answer SESSION_LOST, and the destroy skips
  the DELETE_CLIENT. `xrce_output_refusal` names a refused write.
- `Executor::session_lost()`, kept apart from the I/O failure tally.
- `OwnedBacking`, the executor's last field: an `alloc` open's default backing
  goes back on drop.

| tree | Agent B sees CREATE_CLIENT | WRITE_DATA to B |
| --- | --- | --- |
| main | never | 0 |
| fixed, backend rig | 1.1 s after B started | 113 in 10 s |
| fixed, `Executor::open` rig | about 1 s after B started | 159 in 11 s |

Tests: `tests/session_lost.c` (no Agent) and
`executor::backing::tests::the_static_comes_back_after_a_failed_open_or_a_drop`.

Two shapes the user accepted (2026-11-15) in place of the proposal's wording:

- `XRCE_SESSION_PROBE_MS` is a compile-time `#define` in `internal.h`
  (override with `-D`), like the backend's other XRCE timeouts, not an env or
  Kconfig knob. Follow-up, if an image needs to tune it: a knob-registry row
  and the two lanes' rungs.
- `spin_once` returns a `SpinOnceResult`, not a `Result`, and already folded
  drive errors into a failure tally, so the signal is the latched
  `Executor::session_lost()` (with `session_io_healthy()` going false), not an
  `Err` from `spin_once`.
