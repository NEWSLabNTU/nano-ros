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

CPU is sitting in `trap_entry`'s first instruction
(`addi sp, sp, -66*REGBYTES`). Successive gdb attaches always show
PC at this address — a **recursive trap loop**. Each time `trap_entry`
runs, the very next instruction `STORE x1, 28*REGBYTES(sp)` faults
because `sp` is bad, triggering another trap that re-enters
`trap_entry`, and so on forever. The `[EXCEPTION] mcause=...` text
that `trap_handler` prints visibly does land on UART, but the
handler ends with `while (1) ;` so once any first exception fires
the system is wedged.

### Why "nested-interrupt guards" was the working hypothesis

`_tx_thread_context_save` lines 86–94 are:

```
la      x5, _tx_thread_system_state
lwu     x6, 0(x5)                  # read state
beqz    x6, _tx_thread_not_nested_save
addi    x6, x6, 1                  # nested path: increment
sw      x6, 0(x5)
```

The not-nested path (line 178–179) increments state again after the
branch. The read-check-store sequence is **not atomic**. If a second
trap fires between the load (line 87) and the store (line 179 in the
not-nested path), both passes can read `state == 0`, both take the
not-nested branch, and both perform the full save — the second pass's
save writes its own runtime SP (which is wherever the inner trap
landed in the call chain — e.g. inside `_tx_thread_context_save+100`
itself) into TCB.stack_ptr.

On RISC-V, `mstatus.MIE` is auto-cleared by hardware on every trap
entry, so a *timer interrupt* can't fire during this window. But
**synchronous exceptions** (load/store access fault, illegal
instruction, page fault) are unconditional — they fire even with MIE
cleared. The trap_entry's own `STORE x1, 28*REGBYTES(sp)` (line 79
of `tx_initialize_low_level.S`) is itself a memory access. If `sp`
points into a region that faults on store, that very instruction
triggers an inner trap with `mcause = Store/AMO access fault`.

The hypothetical fix: protect the state-update with the same
"already inside a trap" guard the Cortex-M ports use — set a
TCB-or-global flag in the *first* instruction of `trap_entry`,
check it on entry, and take a different (write-nothing, just
restart) path when the flag is already set. Same shape as
`_tx_thread_system_state` but updated before any memory access
that could itself fault.

### Why that hypothesis is probably wrong as the *root* cause

The nested-trap protection would prevent the *second* trap from
corrupting `TCB.stack_ptr`. It would NOT prevent the *first* trap's
SP from being bad in the first place. Once SP is bad, every trap
entry — even with perfect nested-trap handling — would still fault
on its own `STORE x1, 28*REGBYTES(sp)`.

So the actual root cause is whatever set SP to a `.text` address
*before* the first trap fired. Candidates:

1. **Rust task stack overflow inside the trampoline path.** The
   `_z_task_t.threadx_stack[8192]` field is 8 KB. NetX BSD send
   paths use 100+ byte frames on rv64 with the LP64D ABI, so a
   moderate call chain could exhaust it. Stack-checking enabled
   (`TX_ENABLE_STACK_CHECKING`) didn't trip — but that flag only
   compares against a sentinel **at context switch**, so a
   transient overflow that races a stop-at-zero-then-recover
   pattern can go undetected.
