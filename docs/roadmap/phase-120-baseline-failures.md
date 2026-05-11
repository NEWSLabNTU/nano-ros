# Phase 120 — Pre-Existing Baseline Failures

**Goal:** Drive the 4 pre-existing `just test-all` failures left over from Phase 119 down to zero.
**Status:** 2 fixed (`test_xrce_action_fibonacci`, `test_zephyr_xrce_rust_action_e2e`). 2 remain on ThreadX RISC-V QEMU (transport-timing on the zenoh-pico backend, pre-existing — out of scope for a quick session). Net `just test-all` result: 717/720 passed, 2 hard fails + 1 flaky.
**Priority:** Medium (cleanup; no test is gating a release).
**Depends on:** Phase 119.3.

## Overview

Phase 119 left 4 failures in `just test-all`. They predate phase 119 and aren't regressions, but they keep `test-all` red. This phase audits each, fixes what can be fixed cheaply, and books the rest as follow-up.

## Findings

### Fixed — `test_xrce_action_fibonacci`

Root cause: `NodeError::Transport(TransportError::ServiceRequestFailed)` was returned whenever an action server's `try_recv_request` (or client's `try_recv_reply`) failed *for any reason* — including `TransportError::NoData`, which is the steady-state polling result when no request/reply is pending. The action client's `promise.wait()` interpreted "no reply yet" as a hard failure and bailed out.

Fix: distinguish `NoData` from real errors in three RMW-facing call sites:

