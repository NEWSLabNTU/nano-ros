# Phase 155 — rtos_e2e follow-ups (Phase 154 spinoffs)

**Goal.** Close the four failure modes that Phase 154's ABI fix
unblocked + surfaced. After 154, the ThreadX-Linux matrix is 9/9
and FreeRTOS Rust is 3/3, but four issues remain in the rtos_e2e
test fleet. Each has a different root cause; bundling here so
they're tracked without colliding.

**Status.** Open. Surfaced during 154.4 verify.

**Priority.** Medium. Bench / docs unaffected; only rtos_e2e
matrix coverage.

**Depends on.** Phase 154 landed (commits 7f5e9a86 → deb94258).

## Issue 155.A — ThreadX-RISC-V Rust illegal-instruction trap

**Symptom.** Every `rtos_e2e Platform__ThreadxRiscv64
lang_1_Lang__Rust *` test hits:

```
[EXCEPTION] mcause=0x2 mepc=0x8003396e mtval=0x0
  Illegal instruction
  ra=0x800306fe (trap_entry)
```

**Trace** (GDB, watchpoint on `*(unsigned short *)0x8003396e`):

```
Hardware watchpoint 1: *(unsigned short *)0x8003396e
Old value = 1337 (c.jalr a5 encoding tail)
New value = 0
0x80006038 in zpico_init_with_config (+42)
   0x80006038:  sd s7, 88(sp)
pc  0x80006038
ra  0x80004916 <nros_rmw_zenoh::shim::session::ZenohSession::new+1378>
sp  0x80033910  <nros_rmw_cffi::CffiSession::open_with_vtable+128>   ← !!
```

The stack pointer `sp = 0x80033910` is **inside .text** at
`nros_rmw_cffi::CffiSession::open_with_vtable+128`. The
`sd s7, 88(sp)` then writes a register value into .text,
corrupting code at `0x80033968` / `0x8003396e` / etc. (overlay
disasm later shows `0x8003396e: 0x0000 c.illegal`, which is
how the trap fires when execution reaches that address).

**Root cause.** `sp` shouldn't ever hold a code address. Two
candidates:

1. **Stack base setup bug.** The app thread's stack is allocated
   from the ThreadX byte pool (2 MB, at `0x80050168`).
   `byte_pool_storage` is well above `.text` (`0x80000000 –
   0x8003f000`). If `tx_byte_allocate(...)` returned a pointer
   inside `.text` — i.e., byte-pool storage was corrupted, or
   the allocator's free-list head was clobbered — the thread
   stack lands in invalid memory.

2. **Register save / restore corruption in board ASM.** The
   board ships custom `tx_thread_schedule.S` /
   `tx_thread_context_{save,restore}.S` (ULONG=4 struct-offset
   patches). One of these may be loading `sp` from the wrong
   stack-frame offset, swapping `sp` and a saved callee-saved
   register.

**Suggested debug path.**

- [ ] **155.A.1.** Verify byte-pool integrity at thread create.
      Add a debug print of `tx_byte_pool` state +
      allocation pointer just before `tx_thread_create` for
      `nros_app` in `nros_board_init_eth` / `app_thread`.
- [ ] **155.A.2.** Dump initial `sp` of app thread when
      `app_task_entry` first runs. Compare to expected stack
      range (allocator return + 512 KB).
- [ ] **155.A.3.** Single-step through the first
      ThreadX context switch into app thread (qemu `-s -S`,
      gdb hardware breakpoint on `_tx_thread_schedule`).
      Verify `sp` reload from saved context matches what
      `_tx_thread_stack_build` wrote.
- [ ] **155.A.4.** If board ASM is the suspect, diff our
      overrides vs. vendor port `.S` files line-by-line for
      register-offset miscounts.

**Acceptance.** All three RISC-V Rust `rtos_e2e` variants pass.

## Issue 155.B — FreeRTOS C `nros_support_init -1`

**Symptom.** Every `rtos_e2e Platform__Freertos lang_2_Lang__C *`
test prints:

```
nros C Listener (FreeRTOS)
[nros] examples/qemu-arm-freertos/c/zenoh/listener/src/main.c:62
  nros_support_init(&app.support, NROS_APP_CONFIG.zenoh.locator,
                    NROS_APP_CONFIG.zenoh.domain_id) -> -1
```

— at 20 s test deadline, no "Waiting for messages" surfaced.

**Differs from Rust path:** Rust calls `Executor::open` →
`nros_rmw_zenoh::register()` → `zpico_open`. C calls
`nros_support_init` → `nros_support_init_named` →
`Executor::open` (same underneath, but with a different
session-name + locator copy + state-machine init around it).

**Hypothesis.** `support` state machine probably trips a
guard. Check `nros_support_state_t` transitions — could be
the C-side static `nros_support_t` lands in `.bss` zero-
initialised → `state` happens to equal
`NROS_SUPPORT_STATE_UNINITIALIZED` (good). Then
`SessionMode` derivation or locator-copy size assertion
fails on FreeRTOS QEMU's smaller `nros_support_t::locator`
buffer.

