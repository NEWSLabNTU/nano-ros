---
id: 2994
title: "A signal interrupting the TCP connect fails a one-attempt zenoh open on FreeRTOS POSIX"
status: resolved
type: bug
severity: high
area: [zenoh, rmw, freertos, tests]
related: [2950, 2437, 2389, phase-546, phase-571]
found: 2026-11-27
resolved: 2026-11-27
resolved_in: "fix-b46-connect-eintr"
---

## What happened

Release blocker B46. In the tier-2 nightly at `52fcb58126` (run 45868144195,
runner 118), `nros-tests::freertos_posix
freertos_posix_c_entry_delivers_over_zenoh` failed after 0.91 s:

    zenohd: accepting on tcp/127.0.0.1:11130 117 ms after spawn
    [INFO]  [0.001] boot config: domain 221 (env), locator tcp/127.0.0.1:11130 (baked), ...
    [ERROR] [0.002] zenoh open: no session with tcp/127.0.0.1:11130 in one attempt (connect_timeout_ms=0) (refused, unreachable, or no answer to the handshake)
    [ERROR] [0.002] zpico Session -> ConnectionFailed
    nros run: the target exited with code 1 after 0.0 s

The router was listening on the image's baked port before the image booted,
and the image's one connect attempt failed within a millisecond.

## Cause

Three facts, each read in the code:

1. The FreeRTOS POSIX port drives its tick with SIGALRM, installed with
   `sa_flags = 0`, so without `SA_RESTART` (`portable/ThirdParty/GCC/Posix/port.c`).
2. zenoh-pico's `tcp_posix.c` sets `SO_RCVTIMEO` on the socket before it
   connects. Linux never restarts an interrupted `connect()` on a socket with a
   timeout set, whatever `SA_RESTART` says.
3. A one-attempt open (`connect_timeout_ms = 0`) has no open deadline, so the
   shared helper `_z_tcp_connect_within` made a plain blocking `connect()`, and
   returned -1 on `EINTR` as if the peer had refused.

So a tick landing inside the connect failed the open at once. The window is
tiny on an idle host and wider on a loaded one, which is why it showed once,
in a nightly on a busy runner.

What it was not:

- Not a regression from the phase-554 W4 commits: solo at `52fcb58126` the test
  passed 10/10.
- Not a router that accepts TCP and never answers: a router stopped with
  SIGSTOP after it listened makes the image fail only at image time 10.002,
  after the handshake's 10 s bound.
- Reproduced exactly by injecting one `EINTR` into the first `connect()` with
  an `LD_PRELOAD` shim, against a live router: `[0.001] zenoh open: no session
  ... in one attempt`, 2 ms of wall time. The control run delivers in 3.2 s.

## Fix

In the zenoh-pico fork's shared BSD-socket helper (`tcp_connect_within.h`,
used by `tcp_posix.c` and `tcp_zephyr.c`):

- `EINTR` from `connect()` means the attempt is still going on, the same as
  `EINPROGRESS`: the helper waits for writability and reads `SO_ERROR`, on the
  blocking path too.
- `EINTR` from `poll()` waits again for the time left, so a caller's budget
  still bounds the attempt.

This finishes the same attempt, so phase 571's ruling (a host board makes one
connect attempt) is unchanged.

`accept()` retries on `EINTR` in both ports. `recv()` deliberately does not:
its socket has `SO_RCVTIMEO`, and a tick interrupts a blocked `recv` every
tick, so retrying would restart the timeout each time and a read with nothing
to read would never time out. `EINTR` there still reads as "no data yet".

The same PR carries issue 2950's fork commit (a bound on a one-attempt
connect), which the fork's branch has under this fix.

## Test

`just check zpico-connect-eintr` (`tests/zpico-connect-eintr-tests.sh`)
compiles the fork's helper with `connect()` and `poll()` wrapped so that the
first call reports `EINTR` while the real call runs, and connects to a
loopback listener. It covers the blocking path, the bounded path, an
interrupted `poll`, and both together, plus two refused-port controls. 7/7
pass with the fix. On the previous helper, 4 of the 7 fail with "returned -1,
errno 4 (Interrupted system call)", and the refused-port controls pass on both.
