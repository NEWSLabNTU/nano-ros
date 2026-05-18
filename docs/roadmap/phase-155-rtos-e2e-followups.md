# Phase 155 — rtos_e2e follow-ups (Phase 154 spinoffs)

**Goal.** Close the four failure modes that Phase 154's ABI fix
unblocked + surfaced. After 154, the ThreadX-Linux matrix is 9/9
and FreeRTOS Rust is 3/3, but four issues remain in the rtos_e2e
test fleet. Each has a different root cause; bundling here so
they're tracked without colliding.

**Status.** ✅ CLOSED 2026-05-18. All six sub-issues resolved (A/B/C/D/E/F). NuttX C action remains the sole rtos_e2e fail, tracked under preexisting Phase 77 async-action-client work (NOT a 155 sub-bug).

**Priority.** Medium. Bench / docs unaffected; only rtos_e2e
matrix coverage.

**Depends on.** Phase 154 landed (commits 7f5e9a86 → deb94258).

## Issue 155.A — ThreadX-RISC-V Rust illegal-instruction trap ✅ FIXED 2026-05-18

**Root cause.** Stack-base setup bug (candidate 1 in original triage). Weak `const uint32_t nros_board_app_stack_size = 64 * 1024` in `packages/boards/nros-board-common/c/threadx_hooks.c` was constant-folded by gcc at the call site, so the RISC-V overlay's strong-override (`512 * 1024` in `packages/boards/nros-board-threadx-qemu-riscv64/c/board_threadx_qemu_riscv64.c`) never took effect. App thread got the 64 KB weak default; Rust `Executor::open` stack frame exceeded that; `sp` underflowed past byte-pool storage into `.text` at `CffiSession::open_with_vtable+128`; next register save (`sd s7, 88(sp)`) corrupted code there → illegal-instruction trap on next fetch.

**Fix** (commit ffe4468d): drop `const` from both the weak default + the strong overrides. Plain (non-const) `uint32_t` keeps the symbol as a regular load against the storage cell the linker picks — strong override wins.

**Verified 2026-05-18:** all three RISC-V Rust `rtos_e2e` variants PASS (pubsub 65.2s, service 40.2s, action 42.4s).

Original triage retained below for reference.

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

## Issue 155.B — FreeRTOS C `nros_support_init -1` ✅ FIXED 2026-05-18

**Root cause** (per the 155.B.2/.3 subitem already on this doc):
`nros-rmw-cffi`'s `ret_from_error` catch-all `_ => NROS_RMW_RET_ERROR`
swallowed `ConnectionFailed` / `Disconnected` from
`ZpicoError -> TransportError`, surfacing as
`Backend("rmw_ret error")` to the Rust caller and finally
`NROS_RET_ERROR (-1)` at the C log.

**Fix** (landed in earlier sibling commits): added
`NROS_RMW_RET_CONNECTION_FAILED = -18`, taught
`ret_from_error` to map both `ConnectionFailed | Disconnected
→ -18`, and `error_from_ret` to decode `-18 →
TransportError::ConnectionFailed`. End-to-end: zpico →
ConnectionFailed → NROS_RMW_RET_CONNECTION_FAILED →
ConnectionFailed → NROS_RET_NOT_FOUND at C-side log.

**Verified this session** (after forcing C/C++ fixture rebuild
— stale binaries from pre-fix compile masked the win):

  FreeRTOS  × C × {pubsub, service, action} — 3/3 PASS
  ThreadX-RISC-V × C   × {pubsub, service, action} — 3/3 PASS
  ThreadX-RISC-V × C++ × {pubsub, service, action} — 3/3 PASS

Acceptance reached. Below is the original triage for reference.