**Suggested debug path.**

- [x] **155.B.1.** Print the actual return code rather than
      just "-1". `NROS_RET_*` values from
      `packages/core/nros-c/include/nano_ros/ret.h` tell us
      which branch tripped.
      **Landed 2026-05-18.** `nros::internals::open_session`
      no longer collapses every backend failure to
      `TransportError::ConnectionFailed`; the real variant
      now propagates to the C side. `support.rs` decodes via
      a new `transport_error_to_ret(TransportError)` helper:
      `ConnectionFailed`/`Disconnected` → `NROS_RET_NOT_FOUND`
      (-4); `Timeout`/`WouldBlock` → `NROS_RET_TIMEOUT` (-2);
      `InvalidConfig` → `NROS_RET_INVALID_ARGUMENT` (-3);
      `Buffer/Message/TooLarge` → `NROS_RET_FULL` (-6);
      `PublishFailed` → `NROS_RET_PUBLISH_FAILED` (-10);
      `Service*Failed` → `NROS_RET_SERVICE_FAILED` (-9);
      everything else stays `NROS_RET_ERROR` (-1) so any
      caller branching on `== NROS_RET_ERROR` keeps working.
      Next `nros_support_init -> -X` line in the FreeRTOS C
      test logs tells which precondition the backend
      rejected; 155.B.2/.3 branch on that code. Verification
      pending the FreeRTOS C fixture rebuild (cmake cross-
      toolchain config blocks the local rebuild today;
      patched fixture lands once the upstream cmake issue
      resolves).
- [ ] **155.B.2.** If `NROS_RET_BAD_SEQUENCE` — the
      `support` was non-zero pre-call. Check linker layout
      for `app.support` on FreeRTOS (BSS vs DATA).
- [ ] **155.B.3.** If `NROS_RET_ERROR` from
      `Executor::open` — likely the same shape as 154
      surfaced for Rust; verify the C path also benefits
      from the ABI flip (which is FreeRTOS+lwIP-skipped
      now, see Phase 154 final commit).

## Issue 155.C — FreeRTOS C++ service test, 0 responses

**Symptom.** `test_rtos_service_e2e Platform__Freertos
lang_3_Lang__Cpp` fails at 100 s (extended timeout) with:

```
[freertos cpp] responses: 0, completed: false
freertos cpp service E2E failed — got 0 responses
  (expected >= 3)
```

Pubsub + action variants on the same platform / lang pass.

**Differs from pubsub / action:** service E2E expects 3+
request-response round trips. Server probably never receives
requests, or responds but client never sees them. The
keep-alive + query-reply path through zenoh-pico's
`zp_get` differs from the publish / subscribe path.

**Suggested debug path.**

- [x] **155.C.1.** Client stdout captured via `nextest
      --no-capture`. Pre-fix shape:
      `nros C++ Service Client (FreeRTOS) / Node created /
      Service client ready / Call [1] failed: -2 (Timeout)`.
      Server output not currently captured by `start_pair`;
      need a per-process stdout split (see `RtosProcess`
      enum).
- [ ] **155.C.2.** Run zenohd with `ZENOHD_LOG=trace` to see
      whether queries flow at all.
- [ ] **155.C.3.** Compare to ThreadX-Linux C++ service (which
      passes) — diff zpico-sys feature flags / link policy
      between the two platforms.