- `packages/core/nros-node/src/executor/action_core.rs:140` — `try_recv_goal_request` (action server's send_goal pickup).
- `packages/core/nros-node/src/executor/action_core.rs:295` — `try_handle_cancel` (action server's cancel_goal pickup).
- `packages/core/nros-node/src/executor/action_core.rs:363` — `try_handle_get_result_raw` (action server's get_result pickup).
- `packages/core/nros-node/src/executor/action_core.rs:707` — `try_recv_get_result_reply` (action client's get_result reply).
- `packages/core/nros-node/src/executor/action_core.rs:719` — `try_recv_send_goal_reply` (action client's send_goal reply).
- `packages/core/nros-node/src/executor/handles.rs:1764` — generic `Promise::try_recv` (returned `ServiceRequestFailed` on NoData via `map_err`).

The pattern in every fix is the same `match` shape:

```rust
match handle.try_recv_*(...) {
    Ok(opt) => opt,
    Err(TransportError::NoData) => return Ok(None),
    Err(_) => return Err(NodeError::Transport(TransportError::ServiceRequestFailed)),
}
```

Verified: `cargo nextest run -E 'test(test_xrce_action_fibonacci)'` passes after the fix; failed before.

### Deferred — Zephyr/ThreadX Rust embedded tests

The three remaining failures are not action-protocol bugs:

| Test | Root cause |
|---|---|
| `test_zephyr_xrce_rust_action_e2e` | Zephyr Rust XRCE examples hardcode `"127.0.0.1:2018"` in `examples/zephyr/rust/xrce/*/src/lib.rs`. The test fixture builds with `-DCONFIG_NROS_XRCE_AGENT_PORT=2038` (per-variant Kconfig override applies via `.config`), and the test runs an agent on port 2038. But the Rust source ignores the Kconfig and connects to 2018 → ConnectionFailed. |
| `test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust` | Same hardcoded-port shape on ThreadX Rust action example (separate source tree). |
| `test_threadx_rv64_dds_rust_talker_to_listener_e2e` | Different DDS-side issue; needs separate investigation. |

The fix shape: have the Rust embedded examples read `zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_PORT` and assemble the locator string at runtime (no_std-compatible — use a `heapless::String` buffer or a `const_str`-style concat macro). Same pattern across all Zephyr Rust XRCE talker/listener/service/action examples; ~10 files to touch. Out of scope here.

## Work Items

### 120.1 — Distinguish NoData from real errors in action protocol — **DONE**

- **Files:** `packages/core/nros-node/src/executor/action_core.rs`, `packages/core/nros-node/src/executor/handles.rs`.
- [x] Six `map_err(...)` sites that collapsed all transport errors to `ServiceRequestFailed` now match-on `TransportError::NoData` and return `Ok(None)` for the steady-state polling case.

### 120.2 — Zephyr Rust XRCE locator from Kconfig — **DONE**

- **Files:** `examples/zephyr/rust/xrce/{talker,listener,service-{server,client},action-{server,client}}/src/lib.rs`, matching `Cargo.toml`s.
- [x] Each example's `run()` body now assembles the XRCE locator from `zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR` and `CONFIG_NROS_XRCE_AGENT_PORT` into a `heapless::String<48>`, then passes that to `ExecutorConfig::new(&locator)`.
- [x] Added `heapless = "0.8"` to the five `Cargo.toml` files that didn't already have it (action-server already had it).
- [x] Verified: `test_zephyr_xrce_rust_action_e2e` passes after the fix; was failing with `Transport(ConnectionFailed)` because the agent ran on port 2038 but the binary hardcoded `127.0.0.1:2018`.

### 120.3 — ThreadX RV64 Rust action E2E — **DEFERRED (best-effort attempted)**

`test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`.
Path is the **zenoh-pico** action client/server (not XRCE — separate failure
shape from 120.2). C and C++ on the same QEMU/transport pass; only Rust
manual-poll fails.

What landed (still red after):

- `nros-rmw-zenoh::shim::service::send_request_raw` no_std path: replaced
  3-attempt tight retry with `80 × 5 ms z_sleep_ms` (400 ms budget) so
  transient query-slot contention has time to clear.
- `examples/qemu-riscv64-threadx/rust/zenoh/action-client/src/main.rs`:
  outer 5-attempt retry around send_goal + accept-poll, matching the C
  action client's structure.

Symptom after both changes: `send_goal` returns Ok on attempt 1, but the
50 s accept-poll window expires with no reply. Subsequent retries fail
with `RequestInFlight` because the in-flight promise from attempt 1 still
holds the goal-service slot.

Suspected root cause: server-side queryable on this transport (NetX Duo
BSD + zenoh-pico) never sees the request, or reply path doesn't make it
back. Confirming would need server-side stdout capture in the test
fixture (currently only client output is captured) and likely zenoh-pico
trace logging on both sides. Out of scope for this session — booked as a
follow-up; pre-existing failure, not a Phase 119/120 regression.

### 120.4 — ThreadX RV64 DDS talker→listener — **DEFERRED**

`test_threadx_rv64_dds_rust_talker_to_listener_e2e`. Different backend
(dust-dds Rust, not zenoh-pico), not investigated this session. Pre-existing.

## Acceptance

- [x] 120.1 lands; `test_xrce_action_fibonacci` passes.
- [x] 120.2 lands; Zephyr Rust XRCE tests pass.
- [ ] 120.3 lands; ThreadX RV64 Rust action zenoh-pico E2E passes (deferred — server-side investigation needed).
- [ ] 120.4 lands; ThreadX RV64 Rust DDS talker test passes (deferred).
- [ ] `just test-all`: 720/720 pass.

Net result this session: 8 of 13 pre-existing baseline failures fixed
(Phase 119 + 120.1 + 120.2). The remaining 2 hard fails are both on
ThreadX RV64 Rust embedded targets and need separate investigation.

## Notes

The action-protocol fix in 120.1 affects all RMW backends (zenoh, XRCE, DDS, CycloneDDS, cffi). It's a pure error-mapping bug; no protocol or wire-format change. Tests that exercised the action protocol on backends that return `NoData` instead of `Ok(None)` from polling (e.g. XRCE-C) would have hit this; backends that already returned `Ok(None)` (e.g. zenoh native services) were unaffected.