## Issue 155.B — FreeRTOS C `nros_support_init -1` [original triage]

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
- [x] **155.B.2/.3.** Picked path (a) — applied the
      Phase 154 ABI flip to the vtable round-trip itself
      (commits this session).

      Tracing showed the FreeRTOS path was losing info at
      `nros-rmw-cffi`'s `ret_from_error`: the catch-all `_
      => NROS_RMW_RET_ERROR` swallowed `ConnectionFailed`
      and `Disconnected` (the two variants `ZpicoError ->
      TransportError` maps zpico's `ZPICO_ERR_GENERIC` /
      `ZPICO_ERR_SESSION` into). Round-trip back through
      `error_from_ret` then surfaced
      `TransportError::Backend("rmw_ret error")` to the
      Rust caller, which `transport_error_to_ret` couldn't
      decode → catch-all `NROS_RET_ERROR` (-1).

      Fix:
        - Added `NROS_RMW_RET_CONNECTION_FAILED = -18` to
          both the Rust constants
          (`packages/core/nros-rmw-cffi/src/lib.rs`) and the
          C header (`include/nros/rmw_ret.h`).
        - `ret_from_error` now maps `ConnectionFailed |
          Disconnected → NROS_RMW_RET_CONNECTION_FAILED`.
        - `error_from_ret` decodes `-18 →
          TransportError::ConnectionFailed`.
        - End-to-end: zpico → ConnectionFailed →
          NROS_RMW_RET_CONNECTION_FAILED → ConnectionFailed
          → NROS_RET_NOT_FOUND (-4) at the C-side
          `nros_support_init` log.

      Verified (this session): FreeRTOS C listener test now
      logs `nros_support_init(...) -> -3`
      (`NROS_RET_INVALID_ARGUMENT`) — meaning the FreeRTOS
      backend rejects an argument BEFORE attempting the
      connection. This is a different failure mode than the
      hypothesised `ConnectionFailed`; useful actionable
      signal for the next debug session (likely a
      locator-format or node-name issue inside the zenoh-pico
      backend's argument validation).

      Mapping also covers `NoData → TRY_AGAIN`,
      `InvalidArgument / TopicNameInvalid / NodeNameNonExistent
      → INVALID_ARGUMENT`, `BadAlloc → FULL`, `Unsupported /
      LoanNotSupported → NOT_ALLOWED`, `IncompatibleQos /
      IncompatibleAbi → REJECTED`. Only `Backend(_)` /
      `BackendDynamic(_)` still collapse to -1 (path (b)
      would side-channel the carried string; left for a
      future session if it becomes worth it).

      Next: trace which argument the FreeRTOS zenoh-pico
      backend rejects — bisect via debug-stderr prints in
      `zpico_init` / `zpico_open`. Reclassify as a new
      155.B.4 sub-item if it grows beyond a one-line fix.

- [x] **155.B.4.** Root cause for the `-3
      (NROS_RET_INVALID_ARGUMENT)` from 155.B.3 traced
      and fixed (this session).

      Trace: `nros_support_init -> open_session ->
      nros_rmw_cffi_walk_init_section -> CffiRmw.open ->
      CffiSession::open -> get_vtable() -> InvalidArgument
      (registry empty)`. The registry was empty because
      `nros_app_register_backends()` was never called.

      Phase 128.C.2 had deleted the explicit
      `nros_app_register_backends()` call from
      `nros_support_init_named` under the assumption that
      `walk_init_section` / `linkme` would register every
      backend automatically. But the
      `nros_rmw_register_backend!` macro in
      `nros-rmw-zenoh/src/lib.rs:176` documents that
      `linkme` is a NO-OP on FreeRTOS, NuttX, Zephyr,
      ESP-IDF — leaving the explicit `register()` call as
      the only registration path. The sibling
      `nros_cpp_init` still calls
      `nros_app_register_backends()` for this exact reason
      (see `nros-cpp/src/lib.rs:467`).

      Fix: restore the explicit
      `nros_app_register_backends()` call to
      `nros_support_init_named`, mirroring `nros_cpp_init`.

      Verified: FreeRTOS C zenoh listener now reaches
      "Waiting for messages..." and the pubsub E2E receives
      messages 0..N. `test_rtos_pubsub_e2e
      Platform__Freertos lang_2_Lang__C` should now pass.

## Issue 155.C — FreeRTOS C++ service test, 0 responses ✅ FIXED 2026-05-18

**Root cause.** `LWIP_RAND()` in
`packages/boards/nros-board-mps2-an385-freertos/config/arch/cc.h`
mapped to libc `rand()` (default seed = 1). zenoh-pico's
FreeRTOS+lwIP `z_random_u64` (via vendor
`system/freertos/system.c`) routed through `LWIP_RAND` →
both server + client QEMU instances produced the SAME
session ZID `e241fd0c6a5e726ac3eebe2f7a5d0568`. zenohd
rejected the client's OpenSyn (duplicate peer ID,
`max_links=1`) → no zenoh session → no queries → all four
`Future::wait` calls timed out at 5 s each.

**Fix.** Route `LWIP_RAND()` through
`nros_platform_random_u32()` which reads `s_rng_state`
seeded by `nros_platform_freertos_seed_rng()` during board
init from the IP/MAC hash. Different IP/MAC per config →
different seed → different ZID.

Traced via tshark with built-in `Zenoh Protocol` dissector
on a QEMU `-object filter-dump` pcap pair (manual launcher,
not test harness). Confirmed via pcap diff before/after.

Verified: full FreeRTOS C++ matrix now 3/3 PASS
(pubsub + service + action).

## Issue 155.C — FreeRTOS C++ service test, 0 responses [original triage retained for reference]

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

- [x] **155.C.1.** Captured both server and client side.
      **Client** (via `nextest --no-capture`):
      `nros C++ Service Client (FreeRTOS) / Node created /
      Service client ready / Call [1] failed: -2 (Timeout)`.
      **Server**: `nros::init(&app.support, locator,
      domain_id) -> -100` (`NROS_CPP_RET_TRANSPORT_ERROR`)
      — server never reaches "Waiting for requests", so
      every client call times out. Same root-cause shape
      as 155.B's C-side `nros_support_init -> -1`:
      `nros_cpp_init` collapsed every `NodeError` from
      `CppExecutor::open` to TRANSPORT_ERROR with no
      indication which variant tripped.
      Fix: `packages/core/nros-cpp/src/lib.rs`
      `nros_cpp_init` now decodes via a new
      `node_error_to_cpp_ret(NodeError)` helper —
      NameTooLong → INVALID_ARGUMENT; BufferTooSmall →
      FULL; Timeout → TIMEOUT; NotInitialized → NOT_INIT;
      RequestInFlight → REENTRANT; Transport variants
      mapped further (ConnectionFailed/Disconnected stay
      TRANSPORT_ERROR; Timeout/WouldBlock → TIMEOUT;
      InvalidConfig → INVALID_ARGUMENT; Buffer/Message/
      TooLarge → FULL; rest → TRANSPORT_ERROR). Pairs
      with Phase 155.B.1's C-side mapping so all three
      `nros::init` / `nros_support_init` paths surface
      specific codes. Next FreeRTOS C++ + RV64 C++ run
      logs identify which precondition the backend
      rejected; 155.C.2 / .C.3 branch on that code.
      Server stdout split still TODO at the test-harness
      level (RtosProcess enum, `start_pair`); the
      current diagnostic uses single-instance reruns
      (`cargo nextest run … service_e2e … --no-capture`)
      with one process at a time.
- [~] **155.C.2.** Ran zenohd with `ZENOHD_LOG=debug`
      (`/tmp/zenohd-7661.log`). Key observations:
        - Server connects, registers
          `0/add_two_ints/example_interfaces::srv::dds_::AddTwoInts_/TypeHashNotSupported`,
          declares queryable. Looks correct.
        - Client connects ~20 s later (matches stabilization
          delay). Holds TCP connection until test kills it.
        - **No query messages appear in zenohd log between
          connect and disconnect.** Either client never
          sends `zp_get`, OR zenoh-pico's per-query framing
          fails before reaching the wire, OR zenohd drops
          them silently below DEBUG.
      Next: tshark with built-in `Zenoh Protocol` dissector
      on a QEMU `-object filter-dump` pcap to see whether
      bytes actually leave the client's TCP socket.
      Requires modifying `QemuProcess::start_mps2_an385_networked`
      to use `-netdev user,id=net0 -object filter-dump,
      id=f0,netdev=net0,file=…pcap -device lan9118,netdev=net0`
      instead of legacy `-nic user,model=lan9118`.
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
- [x] **155.D.3.** Replicated the env-export pattern in
      `just/threadx-linux.just` + `just/freertos.just`
      `build-fixtures` recipes for parity (this session).
      - `threadx-linux` exports `THREADX_DIR`,
        `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR`
        before the cmake configure + build loop. Symmetric
        with the existing 155.D RV64 export block.
      - `freertos` exports `FREERTOS_DIR`, `FREERTOS_PORT`,
        `LWIP_DIR`, `FREERTOS_CONFIG_DIR` with the
        repository defaults if the caller hasn't set them.
      Both recipes already passed `-D` for the same vars to
      cmake; the exports ensure cmake-driven cargo
      invocations (corrosion + cc-rs) read the right values
      from process env, not just cmake cache.
      Other cmake-driven cargo recipes audited and found
      symmetric: `nuttx` build-fixtures doesn't have config-
      dir env-vars to leak (NuttX uses NUTTX_DIR which the
      recipe already exports inline).

**Acceptance — partial.** `just threadx_riscv64 build-fixtures`
now passes the env-leak failure point. Hits next-layer issue:
`nxd_bsd.h: conflicting types for 'suseconds_t'` when bare-
metal compile pulls newlib / picolibc `suseconds_t` against
NetX-Duo's own typedef. Spun into Issue 155.E (header guard /
typedef conflict).

**Verified 2026-05-18 ✅ FIXED.** RISC-V cmake-built C / C++ E2E
3/3 + 3/3 PASS end-to-end (see 155.E acceptance below).

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

**Verified 2026-05-18 ✅ FIXED.** Full RISC-V 9/9 matrix PASS:

  Rust × {pubsub, service, action} — 3/3 (pubsub 65.2s, service 40.2s, action 42.2s)
  C    × {pubsub, service, action} — 3/3 (pubsub 65.2s, service 40.3s, action 42.3s)
  C++  × {pubsub, service, action} — 3/3 (pubsub 65.2s, service 40.3s, action 50.2s)

Closes 155.D + 155.E together — the runtime gap was the same
`NROS_RET_ERROR` swallow as 155.B; 155.B's `ConnectionFailed`/
`Disconnected` → `NROS_RMW_RET_CONNECTION_FAILED → -18` mapping
unblocked RISC-V's C / C++ paths once fixtures rebuilt.

## Issue 155.F — NuttX rtos_e2e ✅ Rust 3/3 + C 2/3 (action preexisting) + C++ 3/3

**Status:** Rust 3/3 PASS. C 2/3 PASS (pubsub + service; action FAIL is preexisting Phase 77 async-action-client work, NOT a sub-bug of this phase). C++ 3/3 PASS.

### Sub-bug F1 — Rust `Transport(ConnectionFailed)` immediately after `nros::init` ✅ FIXED

**Symptom:** `nros NuttX platform starting (IP: 10.0.2.31, zenoh: tcp/10.0.2.2:7452)` then `Application error: Transport(ConnectionFailed)`. Slirp gateway + zenohd reachable; same ZID-collision-style fail mode as 155.C, but on NuttX.

**Root cause:** Identical to Phase 156 Sub-bug B (POSIX) but undetected for NuttX. `zenoh-pico/system/common/platform.h` line 28 routes `ZENOH_NUTTX` through `system/platform/unix.h`, which uses BY-VALUE `_z_sys_net_socket_t = { int _fd; }` (4 bytes, single-register). `packages/zpico/zpico-sys/c/zpico/platform_aliases.c`'s network wrappers expect the 32-byte opaque struct from `nros_zenoh_generic_platform.h`. ABI mismatch → `_z_send_tcp` reads garbage `fd` + `len` off the stack → `send()` returns EBADF → ConnectionFailed.

**Fix:**
- `packages/zpico/zpico-sys/build.rs` — extend the `NROS_ZENOH_PLATFORM_USES_UNIX` gate from `use_posix` to `use_posix || use_nuttx`.
- `packages/zpico/zpico-sys/zenoh_platforms.toml` — add `{ path = "{src}/system/unix/network.c" }` to `[platform.nuttx].extra_sources` so the upstream BY-VALUE TCP/UDP impls compile in (matches unix.h's 4-byte socket shape).

NuttX Rust pubsub/service/action all PASS after rebuild.

### Sub-bug F2 — C examples `nros_config_generated.h must be supplied per-build` ✅ FIXED

**Symptom:** Every NuttX C example fails to compile with `#error "nros_config_generated.h must be supplied per-build by the build system; see this stub for guidance."` — picked up from the source-tree stub at `packages/core/nros-c/include/nros/nros_config_generated.h`.

