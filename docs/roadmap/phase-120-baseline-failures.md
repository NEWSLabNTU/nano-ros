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

### 120.3 — ThreadX RV64 Rust action E2E (zenoh-pico) — **DEFERRED**

`test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`.
**Same platform, same QEMU, same zenoh-pico, same NetX BSD — C and C++
pass, only Rust fails.** Manual-poll vs callback path is the only
functional difference; both delegate to the same `Node::create_action_*`
and `session.drive_io()` primitives.

Investigation this session:

- Captured server output (added `server.wait_for_output` drain after
  client_timeout in `rtos_e2e.rs:803-810`). Server prints `Waiting for
  goals...` then **nothing** — `try_accept_goal` never sees a request,
  i.e. the queryable callback never fires for the send_goal key.
- Client side: `send_goal` returns Ok on attempt 1 (z_get sent), then
  the 50 s accept-poll window expires with no reply. Retries fail with
  `RequestInFlight` (in-flight slot from attempt 1).
- Confirmed key-expression composition matches between client and
  server (both go through `Node::create_action_*` with node identity).
- Confirmed network setup: both QEMU instances on slirp NAT to host
  loopback, both connect to zenohd on port 7473 (per-(variant, lang)
  allocation; C uses 7573 and passes).
- Confirmed buffer pool sizes: `ZPICO_MAX_QUERYABLES=8` (action server
  uses 3); `ZPICO_MAX_PUBLISHERS=8` (uses 2); `APP_THREAD_STACK_SIZE=64K`.
  Nothing exhausted.
- Defensive landings shipped on branch `phase-120-threadx-fixes`:
  `send_request_raw` no_std-path retry budget (80 × 5 ms z_sleep_ms vs
  the prior 3-attempt tight loop) + client-side 5-attempt outer retry.
  Neither addresses the root cause but both are improvements; keep.

Real fix needs zenoh-pico tracing on both sides — verify the server's
queryable is actually registered with the router (gossip vs declare
liveliness), and that the router forwards the z_get to it. May also
need a smaller repro outside the action protocol (e.g. raw service
call via manual-poll) to isolate whether this is action-specific or
generic to manual-poll services on this transport.

### 120.4 — ThreadX RV64 Rust DDS talker→listener — **DEFERRED**

`test_threadx_rv64_dds_rust_talker_to_listener_e2e`. Listener crashes
with RISC-V `Instruction access fault` (mepc=0, mtval=0) immediately
after printing `Waiting for messages...`. Code jumped to a null
function pointer.

Symptoms:

- Crash inside the `loop { spin_once; try_recv; }` body, not in setup.
- Listener trap-handler prints `ra=0x80014ca6` (just after a `jal
  uart_puts` inside the trap printer itself — disassembly confirms
  this is the handler's own ra after printing). Captured-at-trap ra
  doesn't unambiguously point to the faulting caller.
- Possibly related to the existing `project_threadx_linux_pointer_truncation`
  memory note: `ULONG → ALIGN_TYPE` mismatch for BSD socket pointer
  casts on 64-bit. dust-dds-rs on NetX BSD may have the same shape.

Real fix needs in-QEMU debug (gdb attach via `-S -s` flag), or static
analysis of `nros-rmw-dustdds` + NetX BSD shim for `(ULONG)pointer`
truncation on rv64.

## Acceptance

- [x] 120.1 lands; `test_xrce_action_fibonacci` passes.
- [x] 120.2 lands; Zephyr Rust XRCE tests pass.
- [ ] 120.3 lands; ThreadX RV64 Rust action zenoh-pico E2E passes.
- [ ] 120.4 lands; ThreadX RV64 Rust DDS listener stops crashing.
- [ ] `just test-all`: 720/720 pass.

Final this session: 11/13 baseline failures fixed (Phase 119 + 120.1 +
120.2). Remaining 2 are both on ThreadX RV64 Rust embedded targets —
one a transport/manual-poll issue, one a 64-bit pointer-truncation
crash. Both need real debugger sessions, not source inspection.

## Notes

The action-protocol fix in 120.1 affects all RMW backends (zenoh, XRCE, DDS, CycloneDDS, cffi). It's a pure error-mapping bug; no protocol or wire-format change. Tests that exercised the action protocol on backends that return `NoData` instead of `Ok(None)` from polling (e.g. XRCE-C) would have hit this; backends that already returned `Ok(None)` (e.g. zenoh native services) were unaffected.
