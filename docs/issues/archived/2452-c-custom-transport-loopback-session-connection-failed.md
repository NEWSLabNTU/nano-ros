---
id: 2452
title: "The C custom-transport loopback example exits 1: its session over `custom://loopback` answers ConnectionFailed"
status: resolved
type: bug
severity: low
area: [examples, rmw-zenoh, testing]
related: [phase-535, 2096, 2455]
found: 2026-11-12
resolved: 2026-11-12
resolved_in: "a custom/ locator compiles zenoh-pico's custom link on every road; zpico_close drops the session"
---

> **Resolved (2026-11-12).** Three defects stood in a row, each hiding the
> next. The example now carries its bytes over a TCP connection it opens to a
> router, exits 0 with all four callbacks fired, and its row is a runtime row:
> the `CustomLink` cell, run by `linux_example_custom_link_e2e`.

## Measured

`examples/linux/c/custom-transport-loopback`, linux/zenoh, before the fix:

```
loopback: transport installed
[INFO] nros: boot config: domain 0 (default), locator custom://loopback (baked), rmw zenoh (baked)
[ERROR] nros: zpico Session -> ConnectionFailed (the two are indistinguishable downstream)
[nros] .../src/main.c:217 rclc_support_init_with_locator(&app.support, "custom://loopback", 0) -> -4
```

Exit 1, in ~0.1 ms, from both the pre-migration leaf and the migrated
one-package workspace. Under gdb, `loopback_open` was never reached.

## Cause

1. **No C/C++ image compiled zenoh-pico's custom link.** The link
   (`Z_FEATURE_LINK_CUSTOM`, `src/link/unicast/custom.c`) was reachable only
   through `zpico-sys`'s `link-custom` cargo feature. A Rust image names it in
   its own manifest (the Rust custom-transport pair does: that is half of why
   it passes); a C/C++ image's Rust staticlib is a crate CMake generates, with
   no manifest a user can name a feature in. So every C/C++ zenoh image built
   `Z_FEATURE_LINK_CUSTOM 0` and `nm` showed no `_z_new_link_custom`. The
   session refused the scheme while `rclc_set_custom_transport` answered OK.
   The locator was also spelled `custom://loopback`, which zenoh parses as a
   scheme named `custom:`. `custom://` is XRCE's spelling; zenoh's is
   `custom/<label>` (the Rust pair uses `custom/loopback`).
2. **The ring buffer could not answer the handshake.** With the link built
   in, `open`, `write` and `read` all ran, and the session still refused: the
   ring echoed the image's own `InitSyn` back where zenoh-pico waits for an
   `InitAck`. A peerless loopback cannot open a zenoh session. That is the
   other half of why the Rust pair passes: it bridges to a real router.
3. **A closed session never closed its link.** With a router on the far end
   the session opened and published, and `close` counted 0. `zpico_close`
   called `z_close` and never `z_session_drop`. Only the drop clears the
   transport, so no zenoh session's link was ever closed on teardown (a TCP
   link's socket fd stayed open too) and the session `z_open` allocated was
   never freed.

Fixing (1) also met a fourth: a custom link drops the IP links
(`NROS_LINK_IP=0`, as `serial` does), and on POSIX zenoh-pico did not
compile without them. `tcp_posix.c` and `udp_posix.c` were guarded on the
socket family alone, while `unix.h` declares `_iptcp` only when an IP link is
on. So every Linux image on a non-IP locator failed to build.

## Fix

- `nros_board_common::policy::LINK_BUILD_KNOBS` gains `custom/…` →
  `NROS_LINK_CUSTOM` (issue 2096's mechanism for CAN and ISO-TP). An image's
  locator on the scheme implies the knob on every road, and zpico-sys reads it
  beside the cargo feature. `nros-rmw-zenoh` always links the Rust half the C
  link calls (`zpico-platform-custom`'s `nros_zpico_custom_take`), so the knob
  alone links.
- The zenoh-pico fork guards `tcp_posix.c` / `udp_posix.c` the way their lwIP
  twins are already guarded (pin moved to `fb5f5cdc`; filed and resolved as
  issue 2455).
- `zpico_close` (and the read-task-start failure path) drops the session
  instead of only closing it.
- The example states `locator = "custom/loopback"` in its image, opens its
  session on that locator, and bridges the callbacks to a router over TCP
  (`NROS_CUSTOM_TCP_TARGET`). Its `read` waits at least 1 ms, because
  zenoh-pico's receive loop asks with 0 ms.
- Witness: `cell(Linux, C, Zenoh, CustomLink, Example, Runtime)`, run by
  `tests/linux_example_custom_link_e2e.rs`. It asserts exit 0 and the counts
  `open 1`, `write ≥ 1`, `read ≥ 1`, `close 1`, plus a publish. It reads them
  through `nros_tests::output::{CUSTOM_LINK_COUNTS_HEADER, custom_link_count}`.