**Root cause:**
1. Carrier `add_executable(<name>)` target in the example CMakeLists.txt was being linked by cmake with the *host* toolchain (x86_64 gcc), failing with `undefined reference to main` (NuttX example registers `void app_main(void)` via NROS_APP_MAIN_REGISTER_VOID, not `int main`). Only the cargo-built `<name>_build` produces the real NuttX kernel ELF.
2. `nros-nuttx-ffi/build.rs` added the source-tree `packages/core/nros-c/include` to cc-rs's include path BEFORE the per-build mirror at `<build_dir>/nano_ros/packages/core/nros-c/include` arrived via APP_INCLUDE_DIRS_FILE. The source-tree stub shadowed the real generated header.

**Fix:**
- `cmake/board/nano-ros-board-nuttx-qemu-arm.cmake` — in `nros_board_link_app`, after redispatching through `nros_nuttx_build_example`, set `EXCLUDE_FROM_ALL TRUE` on the carrier `add_executable` target and `add_dependencies(<target> <target>_build)`. Default build no longer tries to host-link the carrier; explicit `cmake --build . --target <target>` still produces the kernel ELF via cargo.
- `packages/boards/nros-board-nuttx-qemu-arm/nros-nuttx-ffi/build.rs` — apply `CARGO_TARGET_DIR/nros-{c,cpp}-generated/` includes first, then APP_INCLUDE_DIRS_FILE entries with source-tree `packages/core/nros-{c,cpp}/include` entries deferred to the end. Per-build mirrors win; source-tree fallback still provides hand-written headers (`nros/app_main.h` etc.).

