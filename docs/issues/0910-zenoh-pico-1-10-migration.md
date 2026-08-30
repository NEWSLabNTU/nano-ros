---
id: 910
title: "migrating to zenoh-pico 1.10: the serial layer moved, `config.h` is no
  longer shipped, and our config generator is 54 knobs behind"
status: resolved
type: task
area: rmw, build
related: [issue-0852, issue-0882]
---

## Why

Our pin is 1863 upstream commits behind. The gap matters for serial specifically:
upstream **restructured the link transports** and now ships its own Zephyr UART
backend, so most of our Zephyr serial code has an upstream equivalent that did
not exist when we wrote it.

## What upstream changed

| | our pin (`dac320e3`) | upstream 1.10 |
| --- | --- | --- |
| Zephyr serial | ours, in `src/system/zephyr/network.c` | theirs, `src/link/transport/serial/uart_zephyr.c` |
| serial protocol | `src/system/common/serial.c` (ours) | `src/link/transport/upper/serial_protocol.c` |
| lwIP / sockets | `src/system/freertos/lwip` | `src/link/transport/**`, `src/system/socket` |
| `include/zenoh-pico/config.h` | checked in | **generated** from `config.h.in` by their CMake |
| system ABI (`_z_mutex_*`, `_z_task_*`) | `src/system/zephyr/` | unchanged — nano-ros's shims still fit |

The last row is the good news: `src/system/zephyr/system.c` still exists, so
`nros_zenoh_zephyr_system.c` continues to override the symbols it always did.

## Progress so far

Two branches on the fork (`jerry73204/zenoh-pico`), both off `upstream/main`:

- **`serial-fixes`** — one commit, for the upstream PR. Fixes
  `_z_zephyr_uart_read`, which busy-spins on `uart_poll_in` with no timeout, no
  yield and no `uart_err_check`, and returns `len` regardless. All three are
  defects we hit and fixed on our own port; the third is
  [issue 0852](0852-*), where a silent overrun cost a long investigation that
  ended in a bug report against an innocent peer.
- **`nros-integration`** — `serial-fixes` plus a passthrough
  `include/zenoh-pico/config.h`, which upstream no longer ships. Not for
  upstream. A passthrough suffices because every platform in
  `config/*/nros-platform.toml` defines `ZENOH_GENERIC`; the 48 `@TOKEN@`
  substitutions are all in the dead `#else` branch, which is now an `#error` so
  a platform that forgets the define fails loudly.

Build progress against `nros-integration`, each item found by building and
fixed in turn — all now resolved:

1. `nros-platform.toml` named `system/freertos/lwip`, which no longer exists →
   `system/freertos`.
2. `zenoh-pico/config.h` missing → generated from `config.h.in` using upstream's
   CMake defaults (47 substituted tokens, each `#ifndef`-guarded so a `-D` still
   wins). This replaced an earlier *passthrough* that assumed `ZENOH_GENERIC` is
   always defined. It is not: that symbol comes from the cargo manifest, and the
   Zephyr CMake path compiles without it. The `#error` in the dead branch is what
   caught the wrong assumption.
3. The config generator being 54 knobs behind → `upstream_literal_defines()` in
   `nros-zpico-build` now *derives* the literals from `config.h.in` at build
   time instead of transcribing them, so a version bump costs nothing.
4. `src/runtime/*.c` was not globbed. 1.10 added the background executor that
   `z_open`, the read/lease tasks and session close all call into.
5. Socket-free build. Three upstream files chose their implementation from the
   PLATFORM alone, so a serial-only Zephyr image (`CONFIG_NETWORKING=n`) took the
   socket path and failed on `AF_INET6` / `socklen_t` / `struct sockaddr`. Two of
   them already had a not-available stub that was simply unreachable. Fixed on
   `serial-fixes` with a shared `Z_HAS_SOCKET_LINK`.
6. `Z_FEATURE_UNICAST_PEER=0` — nano-ros is a CLIENT to a router. The peer path
   is what accepts inbound transports and what reaches for socket primitives a
   serial image cannot provide.
7. Platform ABI additions: `zp_clock_elapsed_{us,ms,s}_since` and
   `_z_task_get_id` / `_z_task_current_id` / `_z_task_id_equal`.
8. `zpico_send_keep_alive`: `zp_send_keep_alive` is only declared when
   `Z_FEATURE_MULTI_THREAD == 0`. With the lease task running it owns the
   keep-alive schedule, so the threaded build probes `z_session_is_closed`
   instead. That is the weaker check — it reports link death the lease task has
   already noticed rather than testing the link now.

## The two bugs that actually cost the time

Neither was a 1.10 regression. Both were latent in our Zephyr shim; 1.10's
background executor is what made them bite.

**`_z_condvar_init` passed an uninitialised `pthread_condattr_t`.** Zephyr's
`pthread_condattr_init` returns `EINVAL` if the attribute already looks
initialised, and that flag is a bit inside the caller's object — so on a stack
variable it reads whatever the previous frame left. When the garbage had the bit
set, `_z_session_init` returned `-1` and the session never opened. Which way the
bit fell depended on code layout, so adding an unrelated `printk` turned a dead
board into a clean boot. That is what made it read as flaky rather than broken.
Any `printk` in the failing path perturbed it, so it was finally pinned by
recording failures into a word and reading that word over SWD.

**`elapsed_ns` accumulated nanoseconds into a 32-bit `unsigned long`**, wrapping
at 4.295 s. The executor schedules wake-ups as ms since an epoch and compares
deadlines with these helpers, so past ~4.29 s the comparison inverted and the
lease future fired early:

    [4.124000 INFO ::_zp_unicast_lease_task_fn] Closing session because it has
                                                expired after 10000ms

a 10 s lease expiring at 4.1 s on a working link, forever.

## Not carried upstream

The INIT-retry scoping (fork commit `67ee0224`) is **not** needed: upstream's
`_z_connect_serial` sends one INIT per iteration and only loops on RESET, so it
never had the flood. That commit fixed a regression of ours.

`src/system/zephyr/network.c` is no longer compiled. 1.10 left the file in the
tree but it is dead — it still reaches for the retired `_fd` socket member and
does not compile against the redesigned socket type. Its replacements under
`src/link/transport/` are picked up by the existing `src/link/*.c` glob.

`src/system/zephyr/isotp.c` (RFC-0083) does **not** exist upstream and has not
been ported to the new layout. `CONFIG_NROS_ZENOH_LINK_ISOTP` now fails loudly
rather than silently dropping the link.

## State

Done, and measured on hardware (`mr_canhubk3/s32k344`, serial to `rmw_zenohd`,
domain 10):

    ros2 node list   -> /talker
    ros2 topic list  -> /chatter
    ros2 topic hz    -> average rate: 2.000  (min 0.464s max 0.519s)

one session, no reconnects. That rate matches the pre-migration baseline.
Serial-only image: FLASH 326560 B (7.88%), RAM 285376 B (87.09%) with the nros
task stacks relocated to DTCM.
