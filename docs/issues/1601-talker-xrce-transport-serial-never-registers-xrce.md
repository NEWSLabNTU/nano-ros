---
id: 1601
title: "`mps2-an385-baremetal/rust/talker-xrce` registers no RMW backend — its `transport = \"serial\"` no longer matches the board's `Some(\"xrce\")` gate"
status: open
type: bug
area: [examples, boards]
severity: medium
found: 2026-10-01
related: [0189, 1265, phase-244, phase-445, rfc-0098]
---

## What

`test_qemu_xrce_pubsub_e2e` (`packages/testing/nros-tests/tests/emulator.rs`)
fails 3/3 on a freshly built fixture, and the image says why:

```
Executor::open failed: Transport(InvalidConfig)
[ERROR] nros: cannot select an RMW backend — no RMW backend is registered
Bare-metal XRCE QEMU: published=0
```

## Why

The XRCE backend on this board is registered by
`nros-board-mps2-an385`'s `BoardEntry::setup_transport` (`src/entry.rs`), and
only inside

```rust
#[cfg(feature = "xrce-transport")]
if deploy.transport == Some("xrce") { ... nros_rmw_xrce_cffi::register() ... }
```

(issue 0189: bare metal runs no `.init_array`, so this explicit call is the ONLY
registration). phase-445 W6 (commit "the app rung gets a spelling, so the leaf
`[env]` has somewhere to go", 2026-09-11) changed the leaf's `system.toml` from
`transport = "xrce"` to `transport = "serial"` and started VALIDATING the key
against the three link kinds, calling `"xrce"` "the RMW name in the slot that
names the LINK". The board's gate was not moved with it, so since then the
image enables `xrce-transport` and never reaches the call.

Both halves are on `origin/main` as written; nothing about the leaf's
`Cargo.toml` is involved (found while issue 1265 rebuilt every
`mps2-an385-baremetal` Rust row — the other two pub/sub e2e tests on the same
fixture build, `test_qemu_bsp_pubsub_e2e` and `test_qemu_serial_pubsub_e2e`,
pass).

## Fix direction

The board should key on what the image declares that is actually about the
backend — the image's RMW (`[system] rmw = "xrce"`) plus a serial link — not on
the link-kind string standing in for both. Acceptance: the e2e test above
passes, and a gate or test that boots the image fails if registration is
skipped again (today the only witness is a QEMU e2e no merge lane runs).