2. **`ULONG` width mismatch.** This board's `tx_port.h` re-defines
   `ULONG` as `unsigned int` (4 bytes) — see the header comment at
   the top of `packages/boards/nros-board-threadx-qemu-riscv64/config/tx_port.h`:
   > "Shadows ... to fix the ULONG typedef. The upstream port
   > incorrectly defines ULONG as 'unsigned long' (8 bytes) —
   > kernel code uses `ULONG *` pointer arithmetic assuming 4-byte
   > words."

   This contradicts the assumption behind commit `57669baf` (which
   bumped Rust FFI's ULONG params from `u32` to `c_ulong = u64`).
   The RV64 ABI passes both u32 and u64 in the same a-register
   with zero-extension for u32, so for **small values** Rust→C
   still receives the correct low 32 bits. But the Rust shim
   *now passes* 64-bit values to a C side that *reads* 32-bit
   words — the upper 32 bits land in the next ULONG-typed field
   if the C side does pointer-arithmetic. Worth re-evaluating.
3. **Rust→C calling convention near a callback.** The crash
   reliably reproduces only when the action server's full set of
   entities (3 services + 2 publishers + status publisher) is
   declared. Pubsub (2 entities) and service (1 entity) pass on
   the same path. The 6th entity (status publisher) registers a
   subscriber-side handler. The cb dispatch path may pass an
   argument with the wrong width and corrupt SP indirectly via a
   later spill.

### Web research findings (eclipse-threadx GitHub issues)

- **Issue #269** (`Question for RISCV-64 timer interrupt`,
  `eclipse-threadx/threadx`): user noted the upstream RV64 timer
  interrupt was written **without** `_tx_thread_context_save` /
  `_tx_thread_context_restore` (unlike RV32). Maintainer
  (`@goldscott`) confirmed save/restore IS required for any
  interrupt calling ThreadX APIs. Issue closed but not fixed
  upstream. **Our board crate already adds save/restore via
  `trap_entry` in `tx_initialize_low_level.S` — this is NOT the
  bug.** Cross-checked.
- **Issue #389** (`FPU support RISCV-64`, `eclipse-threadx/threadx`):
  user reported `tx_thread_schedule.S` resets `mstatus.FS` to 0,
  which traps on subsequent FP register access. Fix offered by
  user: `li t0, 0x3880` (FS = 1 = Initial) instead of `0x1880`
  (FS = 0 = Off) for FP builds. **Our board crate already applies
  this conditionally**: `li t0, 0x1880; li t1, 1<<13; or t0, t1, t0`
  under `#ifdef __riscv_float_abi_single|double`. Confirmed
  in all 3 sites (`tx_thread_schedule.S:213`,
  `tx_thread_context_restore.S:161`, `:275`). Not the bug.
- **Issue closed without resolution** in both cases — upstream
  RV64 ThreadX port has unresolved structural issues that the
  Eclipse maintainers haven't merged comprehensive fixes for.

Additional 16-byte alignment site found via this research:
`tx_thread_system_return.S` allocated `-29*REGBYTES = -232 B`
(misaligned). Patched to `-30*REGBYTES = -240 B`; matching
restore in `tx_thread_schedule.S` bumped from `29` to `30`. Commit
`bab09903`. 8/9 threadx-rv64 tests still pass after this fix
(non-breaking), but action Rust still fails — alignment is not
the cause of THIS specific bug.

### Caught the bad SP via in-kernel UART trace

Patched `trap_entry` (Phase-120.3-only, reverted after capture) to
emit `!sp=<sp> epc=<mepc> ra=<ra>` and spin if the trapped SP lies
inside `.bss` between TX globals and `byte_pool_storage` (range
`0x80038000..0x80039000` — does NOT include the 2 MB byte-pool
heap where legitimate thread stacks live).

First reliable capture:

```
!sp=0000000080038e70 epc=0000000080016c96 ra=0000000080016bb0
```

Decoded:

