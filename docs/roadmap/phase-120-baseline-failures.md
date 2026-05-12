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

### 120.3 — ThreadX RV64 Rust action E2E (zenoh-pico) — **DEFERRED, root cause identified**

`test_rtos_action_e2e::platform_4_Platform__ThreadxRiscv64::lang_1_Lang__Rust`.

**Findings (this session, via tshark on lo + zenohd RUST_LOG=trace + manual
QEMU run with diagnostic prints in server):**

1. **Server CRASHES with illegal instruction**, not stuck. The crash
   sits behind the `wait_for_output_pattern("Waiting for goals")` boot
   gate, so the test fixture's `wait_for_output` captures the boot
   header but misses the post-boot crash. Manual QEMU run with the
   diagnostic heartbeat (`println!("[server] iter {}", iter)` every
   100 loop iters) prints `iter 0` then immediately traps:
   ```
   [EXCEPTION] mcause=0x2  (Illegal instruction)
                mepc=0x8023aa94    (inside .bss at packet_pool+4)
                mtval=0x0
   ```
   `packet_pool` is NetX Duo's `NX_PACKET_POOL` static in `.bss` — the
   CPU jumped to data via a corrupted function-pointer call.

2. **zenohd is healthy.** Full 4-way handshake (InitSyn / InitAck /
   OpenSyn / OpenAck) completes; lease negotiated at 10 s. Server
   declares all 3 queryables (`send_goal`, `cancel_goal`,
   `get_result`) plus 2 publisher tokens on the correct keyexprs and
   they are registered in the router's routing table. Then the
   server's session times out at exactly `T+10s` and zenohd unregisters
   every resource. Client connects ~4s later, sends the z_get, router
   has nothing to forward to. Confirms: the server is dead before the
   client ever sends a goal.

3. **Crash is service-count-correlated, not action-protocol-specific.**
   On ThreadX RV64 Rust:
   - pubsub (1 pub + 1 sub): **PASS**
   - service (1 service_server): **PASS**
   - action (3 service_servers + 2 publishers + status publisher = 6
     entities + 5 liveliness tokens): **FAIL**
   - C / C++ action (same shape): **PASS**

   Wire format is identical between C and Rust action server's
   declarations (verified via tshark hex). Difference is something
   Rust-specific in the build's interaction with NetX BSD / zenoh-pico
   beyond the ~5-entity threshold.

4. **Same crash shape as 120.4 (DDS listener).** DDS listener traps
   with `mepc=0` (null jump); action server with `mepc=0x8023aa94`
   (inside NetX packet_pool). Both are corrupted function-pointer
   indirections; almost certainly the same underlying root cause
   (likely the `ULONG → ALIGN_TYPE` shape already noted in
   `project_threadx_linux_pointer_truncation` — NetX BSD socket
   pointer truncation on 64-bit, but applied to a different code path
   than the threadx-linux fix).

**Defensive landings** shipped on branch `phase-120-threadx-fixes`:
`send_request_raw` no_std-path retry budget (80 × 5 ms z_sleep_ms vs
the prior 3-attempt tight loop) + client-side 5-attempt outer retry.
Neither addresses the root cause but both are improvements; keep.

**Test fixture improvement** (landed in `rtos_e2e.rs`): drain server
output for 2s after client_timeout, surface it in failure context.
This is how the crash became visible in the first place — without it
the failure looks like "stuck server" rather than "server crashed".

**Investigation update — root cause #1 fixed, root cause #2 identified.**

Two stacked bugs:

1. **ThreadX FFI ULONG width** (FIXED, commit `57669ba`):
   `nros-platform-threadx::ffi` declared every `ULONG` parameter as
   `u32`. On LP64 (rv64, x86_64 host-mode) ULONG is 8 bytes. Garbage
   in the upper 32 bits of every `tx_byte_allocate` / `tx_thread_*` /
   `tx_semaphore_*` call's ULONG-typed register. Z_malloc'd
   zenoh-pico structs landed with corrupted size / wait_option,
   surfacing as the `Illegal instruction` at NetX `packet_pool+4`
   crash once enough structs piled up. With the fix the server
   no longer crashes — it now reaches the spin loop cleanly and
   completes the full 4-way zenoh-pico handshake.

