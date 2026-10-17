---
id: 1934
title: "The zenoh-pico custom link's handshake read gives up at the first empty read, so a custom-transport session fails to open whenever the router answers late"
status: resolved
type: bug
area: [rmw, zenoh, transport, testing]
severity: medium
found: 2026-10-17
related: [1932]
---

## Resolution (2026-10-17)

Fork commit `3248995b` pushed to zenoh-pico's `nano-ros` branch and pinned by PR #2324. Verified on main: run-matrix 38980380118 (`workflow_dispatch`, main `be26a55b`), `tier 1 (cells)` on newslab-server-241 — the busy runner it failed on — passed, including `custom_transport_loopback`.

## Measured

Run-matrix **38933437082** (`workflow_dispatch` on main `c43a90314`), job
`tier 1 (cells)` on `newslab-server-241`, step `just ci`, the
`=== Required-features fixture tests ===` block:
`nros-tests::custom_transport_loopback test_custom_transport_loopback`
FAILED in 6.65 s. That lane runs with `--failure-output never`, so the CI log
holds no output. The earlier tier-1 runs (38861323075, 38818350050) passed it,
and tier 2 of the same run passed it in 0.7 s.

Reproduced in the box tree on the CI's fixtures (zenoh-pico at the pin,
`492c74f0`, whose `custom.c` matches the runner's):

- idle host: 5 of 5 pass, ~0.65 s each;
- 8 busy loops on 4 cores: 1 of 8 FAILED, with the listener's output
  ```
  [ERROR] nros: zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
  [ERROR] nros: RMW session open failed — ConnectionFailed
  thread 'main' panicked at src/main.rs:54:58:
  Failed to open session: Transport(ConnectionFailed)
  ```
  about 2.7 s after the TCP connect succeeded;
- deterministic: the listener through a TCP proxy that delays each
  router→client chunk by **5 ms** opens **0 of 3** sessions.

## Cause

The session handshake (`_z_link_recv_t_msg`, `src/transport/common/rx.c`)
reads a whole transport message through the link's `read_exact`: the 2-byte
length, then the body. For the custom link that is
`_z_f_link_read_exact_custom` in our fork's `src/link/unicast/custom.c`, which

1. asks the user `read` callback to wait **0 ms**, and
2. stops at the first read that returns 0 bytes.

`nros-transport-callbacks`' `tcp_read` honours the 0 (as `max(1)` = 1 ms), and
so does any implementation that follows the documented contract ("receive up
to `len` bytes within `timeout_ms`"). So the InitAck/OpenAck has to be
**already buffered** when zenoh-pico looks. An idle loopback router answers in
well under a millisecond; a loaded host does not, every open attempt fails, and
after zpico's retries the session reports `ConnectionFailed`.

The steady-state poll path (`_z_f_link_read_custom`) is not affected: the
unicast rx loop (`src/transport/unicast/rx.c`) already tolerates a partial
message and comes back.

## What this is NOT

- Not the full-duplex mutex deadlock 542dd0258d fixed: the listener's
  read and write both work, and it fails at session OPEN, before any declare.
- Not a new regression on main: `custom.c`'s read path is unchanged since
  4adeee47 (phase 115.F). The 21 commits between the passing and failing runs
  touch nothing on this path; the first two runs simply did not hit it.
- Not issue 1932 (XRCE sessions under a full sweep on the same runner), though
  both are a session handshake that loses under load.

## Fix

Fork commit `3248995b` on zenoh-pico's `nano-ros` branch, pushed
2026-10-17 as a fast-forward of the old pin `492c74f0`; the superproject pin
moves to it in the same change as this section. `_z_f_link_read_exact_custom` now waits for the rest of
the message, passing the user `read` the remaining budget and bounding each
gap by `Z_CONFIG_SOCKET_TIMEOUT` — the bound `SO_RCVTIMEO` gives a TCP link's
`read_exact`, which also loops per recv. A negative return stays fatal. The
poll path keeps its 0 ms ask.

Measured with that commit built into the fixtures (box tree, same host):

| | pin `492c74f0` | with `3248995b` |
| --- | --- | --- |
| through a 5 ms delay proxy | 0 of 3 open | 5 of 5 open |
| 12 busy loops on 4 cores, `custom_transport_loopback` | 20 of 20 pass, 1 of 8 failed at 8 loops earlier | 20 of 20 pass |
| through a 200 ms delay proxy | — | 0 of 3 open |

The 200 ms row is the bound, not a gap: this build's
`Z_CONFIG_SOCKET_TIMEOUT` is 100 ms (`nros-zpico-build`), and the plain TCP
link (`listener`, `NROS_LOCATOR` through the same proxy) answers the same way
— 2 of 2 open at 5 ms, 0 of 2 at 200 ms. The custom link now fails where the
TCP link fails, and no earlier.

## What would close it

Done: the fork commit is pushed and the pin is past it. Still to see:
`custom_transport_loopback` passes through the 5 ms delay proxy and under
load, and in tier 1 on main.