- `sp = 0x80038e70`  →  original sp (pre-trap_entry's -528 decrement)
  was `0x80039080`. That's the **gap between TX globals
  (~0x80038200) and `app_thread` TCB (0x800391e0)** — uninitialized
  `.bss`. Not a valid stack region. (Real stacks come out of
  `byte_pool_storage` which starts at `0x80039368`.)
- `epc = 0x80016c96`  →  `sd t0, 504(sp)` inside
  `_tx_thread_context_save` — the FCSR-save instruction near the
  function's tail. With sp = 0x80038e70, this STOREs to
  `0x80038e70 + 504 = 0x80039068` (still inside the gap).
- `ra = 0x80016bb0`  →  inside `trap_entry`, just after
  `call _tx_thread_context_save`. The trap_entry chain ran fine
  this far — context_save was still running when whatever exception
  fired.

Reading: a thread's TCB-stored `stack_ptr` (offset 8) was set to
`0x80039080` somewhere upstream of the trap. The next time that
thread was scheduled, `_tx_thread_context_restore` loaded sp from
TCB.stack_ptr → sp became bad. The thread then ran (or trap_entry
ran) until the next memory access faulted, at which point the
recursive trap loop began.

**This is the actual mechanism behind the gdb-observed
`_tx_thread_current_ptr = 0x800380e8` corruption.** Both
observations describe the same underlying state — the TX globals
block is being overwritten by something writing into the .bss gap
between TX globals and `app_thread`'s TCB.

### Confirmed Rust-only

Rebuilt the C action server with the same patched `trap_entry`
detector and ran it end-to-end against zenohd. C server output:

```
  Feedback: [0, 1, 1, 2, 3, 5]
  Goal SUCCEEDED
```

C client output: `Action completed successfully.`

**Zero `!sp=` triggers on C path.** Same ThreadX kernel build,
same `trap_entry` asm, same `_tx_thread_context_save`, same
zenoh-pico C library, same NetX BSD. The .bss-gap SP corruption
fires only when the Rust action server runs.

So the corrupting STORE is in code that's specific to the Rust
path. C action server uses `nros_action_server_init` →
`Executor::add_action_server_raw_sized` (callback model, registers
the action server in the executor arena). Rust action server uses
`Node::create_action_server` (manual-poll, no arena entry). Both
create the same 6 entities (3 queryables + 2 publishers + status
publisher), so the entity-count itself isn't the cause — the
difference is in **the application loop pattern** afterward:

| | C (callback) | Rust (manual-poll) |
|---|---|---|
| Spin loop | `nros_executor_spin_some(10 ms)` | `try_accept_goal(...)` then `executor.spin_once(10 ms)` |
| Arena entries | 1 action server | 0 (manual poll bypasses arena) |
| Buffer slabs | inside the arena entry | inside the `ActionServer` value on the app thread's stack |

The Rust `ActionServer` value (`GOAL_BUF + RESULT_BUF + FEEDBACK_BUF
+ cancel_buffer + ...`) is roughly 4 KB and lives **on the app
thread's stack** (since `Node::create_action_server` returns it by
value). The C path keeps the same buffers inside an arena entry,
i.e. inside the `Executor` value, which is itself either inline in
`app_thread`'s stack or accessed via a stable address. So a likely
mechanism: the Rust `ActionServer` lives at an app-stack offset
that's close enough to the byte-pool / TX-globals region that an
out-of-bounds buffer write lands in the .bss gap.

### Next: instrument Rust's ActionServer construction

- Print `&server as *const _ as usize` after `node.create_action_server()`.
  If the address is `0x8003xxxx` (in .bss / TX-globals range) the
  hypothesis is confirmed.
- If yes: `ActionServer` is escaping stack and being placed in
  static storage by the compiler. Track down why.
- Else: bisect by removing fields from the action server's
  buffer slabs to find which write goes out of bounds.

**Hypothesis-1 result (verified):** `ActionServer` address is
`0x800554b8` (size 5896 B) — in `byte_pool_storage` (= app_thread's
ThreadX-allocated stack region). NOT in `.bss` / TX-globals.
ActionServer placement is legitimate; the buffer slabs aren't the
SP-corruption source.

### Why C vs Rust take different paths inside nros-node

The CLAUDE.md says "C API: thin wrapper delegates to nros-node".
The wrapper IS thin — but it delegates to a **different nros-node
entry point** than the Rust example uses:

| | Rust example | C example (nros-c thin wrapper) |
|---|---|---|
| User-facing API | `node.create_action_server::<A>(name)` | `nros_action_server_init` + `nros_executor_add_action_server` |
| Internal nros-node call | `Node::create_action_server_sized` (manual-poll) | `Executor::add_action_server_raw_sized` (callback model + arena) |
| ActionServer storage | returned by value, lives on caller's stack | inside `Executor.arena[slot]` (16 KB inline buffer in `Executor`) |
| App loop pattern | `try_accept_goal()` + `spin_once()` | `nros_executor_spin_some()` which dispatches via callbacks |

Both create the same 6 zenoh entities (3 queryables + 2 publishers
+ status publisher) with the same node identity, so wire-level
state is identical. The CRUD difference is the **post-handshake
spin loop**: arena-based callback dispatch on the C side vs
explicit `try_*` polling on the Rust side. The .bss-gap SP
corruption is somewhere in **the Rust manual-poll spin loop**.

A clean re-test would be to write a Rust example that uses
`executor.add_action_server` (= callback model, same Rust path C
ultimately calls). If that passes the rv64 test, the bug is
narrowed to the manual-poll-specific path; if it fails, the bug
is in the Rust→C calling-convention layer common to both Rust
paths.

### Per-iteration heartbeat narrows the crash to `spin_once`

Patched the rv64 Rust action server to print
`[iter N] before/after try_accept_goal/spin_once` for every
iteration. Result:

```
[iter 47] before try_accept_goal
[iter 47] before spin_once
[iter 47] after spin_once
[iter 48] before try_accept_goal
[iter 48] before spin_once
   <crash>

[EXCEPTION] mcause=0x1 (Instruction access fault)
mepc=0x0 mtval=0x0
```

**The crash is reproducibly inside `executor.spin_once(10 ms)` at
iteration 48** (~480 ms after the spin loop starts, BEFORE the
client even connects). `try_accept_goal` returns cleanly all the
way through iter 48. `mepc=0` means a NULL function-pointer JALR.

iter 48 = 0.48 s in. zenoh-pico's lease task's first keep-alive
isn't due until ~3.34 s, so that's not the trigger. Read task
processed the router's 2 DECLARE responses earlier (~T+0.1 s).
Whatever fires the NULL JALR happens deep inside `drive_io →
context.spin_once → zpico_spin_once → condvar_wait_until`, OR in
a callback dispatched from the read task during the condvar wait.

Most likely candidate: the `queryable_callback` Rust fn registered
via `context.declare_queryable_raw(keyexpr, queryable_callback,
ctx)` (see `nros-rmw-zenoh/src/shim/service.rs:244`). The C-side
`g_queryables[idx].callback` field gets `queryable_callback`'s
function pointer; the C-side `query_handler` then invokes it as
`entry->callback(...)`. If `queryable_callback`'s function pointer
ever lands at NULL (table corruption, wrong idx, or Rust fn-
pointer ABI mismatch), the call goes to 0x0.

### Next: instrument zenoh-pico's query_handler / sample_handler

- Add printk to `query_handler` (in
  `packages/zpico/zpico-sys/c/zpico/zpico.c`) right before
  `entry->callback(...)`. Print `entry->callback` pointer value.
  If 0x0, confirms the callback table is being NULL'd.
- Same for `sample_handler` (subscribers) and the get-reply
  handler — any of them may share the corruption.
- If callback pointers look fine but the call still goes to NULL:
  Rust ABI mismatch on the `extern "C" fn` signature passed
  through the table.

### First-trap capture confirms the bad fn pointer target

Patched `trap_entry` to dump `sp / mepc / mcause / mtval` on the
**first** non-interrupt trap (gated by a `_trap_caught` flag in
`.data`) and spin. Result:

```
!t1 sp=0x80046f40 ep=0x80251630 mc=5 mt=0x800393d8
```

- `sp = 0x80046f40` → legitimate thread-stack address inside
  `byte_pool_storage`. Sp itself is NOT corrupted on the first
  trap — the earlier "sp in .bss-gap" observations were the
  result of *subsequent* recursive trap_entry calls walking sp
  downward (each decrement another 528 bytes).