**Tried + reverted** (didn't fix):

- Pre-discovery `for (int i = 0; i < 500; i++) nros::spin_once(10)`
  before first call. Side effect: client hung past
  "Discovery wait done" — never reached the `for` body.
  Possible interaction between intensive spin_once and
  zenoh-pico's read-task on lwIP. Reverted.
- Bump `Future::wait` timeout 5 s → 30 s. Same `Call [1]
  failed: -2` — server simply never replies within 30 s.
  Confirms it's not just first-call cold-start latency;
  reply path is broken end-to-end. Reverted.

**Likely real cause.** FreeRTOS+lwIP C++ `Future::wait` →
`nros_cpp_spin_once` doesn't pump zenoh-pico's reply
delivery the same way Rust's `executor.spin_once` does. C++
Listener's `nros::spin_once(10)` in a tight loop works fine
for pubsub; the query-reply path probably needs a different
spin shape OR there's a bug in `nros-cpp` `Client::send_request`'s
slot-management that mishandles the reply when
`Z_FEATURE_MULTI_THREAD=1` (alias TU + vendor freertos/lwip
both running tasks).

**Acceptance.** All three FreeRTOS C++ service variants pass
(currently only pubsub + action pass).

## Issue 155.D — RISC-V cmake env-var leak

**Symptom.** `just threadx_riscv64 build-fixtures` builds Rust
examples cleanly but C / C++ fixture build fails with:

```
fatal error: nx_user.h: No such file or directory
[…]/packages/boards/nros-board-threadx-linux/config
```

Note the **Linux** board's config dir leaking into the **RISC-V**
build.

**Root cause.** `.envrc` exports `THREADX_CONFIG_DIR` default
to `nros-board-threadx-linux/config`. RISC-V Rust examples
override via per-example `.cargo/config.toml [env]` block.
But cmake-driven cargo invocations (corrosion) don't pick up
those overrides because corrosion forks `cargo` from cmake's
process environment, bypassing the per-example `.cargo/config.toml`.

The cmake board file at `cmake/board/nano-ros-board-riscv64-qemu.cmake`
sets `THREADX_CONFIG_DIR` as a cmake CACHE variable, but
that's not an env-var so it doesn't reach the spawned cargo
process.

**Fix sketch.**

- [x] **155.D.1.** Two-pronged fix (commit `deed6b57`):
      board `cmake` file does `set(ENV{THREADX_CONFIG_DIR}
      …)` + sibling vars; `just threadx_riscv64
      build-fixtures` exports the same names in the shell
      before `cmake -S` (cmake `-D…=…` only sets cache,
      doesn't reach subprocess env).
- [~] **155.D.2.** Alternative path (in-cmake `set(ENV…)`
      only) implemented as the cmake half of #1; the
      justfile half is needed because `cmake --build` runs
      after configure exits, so the configure-time env
      patch alone doesn't survive.
- [ ] **155.D.3.** Replicate the env-export pattern in
      `just/threadx-linux.just` + `just/freertos.just`
      `build-fixtures` recipes for parity. Audit other
      cmake-driven cargo recipes for the same pattern.

**Acceptance — partial.** `just threadx_riscv64 build-fixtures`
now passes the env-leak failure point. Hits next-layer issue:
`nxd_bsd.h: conflicting types for 'suseconds_t'` when bare-
metal compile pulls newlib / picolibc `suseconds_t` against
NetX-Duo's own typedef. Spun into Issue 155.E (header guard /
typedef conflict).

## Issue 155.E — RISC-V cmake-build `suseconds_t` conflict (new)

**Symptom.** After 155.D's env propagation lands,
`just threadx_riscv64 build-fixtures` reaches the C / C++
glue compile then errors:

```
nxd_bsd.h:209:33: error: conflicting types for 'suseconds_t'
  209 | #define nx_bsd_suseconds_t      suseconds_t
nxd_bsd.h:629:21: note: in expansion of macro 'nx_bsd_suseconds_t'
```

The bare-metal compile of `board_threadx_qemu_riscv64.c`
includes `nxd_bsd.h`, which defines `nx_bsd_suseconds_t` =
`suseconds_t`. picolibc's `<sys/types.h>` already declares
`suseconds_t` with a different (or incompatible-by-typedef-
attribute) signature.

**Suggested debug path.**

- [x] **155.E.1.** Root cause confirmed (commit `aab273ab`):
      `threadx_glue` cmake compile passed only
      `TX_INCLUDE_USER_DEFINE_FILE` + `NROS_PLATFORM_BAREMETAL`;
      missing `NX_BSD_ENABLE_NATIVE_API` made `nxd_bsd.h`
      hit the alias-typedef path that collides with
      picolibc's `suseconds_t`. Same flag the Rust-side
      build sets via zpico-sys manifest. Rust-side board
      build.rs also got an explicit
      `.define("NX_BSD_ENABLE_NATIVE_API", None)` in
      `configure_riscv64` as belt-and-braces (this commit).
- [x] **155.E.2.** Fixed by adding
      `NX_BSD_ENABLE_NATIVE_API` + `NX_INCLUDE_USER_DEFINE_FILE`
      to `nros_threadx_build_glue(... DEFINES ...)` in
      `cmake/board/nano-ros-board-riscv64-qemu.cmake`.
- [x] **155.E.3.** Plus toolchain-level fixes: picolibc +
      cxx-compat `-isystem` paths in
      `cmake/toolchain/riscv64-threadx.cmake`; gcc-driver
      flag filter (`-nostartfiles` etc.) in
      `cmake/toolchain/riscv64-lld-wrapper.sh` so lld
      stops erroring on flags it doesn't understand.

**Acceptance — partial.** `just threadx_riscv64
build-fixtures` clean through Rust + C + C++ build. RISC-V
C / C++ E2E reaches runtime but tests fail with
`nros_support_init -> -1` — same shape as 155.B (FreeRTOS C).
The 155.B fix (this commit) propagates `TransportError`
variants to specific `NROS_RET_*` codes so next RISC-V
C / C++ run logs which precondition the backend rejected.

## Notes

These four are mutually independent (different root causes,
different file sets). They can land in any order in any number
of follow-up sessions; bundling here just for tracking.

Phase 154 itself stays closed — its acceptance was "ThreadX-Linux
Rust E2E unblocked" which passed. The four items above are
follow-ups, not regressions of 154.
