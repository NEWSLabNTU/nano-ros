# Phase 564 — XRCE on the device: a session that stays responsive and recoverable

**Status (2026-11-15). IN PROGRESS: W1 and W2 done; W3 code done, its porting page pending; W4 open.**

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
| W3 | [2579](../issues/2579-serial-only-baremetal-port-quiet-gaps.md) — a serial-only bare-metal port meets three quiet gaps | #1049 |
| W4 | [2576](../issues/2576-xrce-agent-restart-leaves-node-silently-dead.md) — after an Agent restart the node stays silently dead | #1084 |

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

- **Status:** BLOCKED (the porting page, with the book owner). The code for
  all three gaps is done (2026-11-15); issue 2579 and GitHub #1049 close
  when the page lands.

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
owner as a brief.

### W4 — A lost session is visible and can be re-opened

- **Status:** OPEN. Needs a design ruling before code: the shape of the
  liveness signal.

After an Agent restart the node keeps spinning, publishes fail, no callback
fires and nothing logs; a new Agent never sees a `CREATE_SESSION`.

Acceptance: measured on main first (kill the Agent mid-run, start a new one,
watch the inbound counter); then a session-loss signal a board or node can act
on, and a re-open path, proven by the same rig recovering without a reboot.