- `mepc = 0x80251630` → inside **`nx_bsd_socket_pool_memory`**
  (NetX BSD socket-pool data). Same address every run.
- `mcause = 5` (Load access fault), `mtval = 0x800393d8` →
  the load address that faulted. That's `s0 + 96` where `s0`
  happens to hold `0x80039378` (~start of `byte_pool_storage`)
  — the data at the bad PC decoded as `lw s0, 96(s0)`, so the
  load came from interpreting socket-pool data bytes as an
  instruction stream.

**So the first trap is already a consequence of a JALR landing
inside `nx_bsd_socket_pool_memory`.** A function-pointer field
somewhere holds the value `0x80251630`. When user code (or
zenoh-pico) calls that fn pointer, the CPU jumps to socket-pool
data bytes, decodes them as RISC-V instructions, and the load
inside the garbage instruction stream faults.

The corrupt fn pointer value is **deterministic** across runs —
same `0x80251630` every time. Suggests a specific data field that
gets set deterministically wrong, NOT a randomized memory bug.

### Note on nx_bsd_socket_pool_memory layout

```c
static ULONG nx_bsd_socket_pool_memory[NX_BSD_MAX_SOCKETS *
    (sizeof(NX_TCP_SOCKET) + sizeof(VOID *)) / sizeof(ULONG)];
```

In `third-party/threadx/netxduo/addons/BSD/nxd_bsd.c:97`. The
`sizeof(VOID *)` is 8 bytes on rv64; `sizeof(ULONG)` is **4 bytes**
under our board's `tx_port.h` override. NetX uses this as the
backing memory for `nx_bsd_socket_block_pool`, which stores
NX_TCP_SOCKET-sized blocks. Each block has an 8-byte
linked-list pointer prepended by `_nx_block_pool_create`.

The arithmetic `* (size + 8) / 4 * 4` reduces to the correct
total bytes, but the block-pool's block-size argument is
`sizeof(NX_TCP_SOCKET) + sizeof(VOID *)`. If the C compiler
computes `sizeof(NX_TCP_SOCKET)` based on a different `ULONG`
width than the kernel asm assumes, blocks land at unexpected
offsets and the "next" pointer of one block ends up pointing
inside the data area of another.

**Concrete fix candidate:** check whether `sizeof(NX_TCP_SOCKET)`
computed by C includes any ULONG fields whose width changed by
the board's `tx_port.h` override. If yes, every cmake build of
NetX uses the same ULONG override so sizes stay consistent, but
the asm-coded TCB offset constants (`TX_TCB_*_OFF`) are hand-
written assuming the same ULONG=4 layout. A mismatch would
explain why this corruption fires only on rv64 (where the
override applies) and only with the heavier action-server entity
load.

### TX_TCB_*_OFF offsets verified — NOT the bug

User asked whether TX_TCB_*_OFF constants come from manual math
or are derived. Answer: hand-coded in the board's
`tx_port.h:54-61`, based on the comment block's documented
layout. Verified via a cross-compiled `_Static_assert` (see
`tmp/verify_offsets.c`): all 8 offsets match `offsetof(TX_THREAD,
field)` as the C compiler computes it under the board's ULONG=u32
override. So the asm offsets DO match the C struct.

That rules out the "asm-offset / C-struct mismatch" hypothesis.
The `0x80251630` bad-JALR target value must come from a different
mechanism. Candidates remaining:

1. A struct field stores `&socket_array[0]` (full 8-byte pointer)
   in a slot the caller later interprets as a function pointer.
   `nx_bsd_socket_pool_memory + 8` skips over the block-pool's
   prepended next-link pointer and lands at the start of the
   first `NX_TCP_SOCKET` block. So the bad value IS the first
   socket's base address.
2. A callback registration somewhere passes `&socket[0]` where it
   should have passed a function pointer. Most likely candidate:
   any `nx_*_set_*_notify` API where the callback argument is
   confused with the user-context argument by a Rust-side caller.

