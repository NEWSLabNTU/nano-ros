---
id: 194
title: "threadx-linux rust rtos e2e: boots, 0 messages delivered (pubsub/service/action)"
status: open
type: bug
area: threadx-linux
related: [issue-0191, phase-287]
---

## Summary

The `rtos_e2e` ThreadxLinux RUST lanes fail deterministically with zero
delivery (3/3 nextest retries, solo run — not the full-sweep load flake that
hit the nuttx C/C++ lanes):

```
rtos_e2e test_rtos_{pubsub,service,action}_e2e::…ThreadxLinux::lang_1_Lang__Rust
  threadx_linux rust pubsub E2E failed — 0 messages received
```

Observed 2026-07-13 during the phase-287 W7 full-matrix sweep, on freshly
rebuilt fixtures (`just build-test-fixtures` green the same session). The
C and C++ ThreadxLinux lanes pass; only RUST delivers nothing.

## Reading

Sibling of #191 (freertos rust `*-entry` images: session opens, nothing
publishes). Same language axis, different platform/net plan (threadx-linux =
NSOS/POSIX loopback, no QEMU/slirp), so the shared suspect is the RUST entry
runtime publish path rather than the network plan. Whoever picks this up
should diff against the #191 findings first — a single root cause in the
rust entry runtime would close both.

## Repro

```sh
just build-test-fixtures
cargo nextest run -E 'test(test_rtos_pubsub_e2e::platform_3_Platform__ThreadxLinux::lang_1_Lang__Rust)'
```