NuttX C pubsub + service PASS. NuttX C action FAILS on `accepted=false, completed=false` — preexisting Phase 77 async-action issue tracked separately.

### Sub-bug F3 — C++ examples blocked on corrosion cross-compile ✅ FIXED 2026-05-18

**Symptom:** C++ examples fail to link with `libnano_ros_cpp_ffi_<pkg>.a: file format not recognized` — host x86_64 objects in an ARM link. Codegen's `nano_ros_cpp_ffi_<pkg>` cargo build in `packages/codegen/.../NanoRosGenerateInterfaces.cmake:466` only emits the `+nightly` + `-Zbuild-std=core` path if `Rust_CARGO_TARGET MATCHES "nuttx"`, but `nros_nuttx_set_cargo_target` published the value via PARENT_SCOPE — which doesn't cross the `add_subdirectory(<repo-root>)` boundary, so the example's top scope never saw it.

**Root cause:** Three independent gaps stacked:
1. **Rust_CARGO_TARGET propagation.** PARENT_SCOPE walks the include chain, not add_subdirectory'd scopes. The codegen `nros_generate_interfaces()` call in the example scope read `Rust_CARGO_TARGET` as unset and emitted a host-target cargo build of the cpp FFI codegen lib.
2. **Build ordering.** The `_build` custom target ran `nros-nuttx-ffi`'s cargo invocation in parallel with corrosion's `cargo-build_nros_cpp`. Without a dep, main.cpp compile beat the per-build `nros_cpp_config_generated.h` mirror that nros-cpp's POST_BUILD writes, and the source-tree `#error` stub won.
3. **Stale `Error 2` from prior failed build attempts** masked successful re-builds.