### Next: identify the bad STORE

Watch for any STORE to addresses `0x800380c0..0x80039000` (the
TX-globals block, including the gap up to `app_thread` TCB). Most
likely culprit: a function pointer or buffer-with-bad-offset that
treats the .bss gap as available memory. Possible candidates:

1. NetX BSD socket / addrinfo pool init writing past end of pool
   (saw the `(UINT) socket_ptr->nx_tcp_socket_reserved_ptr`
   pointer-truncation warnings during `build-fixtures`).
2. ThreadX run_count / time_slice writes via a stale TCB pointer
   inside `_tx_thread_schedule.S` lines 109-112.
3. zenoh-pico's `_z_task_t` malloc returning a pointer whose
   stack region overlaps the gap because of misaligned heap layout.

### gdb session findings — ThreadX globals intermittently corrupted

Sampled `_tx_thread_current_ptr` at multiple gdb-attach points after
the action server hung:

- Sample 1 (`tmp/gdb-watch.sh`, 8s after server start):
  - `_tx_thread_current_ptr` (address 0x800380c8) = `0x800380e8`
    (= address of `_tx_thread_preempt_disable`, NOT a TCB pointer)
  - `_tx_thread_execute_ptr` = `0`
  - PC stuck at `trap_entry` (recursive trap loop)
- Sample 2 (`tmp/gdb-globals.sh`, 6s after server start):
  - `_tx_thread_current_ptr` = `0x80253428` (a valid TCB address)
  - `_tx_thread_execute_ptr` = `0x80253428`
  - PC at `trap_handler` (in the `while(1)` after the EXCEPTION print)

So the ThreadX globals **are being corrupted intermittently** —
sometimes they hold valid TCB addresses, sometimes they hold
addresses of *adjacent globals* (current_ptr ← preempt_disable's
address). This is consistent with an off-by-one or off-by-N memory
write that lands inside the `.bss` block holding the ThreadX globals
(`0x800380c0` .. `0x80038200` region).

The corruption appears WHILE the system is mid-trap (PC in
`trap_entry` / `trap_handler`), suggesting the trap entry asm itself
may be writing into this region — either because the trap stack
pointer was bad before the trap fired (we ended up using `.bss`
addresses as stack), or because the context-save's `STORE sp,
TX_TCB_STACK_PTR_OFF(t1)` ran with `t1` already pointing into the
globals block instead of into a real TCB.

The "PC ends inside `nx_bsd_socket_pool_memory + 8`" observation
from earlier sessions is consistent — that's just a different
recursive-trap landing point depending on linker layout and which
.bss region the bad SP wandered into.

Watchpoint experiment (`tmp/gdb-watchpoint.sh`): set hardware watch
on `*0x800380c8`. Caught the initial write (current_ptr =
`0x800391e0` at `app_thread_entry`, a valid TCB) and the
clear-to-zero (at `_tx_thread_dont_save_ts`, expected). Subsequent
writes weren't captured cleanly because of gdb-batch script
limitations on `commands`/`silent`. Re-attempt needs an interactive
gdb session or a Python-scripted gdb to step through every
watchpoint hit and dump PC + ra.

### Next steps for a fresh session

- Re-read board's `tx_port.h` ULONG-is-u32 note and revert the FFI
  ULONG widths to `u32` on `nros-platform-threadx`, **but only for
  this specific port**. Verify pubsub/service Rust tests still pass
  (they may already work coincidentally on RV64 ABI even with the
  wider type).
- Bump `_z_task_t.threadx_stack[]` to 32 KB and recompile *without
  the alignment attribute* — verify whether 8 KB is the cliff and
  whether the alignment attribute itself shifts the failure.
- Single-step the lease task at the first `tx_thread_sleep` call
  via gdb breakpoint, capture sp / ra / mstatus on every instruction
  until the trap fires, identify the exact instruction that
  corrupts SP.

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
