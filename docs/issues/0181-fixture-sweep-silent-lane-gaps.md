---
id: 181
title: "build-test-fixtures exits 0 with whole lanes unbuilt (esp32, px4, freertos/threadx-linux rust) — tests then fail 'not prebuilt'"
status: open
type: bug
area: build
related: [issue-0164, phase-287]
---

## Summary

A full `just build-test-fixtures` run (2026-07-12, post phase-287 W6) printed
its lane markers and **exited 0**, yet `test-all` then failed a dozen tests
with `Test fixture binary not prebuilt` / `Binary not found after build`:

- `esp32_emulator` ×5 + `logging_smoke_esp32`: `examples/qemu-esp32-baremetal/
  rust/talker/target/riscv32imc.../esp32-qemu-talker` absent ("Binary not
  found after build" — the esp32 lane's cargo build produced nothing and the
  sweep still exited 0).
- `px4_xrce` ×2: `examples/px4/rust/xrce/px4-stub/...` never built.
- `rtos_e2e` rust lanes (freertos pubsub/service, threadx-linux
  pubsub/service): `examples/qemu-arm-freertos/rust/talker/target-zenoh/...`
  absent — the freertos lane's `build-examples` (rust) half didn't run in the
  sweep even though `build-fixture-extras` (C/C++) did.
- `native/rust/{listener,service-client-callback}` STALE (generated source
  newer than binary) — the native rust example rebuild didn't cover them.

## Why it matters

The sweep is the staleness gate's ground truth: "exit 0" is read as "every
lane fresh", so these tests fail red in `test-all` and look like runtime
bugs. The mtime-treadmill pitfall (CLAUDE.md) makes this recurrent.

## Fix direction

Per-lane build steps must fail loudly (or emit an explicit `[SKIP <lane>:
<reason>]` that the fixture-staleness gate understands) instead of exiting 0
with nothing built. Audit: esp32 lane (espup toolchain probe), px4 lane
(PX4-Autopilot checkout probe), the rust half of freertos/threadx-linux
`build-examples`, and the native rust example set.
