# Phase 120 — Pre-Existing Baseline Failures

**Goal:** Drive the 4 pre-existing `just test-all` failures left over from Phase 119 down to zero.
**Status:** 1 fixed (`test_xrce_action_fibonacci`); 3 remain (Zephyr/ThreadX RTOS embedded tests with hardcoded XRCE agent port + similar infrastructure-level issues, deferred to a follow-up).
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

### 120.2 — Zephyr Rust XRCE locator from Kconfig — **TODO**

Make the Zephyr Rust XRCE examples (`examples/zephyr/rust/xrce/{talker,listener,service-{server,client},action-{server,client}}/src/lib.rs`) read `zephyr::kconfig::CONFIG_NROS_XRCE_AGENT_ADDR` + `CONFIG_NROS_XRCE_AGENT_PORT` instead of hardcoding `"127.0.0.1:2018"`. Requires either:

- A no_std-friendly `const_str` macro (since `ExecutorConfig::new` wants `&str`), or
- A `heapless::String<64>` built at `run()` entry from `write!(...)`.

### 120.3 — ThreadX Rust XRCE locator — **TODO**

Same fix in `examples/threadx-riscv64/rust/xrce/*` (or wherever the ThreadX action example lives).

### 120.4 — ThreadX DDS talker→listener — **TODO**

`test_threadx_rv64_dds_rust_talker_to_listener_e2e` — needs investigation. Likely separate from the port-hardcoding issue.

## Acceptance

- [x] 120.1 lands; `test_xrce_action_fibonacci` passes.
- [ ] 120.2 + 120.3 land; Zephyr + ThreadX Rust XRCE tests pass.
- [ ] 120.4 lands; ThreadX DDS talker test passes.
- [ ] `just test-all`: 720/720 pass.

## Notes

The action-protocol fix in 120.1 affects all RMW backends (zenoh, XRCE, DDS, CycloneDDS, cffi). It's a pure error-mapping bug; no protocol or wire-format change. Tests that exercised the action protocol on backends that return `NoData` instead of `Ok(None)` from polling (e.g. XRCE-C) would have hit this; backends that already returned `Ok(None)` (e.g. zenoh native services) were unaffected.