2. **`tx_thread_sleep` blocks indefinitely for the zenoh-pico lease
   task** (OPEN). Confirmed via UART trace: lease task enters
   `tx_thread_sleep(334)` (3.34 s sleep, 100 Hz tick) but never
   returns. The same `tx_thread_sleep` works for the app thread's
   pre-closure 0.5 s sleep, so the regression is specific to
   zenoh-pico-spawned tasks (priority 14, created via
   `_z_task_trampoline` → `tx_thread_create`). Net effect: lease
   task never sends keep-alives → router unregisters every server
   resource at exactly T+10 s (the negotiated `_lease`) → client
   queries arriving after T+10 s route to nobody →
   `ServiceRequestFailed`.

Evidence chain for #2:

- `tshark` on `lo:7473` shows zenohd → server keep-alives every 2.5 s
  (3-byte payloads). TCP path is healthy in both directions.
- `ZENOHD_LOG=trace` shows `Declare queryable 2 (0/fibonacci/...)`
  at T+0, `Unregister resource` at T+10.0 s, exactly the lease
  expiry. No errors.
- UART trace of the wrapped `sleep_ms` prints `[slp 334t]` (entry,
  ticks=334) but never the matching exit marker `[e]`. App_thread's
  earlier `[slp 50t]` did pair with `[e]` correctly.
- Trampoline trace confirms two tasks start
  (`[zpico] task trampoline start` ×2) and both reach their `_fun`.
  Neither prints `_fun returned` — so they're stuck inside their
  bodies, not crashing.

Open question for follow-up: why does `tx_thread_sleep` work for the
user-spawned app_thread but not for the zenoh-pico tasks spawned by
`_z_task_init` via `tx_thread_create`? Both go through the same
ThreadX kernel path. Candidates:

- Stack size (`Z_TASK_STACK_SIZE = 8192`) — bumping to 32 KB shifted
  the failure to a different fn-pointer crash, suggesting linker
  layout sensitivity. 64 KB doesn't unblock either.
- `TX_TIMER_PROCESS_IN_ISR` is undefined → timer-thread (priority 0)
  decrements sleep counters. Setting the flag exposes an FPU-not-
  enabled trap in the rv64 context-save (frcsr at trap entry).
- `Z_TASK_PREEMPT_THRESHOLD == Z_TASK_PRIORITY == 14` may interact
  badly with the timer thread's wake-up path on rv64.

**gdb memory-dump finding** (later session): at the stuck point, the
trapped thread's TCB stack-pointer field (`TX_TCB_STACK_PTR_OFF = 8`)
holds `0x80016b56` — resolves to **inside `_tx_thread_context_save+100`**,
a code address, not a stack address. `_tx_thread_context_save`
writes `sp → TCB.stack_ptr` at line 262 of the board-local copy.
For the field to end up with a code address, the saved `sp` had to
be pointing into `.text` at save time.

