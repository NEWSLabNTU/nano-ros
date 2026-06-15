---
id: 72
title: safety-e2e CRC dead over zenoh — nros/safety-e2e doesn't reach the backend's safety-e2e
status: open
type: bug
area: build
related: [phase-250]
---

## Symptom

A `safety-e2e` build receives messages and surfaces `IntegrityStatus` with correct
sequence gap/dup tracking, but `crc_valid` is always `None` (`crc=n/a`) — the CRC-32
integrity check never runs over the zenoh transport. Affects both the imperative
`.typed::<M>().safety()` path and the declarative `.safety()` / `ctx.integrity()` path
(phase-250).

## Root cause

The CRC-32 attach (publisher) and validate (subscriber) live behind the **zenoh backend's
own** `safety-e2e` feature, in `nros-rmw-zenoh`:

- `packages/zpico/nros-rmw-zenoh/src/shim/publisher.rs:313` — the 37-byte
  (`RMW_ATTACHMENT_SIZE_WITH_CRC`) attachment with the trailing CRC is `#[cfg(feature =
  "safety-e2e")]`; without it the publisher sends the 33-byte (seq-only) attachment.
- `packages/zpico/nros-rmw-zenoh/src/shim/subscriber.rs:888,1253` — the
  `try_recv_validated` override that recomputes + compares the CRC is the `safety-e2e`
  override; without it the trait default returns `crc_valid: None`.

But `nros/safety-e2e` forwards only to `nros-node`, `nros-rmw`, and `nros-rmw-cffi?`
(`packages/core/nros/Cargo.toml`) — **not** to `nros-rmw-zenoh`. Cargo features do not
propagate "upward", so building with `nros/safety-e2e` leaves the zenoh backend's
`safety-e2e` off: no CRC on the wire, no validation on receive.

## Fixed (immediate)

The hand-written native examples + the phase-250 fixture now enable the backend feature
directly (optional, so it only applies on the rmw-zenoh path):

- `examples/native/rust/talker/Cargo.toml`, `examples/native/rust/listener/Cargo.toml`:
  `safety-e2e = ["nros/safety-e2e", "nros-rmw-zenoh?/safety-e2e"]`.
- `packages/testing/nros-tests/bins/declarative-safety-listener/Cargo.toml`:
  `nros-rmw-zenoh` carries `safety-e2e`.

Verified: `crc=ok` end-to-end (`tests/safety_e2e.rs::test_declarative_safety_listener_receives_integrity`).

## Open — the orchestration lowering (phase-250 Wave 1) has the same gap

The declared `[safety]` axis lowers to `nros/safety-e2e` on the generated entry
(`generated_default_features`, `generate.rs`), but **not** to the backend's `safety-e2e`.
So an orchestration-built `[safety]` system still gets `crc_valid: None`. Completing it is
two paths:

1. **Native / board-less** — thread `plan.safety` into `backend_features()` /
   `render_one_backend()` (`generate.rs:1564,1613`) so the direct `nros-rmw-<x>` dep carries
   `safety-e2e`.
2. **Board-backed (embedded)** — the backend is pulled by the board crate's `rmw-<x>`
   feature; the board crate (e.g. `nros-board-native`) needs a `safety-e2e` passthrough
   feature (`nros-rmw-zenoh?/safety-e2e`) that the generated entry enables when `[safety]`
   is declared. This is the RFC-0031 board-as-RMW-selection-point analog for the safety
   capability.

Until then, orchestration `[safety]` enables the validation *surface* (the
`ctx.integrity()` API, sequence tracking) but not the CRC sub-field over zenoh.

## Also consider

- Other backends (cyclonedds, xrce) — do they carry a `safety-e2e` CRC path? If not, the
  axis silently no-ops the CRC there too; document or gate.
- A build-time guard / lint: if `nros/safety-e2e` is on but no `nros-rmw-*/safety-e2e` is,
  the CRC is silently dead — worth a warning (mirrors the weak-symbol / dep-chain gates).
