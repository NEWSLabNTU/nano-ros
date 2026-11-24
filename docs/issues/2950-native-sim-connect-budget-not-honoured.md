---
id: 2950
title: "On Zephyr native_sim a zenoh connect to an unreachable address blocks ~136 s, whatever the stated connect budget (measured with connect_timeout_ms=0)"
status: open
type: bug
severity: medium
area: [zephyr, zenoh, network]
related: [2941]
found: 2026-11-24
---

## What happens

A Zephyr native_sim image whose zenoh locator names an address the host cannot
reach blocks in the session open for about 136 seconds, and only then fails.
Its connect budget is `connect_timeout_ms = 0`, the builtin, which means one
attempt that fails at once.

Measured on host 241, 2026-11-24, by the manager. The image was the tier-2
runner's `build-ws-rs-entry-zenoh` `zephyr.exe` (a Rust workspace entry,
built 08:53Z), run by hand with no router for 160 s. Console, with host
seconds:

```text
  0 s  *** Booting Zephyr OS build v3.7.2 *** / Network ready (NSOS — host kernel sockets)
  3 s  nros: heap peak 2960 of 262656 bytes ...
137 s  zenoh open: no session with tcp/10.0.2.2:7430 in one attempt (connect_timeout_ms=0) (refused, unreachable, or no answer to the handshake)
       ... ConnectionFailed, `nros: zephyr entry FAILED: Spin`, kernel panic, exit
```

The image dialled `10.0.2.2` because its bringup stated it (issue 2941) and
the fixture define that normally overrides it was missing (PR #4790). A host
with no route to that address drops the SYN, so a connect waits for the
host's own SYN-retry budget: about 127 s with Linux's `tcp_syn_retries = 6`,
which fits the 136 s seen.

## Why it matters

The connect budget is a stated knob (`[knobs.zenoh.limits] connect_timeout_ms`
/ `connect_forever`). It is supposed to bound how long an open can take, and
its builtin 0 promises that an unreachable router "says so at once". On
native_sim it bounds nothing:
- a test cell that dials a wrong address idles to its own timeout with an
  empty console (the release run-matrix of 2026-11-24 read that as a hang);
- an image that states a budget, say 3 s, still waits the host's ~2 minutes.

A refused port (a router address with nothing listening) is not affected: the
host answers RST at once.

## Where to look

zenoh-pico's `tcp_zephyr.c` opens through `_z_tcp_connect_within`
(`src/link/transport/tcp/tcp_connect_within.h`, a nano-ros patch for issue
2437). It sets `O_NONBLOCK` with `fcntl`, calls `connect`, then `poll`s for
writability bounded by the budget. If `fcntl` FAILS it falls back to a plain
blocking connect, which keeps the host's SYN-retry bound. That fallback is
the most likely path here.

Suspects, not yet told apart (no Zephyr tree on the host that filed this):
1. NSOS (Zephyr 3.7's native_sim offloaded sockets) does not implement
   `F_SETFL O_NONBLOCK`, so the helper falls back.
2. NSOS accepts the flag but its offloaded `connect` still blocks on the host
   socket.
3. NSOS's `poll` does not report a pending connect, so `poll(…, 0)` is not
   what decides.

A `printk` of the `fcntl` and `connect` results and `errno` in
`_z_tcp_connect_within`, on one native_sim image dialling an unroutable
address, separates the three.

## Acceptance

- A native_sim image dialling an unroutable address with `connect_timeout_ms =
  0` fails its open within about a second.
- With a stated budget of N ms, it fails after about N ms.
- Both measured on host time with the guest console.
- The fix holds for the other ports that share `_z_tcp_connect_within`, or a
  port that cannot honour a budget says so at configure time instead of
  silently.