Strong evidence of a **nested-trap-during-context-save** pattern: an
interrupt (most likely the tick timer) fires while a thread is mid-
context-save, the inner trap re-enters `_tx_thread_context_save`
itself, the second pass writes its own runtime SP (which is
literally inside the function's instruction stream) into the TCB.
That corrupted stack-pointer is what causes the eventual
recursive trap in `trap_entry`'s `STORE x1, 28*REGBYTES(sp)`.

Closing the loop needs nested-interrupt guards in `trap_entry` /
`_tx_thread_context_save`. The RV64 ThreadX port may be missing
the `_tx_thread_system_state` increment + tick-pending defer that
the Cortex-M port uses. Out of scope for this session.

The matching ABI fix (5-asm 65 → 66 REGBYTES alignment) in
`c6274bb1` is independent and correct regardless of this nested-
trap bug — keep it. Test still fails because the nested-trap
corruption strikes before the action server can serve a goal.

Result is the same end-state on the test (T+10s router unregisters
the server's queryables, client `ServiceRequestFailed`), but the
root cause is now identified as **a kernel-level rv64 ThreadX bug
in nested-interrupt handling**, not in our Rust application code
or the zenoh-pico wire-protocol. The bug affects the lease task's
sleep specifically because the lease task's `tx_thread_sleep` is
where the thread voluntarily suspends and is most likely to
collide with a tick interrupt mid-context-save.

**Bare-metal zenoh-pico debug logging (commit `25a4973c`):** added a
vsnprintf + UART sink (`packages/zpico/zpico-sys/c/platform/threadx/log_uart.c`)
opt-in via `NROS_ZPICO_LOG_TO_UART=1` + `NROS_ZENOH_DEBUG=3`. Replaces
`ZENOH_LOG_PRINT=printf` (which pulls in stdio not available on
bare-metal) with `zpico_log_print(fmt, …)`. With this enabled, the
threadx-rv64 action server's full handshake is visible on UART:

```
Sending Z_INIT(Syn) → Received Z_INIT(Ack)
Sending Z_OPEN(Syn) → Received Z_OPEN(Ack)
Allocating queryable for (0/fibonacci/_action/send_goal/…)
Allocating queryable for (0/fibonacci/_action/cancel_goal/…)
Allocating queryable for (0/fibonacci/_action/get_result/…)
Allocating interest  for (0/fibonacci/_action/feedback/…)
Allocating interest  for (0/fibonacci/_action/status/…)
Received Z_FRAME message → Handling _Z_N_DECLARE: 8
Received Z_FRAME message → Handling _Z_N_DECLARE: 8
```

So the **handshake is healthy**, all 6 entities are declared, and the
read task processes the router's 2 DECLARE responses. The crash is
strictly in the **post-handshake** code path; setup is fine.

**gdb-multiarch findings (via QEMU `-s` gdb stub):**

The "lease task hang" is actually a silent crash. Attaching gdb to a
running stuck server shows PC sitting in `trap_handler`'s `j .L33`
infinite loop (`while (1)` after the exception print). With a
patched `tx_initialize_low_level.S` that passes the trapped ra to
trap_handler from `28*REGBYTES(saved_sp)`, the crash dump shows:

```
mepc       = 0x80251630   nx_bsd_socket_pool_memory + 8
mtval      = 0x8003a0a8   byte_pool_storage + 3392
mcause     = 5            Load access fault
```

PC is **inside `nx_bsd_socket_pool_memory`** — NetX BSD's backing
storage for the BSD socket block pool. The CPU jumped to data via a
JALR through a fn-pointer field of an allocated NX_BSD_SOCKET struct
(offset 8 of the second socket in the pool, given `nx_bsd_socket_array`
starts at the pool base). When that data is executed as instruction
stream, an LD inside it tries to load from `byte_pool_storage + 3392`
(plausibly the trapped thread's stack frame on the byte pool), which
also fails — `Load access fault` rather than `Illegal instruction`
just because the garbage decoded to a valid LD opcode first.

This matches the shape of the existing
`project_threadx_linux_pointer_truncation` memory note (NetX BSD's
ULONG-cast-pointer pattern misbehaving on 64-bit), but rv64's
`ULONG = unsigned long = 8 bytes` natively, so the threadx-linux x86_64
`NX_THREAD_EXTENSION_PTR_*` workaround macros aren't directly the fix
here. The bug is a different ULONG-vs-pointer interaction inside the
NetX BSD shim layer (`packages/zpico/zpico-sys/c/platform/threadx/network.c`)
or in `nxd_bsd.c` itself. Bisecting it needs a series of `nx_bsd_*`
print-on-entry/exit prints in network.c — out of scope for this
session.

### 120.4 — ThreadX RV64 Rust DDS talker→listener — **DEFERRED, likely same root cause as 120.3**

`test_threadx_rv64_dds_rust_talker_to_listener_e2e`. Listener crashes
with `mepc=0` (null jump) after `Waiting for messages...`. Same
corrupted-function-pointer signature as 120.3 — likely the same NetX
BSD pointer truncation, hit via a different (DDS RTPS) code path.

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
