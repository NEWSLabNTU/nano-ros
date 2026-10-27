---
id: 2105
title: "On a Zephyr native-IP zenoh image the executor's socket receive never times out, so the lease and keep-alive run only when the router speaks"
status: resolved
type: bug
area: [zephyr, zenoh]
severity: high
found: 2026-10-27
resolved: 2026-10-27
resolved_in: "fix/2105-zephyr-timer-stall (phase-508)"
related: [rfc-0110, phase-508, 2094, 1636, 0129]
---

## Symptom

First seen as the RFC-0110 stack sampler stalling. On
`build-cortex-m-c-talker-zenoh` (C talker, `mps2_an385`, QEMU, SLIRP), the
sampler future took one sample at +4 s, returned `wake_up_after(1000)`, and
was never polled again.

## Measured (gdb on QEMU's gdbstub, 100 s run)

| | before | after |
| --- | --- | --- |
| `_zp_unicast_read_task_fn` polls | 8 | 835 |
| keep-alive future runs | +4 s, +64 s | every ~23 s |
| lease future runs | +4 s, +64 s | +4 s, +69 s |
| router sessions | **2** (a re-dial at ~60 s) | 1 |

The talker kept publishing throughout. That is why the pub/sub cell stayed
green: it stops at the first publish, long before any lease. A talker's own
traffic would also hide a keep-alive that never runs, and here the session
re-dialled anyway.

## Root cause

zenoh-pico 1.10's executor thread (RFC-0110 D2) checks its timers only when
its socket receive returns. The receive therefore has to time out
(`Z_CONFIG_SOCKET_TIMEOUT`, 100 ms on Zephyr).

- Zephyr 3.7 implements `SO_RCVTIMEO` only under
  `CONFIG_NET_CONTEXT_RCVTIMEO`. Otherwise the `case SO_RCVTIMEO` in
  `subsys/net/lib/sockets/sockets.c` is skipped.
- zenoh-pico's `tcp_zephyr.c` logs the refusal and carries on ("Zephyr may
  reject this option depending on the network stack configuration").
- Nothing in nano-ros enabled the option, so the receive blocked until the
  router spoke: its 30 s keep-alive, or a reply.

Under zenoh-pico 1.8 a separate lease thread made this harmless, which makes
it a D2 regression. It affects every Zephyr image with the native IP stack,
including real boards, not only this emulator.

native_sim never showed it. Its sockets are offloaded to the host (NSOS),
which honours the timeout without the option: a quiet native_sim listener
held one session for 150 s. That is why 2094's soak found NuttX and not this.

## Fix

- **`NROS_RMW_ZENOH` selects `NET_CONTEXT_RCVTIMEO`** whenever
  `NETWORKING && NET_SOCKETS`. A serial-only image is untouched.
- **`zpico.c` refuses a THREAD-mode Zephyr build with sockets and without
  the option.** The `#error` names this issue. Negative control: with the
  `select` removed, the Cortex-M talker build stops at that `#error`.
- **Sweep.** Zephyr's UDP link uses the same option, so this covers it.
  FreeRTOS (lwIP) and NuttX socket timeouts are proven by
  `test_rtos_quiet_link_keeps_its_session`. ThreadX, bare-metal smoltcp and
  serial images run in CALLER mode and do not block.
- **Regression cell:**
  `zephyr_cortex_m_qemu::zephyr_cortex_m_talker_keeps_one_session_past_the_lease`
  keeps the talker running for 75 s and requires ONE router session. It
  needs its own 120 s nextest timeout.
- **`ROUTER_SESSION_MARKER` / `ROUTER_SESSION_LOG_FILTER` move to
  `nros_tests::output`,** so both session-continuity cells read one
  spelling of the router's text.

## Verified

- The new cell passes.
- The existing Cortex-M cells pass: C, C++ and Rust pub/sub, plus the boot
  report.
- The native_sim zenoh C and Rust example cells pass.
- With `CONFIG_INIT_STACKS`, the sampler now runs to completion: 992 of
  131072 bytes, stable. That gives RFC-0110 its Zephyr stack figure.