**Fix:**
- `cmake/board/nano-ros-board-nuttx-qemu-arm.cmake` — after `nros_nuttx_set_cargo_target`, also publish `Rust_CARGO_TARGET` as CACHE so it reaches the add_subdirectory'd example scope. Safe to set after the corrosion `add_subdirectory(nros-c/nros-cpp)` calls in the root CMakeLists.txt have already run (corrosion sees the unset value and builds for host; the resulting host .a is never linked into the NuttX kernel ELF — that link goes through `nros_nuttx_build_example`'s cargo invocation that cross-builds every nros-* crate via the FFI crate's path-deps).
- `packages/core/nros-c/cmake/nros-nuttx.cmake` — in `nros_nuttx_build_example`, `add_dependencies(${name}_build cargo-build_nros_c cargo-build_nros_cpp)` so the per-build `nros_{,cpp_}config_generated.h` mirrors complete before the FFI cargo invocation reads them.

**Verified 2026-05-18:** all 12 NuttX cmake fixtures (6 × C + 6 × C++) build cleanly in a single `cmake --build` pass. NuttX C++ E2E 3/3 PASS (pubsub 45.3s, service 30.4s, action 30.4s).

## Notes

These four are mutually independent (different root causes,
different file sets). They can land in any order in any number
of follow-up sessions; bundling here just for tracking.

Phase 154 itself stays closed — its acceptance was "ThreadX-Linux
Rust E2E unblocked" which passed. The four items above are
follow-ups, not regressions of 154.
