---
id: 194
title: "threadx-linux rust rtos e2e: boots, 0 messages delivered (pubsub/service/action)"
status: resolved
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

## RESOLVED — 2026-07-15: three stacked defects, none in the runtime publish path

1. **Museum fixtures masked a retired binary shape.** The harness builders
   (`threadx_linux.rs::build_rust_example`) probed
   `<role>/target-zenoh/…/threadx-linux-<role>` — a binary the role crates
   (lib-only Component pkgs since 212.L) can no longer produce. May-30
   pre-212.L museum binaries satisfied the probe and even DELIVERED (the
   "0 received" run's listener log showed 10 old-marker `Received [N]:` lines —
   pre-phase-277 markers, so the `I heard:` grep counted 0). Repointed the six
   builders at the sibling `<role>-entry` images (the freertos #181 repair,
   never applied here), with the matching `Application setup complete`
   readiness markers in all three rtos_e2e gates.
2. **NoBackend → `Executor::open` `Transport(ConnectionFailed)` with ZERO wire
   I/O** (strace: no `socket()` at all). `nros-board-threadx-linux` never got
   the #131 `rmw-zenoh = ["nros-board-threadx/rmw-zenoh"]` feature forwarding
   its riscv64 sibling has, so the family board's boot-path
   `nros_rmw_zenoh::register()` was compiled out of every entry image. Added
   the forwarding + enabled the feature on all six entry pkgs' board dep.
3. **stdout block-buffering hid the readiness banner.** The Rust entry's
   `nros::main!` `fn main` overrides the board `startup.c` weak C `main` that
   carried the `setvbuf(_IOLBF)` fix, so a piped harness saw NOTHING within
   the 30 s gate. Added `line_buffer_stdout()` to the board's
   `run`/`run_with_deploy` before any output.

Also baked the per-variant router ports the harness computes
(service 7465 / action 7475; pubsub keeps 7455) into the entry
`[package.metadata.nros.deploy.threadx-linux].locator` blocks — all six
previously baked the pubsub port.

Verified: `rtos_e2e` ThreadxLinux Rust pubsub + service + action all PASS
(3/3, fresh entry fixtures; manual piped smoke shows live `Publishing:` lines).
#191 (freertos rust) should be re-triaged against causes 1–3 — same family.
