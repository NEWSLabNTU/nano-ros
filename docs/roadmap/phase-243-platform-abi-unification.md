# Phase 243 — Platform ABI unification (one `<nros/platform.h>`)

**Implements.** [RFC-0042](../design/0042-platform-build-determinism.md) D1, the
B.3 step of [phase-241](phase-241-platform-build-determinism.md). Carved out
because B.3 grew from "delete the duplicate" into a real **ABI unification of two
live platform headers** — its own coherent, higher-risk unit.

**Goal.** Collapse the two live C platform ABIs that both answer to the include
name `<nros/platform.h>` into ONE canonical header (owned by `nros-platform-api`,
landed in B.1/B.2), then **delete the legacy `nros-c/include/nros/platform.h` + its
per-RTOS sub-headers (A)**. Ends the split-brain where POSIX/native resolve A
(direct-libc heap, ns-clock, per-platform atomics) while Zephyr/xrce/zpico resolve
api (alloc funnel, ms/us-clock).

**Status.** **LANDED on main (2026-06-13 reconciliation):** `nros-c/include/nros/
platform.h` + its per-RTOS sub-headers are deleted on main (`git ls-files | grep -c
nros-c/include/nros/platform` = 0) — the one canonical `<nros/platform.h>` is in
`nros-platform-api`. Waves 243.1/.2/.3/.5 landed; e2e was green on **5/6 cells**
(qemu, esp32, freertos, threadx_linux, threadx_riscv64). **nuttx red is the
pre-existing `240.6` regression** (undefined Rust `nros_platform_*` link symbols —
identical on clean main), NOT this phase. **243.6 B.4 confirmed (2026-06-13):**
`c_stub_platform.rs` compiles clean on main → the canonical-header ↔
`nros-platform-cffi/src/lib.rs` parity guard is intact post-collapse.
Three en-route fixes landed (each its own commit):
- `zpico.c` `_freertos_printk`: arch-guard the ARM semihosting asm (`__arm__`/
  `__thumb__`) so the freertos-config TU survives a host compile.
- api capability block: grant `HAS_MALLOC` for **all non-bare-metal** (matches A's
  default→posix.h fallback) — `-DNROS_PLATFORM_THREADX` (threadx-linux), nuttx, etc.
- threadx-rv64 `cxx-compat`: add a freestanding `<initializer_list>` (the 242.3
  `parameter.hpp` include reached by 240.6's typed-component cpp examples).
243.4 (delete the now-dead board/shim `time_ns`/atomics strong-defs) deferred as
harmless cleanup — e2e proves nothing references them.

**Priority.** P2 — finishes the RFC-0042 D1 canonical-interface goal. B.2 already
removed the cffi duplicate; this removes the *second* surface so there is exactly
one `<nros/platform.h>`.

**Depends on.** B.1/B.2 (the api canonical header + the cffi→api rewire, landed);
the 241.A gate + `c_stub_platform.rs` parity (guards); a `run_e2e` dispatch.

---

## What A uniquely provides (the merge surface)

From the B.3 investigation (file:line in phase-241 B.3 / the RFC):
- **atomics** `nros_platform_atomic_store_bool` / `load_bool` — per-platform inline
  (posix `__atomic`, zephyr `atomic_set`, freertos critical-section, baremetal
  barrier). **Live**: `nros-c/src/guard_condition.rs:205,226,239` (via the
  `nros-c/src/platform.rs` Rust wrappers); boards + custom-platform IMPLEMENT.
- **ns-clock** `nros_platform_time_ns` / `sleep_ns` — per-platform inline (posix
  `clock_gettime`, …) + extern on baremetal. **Live**: the `get_time_ns()`/
  `sleep_ns()` wrappers in `platform.rs`, called ~20× (executor/service/action/
  clock); `nros-rmw-cyclonedds/src/internal.hpp:27,63`; `nros-cpp/src/lib.rs:1251-1254`.
- **typed mutex** `nros_mutex_t` + `mutex_{init,lock,unlock,destroy}` — **DEAD**
  (no caller; `NROS_FEATURE_THREADS` never `-D`'d; smoke tests use api's `void*`
  mutex). Drop.
- **baremetal-only utils** `NROS_MEMORY_BARRIER`, `nros_platform_disable_irq`/
  `restore_irq` — **DEAD** (zero consumers outside `baremetal.h`'s own atomics).
  Drop with A.

api lacks only atomics + ns-clock; everything else (clock_ms/us, alloc/dealloc,
realloc, sleep_*, yield, random, wall-clock, tasks, recursive mutex, condvars,
wake, critical-section, log, capability macros) it already has (incl. the B.1
capability-default block).

Insulation that makes this tractable: every Rust caller goes through the
`nros-c/src/platform.rs` wrappers (`get_time_ns`, `sleep_ns`, `atomic_store_bool`,
`atomic_load_bool`), so re-pointing the **wrapper bodies** migrates all ~20 call
sites at once.

---

## Work items (waves)

Ordered so the gate guards from W1, A's consumers are migrated before A is deleted
(W5), and the ABI change is validated end-to-end (W6). Each wave is revertible.

### 243.1 — api gains generic atomics (+ unblock the gate)
- [ ] Add to `nros-platform-api/include/nros/platform.h`: `static inline
      nros_platform_atomic_store_bool(bool*, bool)` = `__atomic_store_n(ptr, v,
      __ATOMIC_RELEASE)`; `…load_bool(const bool*)` = `__atomic_load_n(ptr,
      __ATOMIC_ACQUIRE)`. One impl — valid on every target nros-c builds (1-byte
      bool, no CAS; riscv32imc never builds nros-c). Header-only inline (no Rust
      mirror entry needed).
- [ ] **B.5:** `platform_header_matrix.rs` — add `nros-platform-api/include` as the
      first `-I`; the `core-no-malloc` (atomics) cell now passes against api.
- **Acceptance:** the 241.A gate is green resolving the api header (atomics + the
      #38 cells).

### 243.2 — migrate the Rust platform wrappers off A (one file)
- [ ] `nros-c/src/platform.rs`: in the no_std path, reimplement `get_time_ns()` over
      `nros_platform_clock_us() * 1000` and `sleep_ns(ns)` over
      `nros_platform_sleep_us(ns / 1000)`; reimplement `atomic_store_bool`/
      `atomic_load_bool` over `core::sync::atomic::AtomicBool`. Remove the
      `extern "C"` decls for `time_ns`/`sleep_ns`/`atomic_*` (lines 19-31); add
      externs for `clock_us`/`sleep_us` if not already reachable. `guard_condition.rs`
      + the ~20 timing callers are unchanged (they use the wrappers).
- **Acceptance:** `cargo check -p nros-c --no-default-features` on the core-libs
      bare targets (thumbv7m, riscv32imc) passes; no reference to the A externs.

### 243.3 — migrate the C++ self-externs to api's clock
- [ ] `nros-rmw-cyclonedds/src/internal.hpp:27,63` + `nros-cpp/src/lib.rs:1251-1254`:
      replace the self-declared `extern "C" nros_platform_time_ns` (used as
      `… / 1e6` → ms) with `nros_platform_clock_ms()` directly.
- **Acceptance:** cyclonedds + nros-cpp compile (host + a zephyr/threadx cell).

### 243.4 — delete the now-orphaned ns-clock / atomics impl defs
- [x] **Boards/shims done (on main).** The `nros_platform_time_ns`/`sleep_ns` +
      `atomic_store_bool`/`load_bool` strong defs are gone from the zephyr shim,
      `nros-board-mps2-an385-freertos/startup.c`, and
      `nros-board-threadx-qemu-riscv64/startup.c` — repo-wide the only remaining
      real defs of these symbols are api's canonical `static inline` atomics
      (`nros-platform-api/include/nros/platform.h`) + the custom-platform example.
- [ ] **Residual: the `custom-platform` example is stale doc-debt, not a quick
      delete.** `examples/native/c/custom-platform/src/platform_impl.c` implements
      *only* the four now-dead/header-inline functions (`time_ns`, `sleep_ns`,
      `atomic_{store,load}_bool`) — it teaches a retired ABI. The current
      custom-platform surface is ~40 functions (`clock_us`, `alloc`/`realloc`/
      `dealloc`, `sleep_*`, `yield`, `random_fill`, the task/mutex/condvar/wake
      API). The example needs a **re-author to the current ABI** (or a redesign /
      retirement decision), not just a def deletion. Not CI-gated; tracked here.
- **Acceptance:** each board builds against api (atomics inline + clock wrappers).

### 243.5 — retire A + repoint the include order
- [ ] Delete `nros-c/include/nros/platform.h` and `nros-c/include/nros/platform/
      {posix,zephyr,freertos,baremetal}.h`.
- [ ] Repoint include order so POSIX/native resolve api (not the deleted A):
      `nros-c/CMakeLists.txt:134-137` (the `nros_c-static` INTERFACE) + the
      top-level `NanoRos` interface (`CMakeLists.txt:131-134`) must list
      `nros-platform-api/include` **first**.
- [ ] POSIX malloc/free now funnels through `nros_platform_alloc` (the posix port
      provides it — `nros-platform-posix/src/platform.c:53,68`), ending the
      direct-libc divergence (RFC-0034 D6).
- **Acceptance:** `git ls-files | grep -c nros-c/include/nros/platform` = 0; every
      `<nros/platform.h>` consumer resolves the one canonical api header.

### 243.6 — parity + full validation
- [x] **B.4 (confirmed 2026-06-13):** `c_stub_platform.rs` compiles clean on main,
      so it still gates the canonical header ↔ `nros-platform-cffi/src/lib.rs` (the
      atomics are inline → not mirrored; the rest is unchanged).
- [ ] Branch + `run_e2e` dispatch: all per-platform cells green (the POSIX-funnel +
      atomics + clock change touches every platform). Accept the pre-existing nuttx
      `240.6` red as out-of-scope (cross-ref).
- **Acceptance:** gate + parity + e2e green (nuttx modulo 240.6); RFC-0042 D1 flips
      toward `Stable` (one `<nros/platform.h>`).

## Risks / notes

- **ABI change, every platform.** Unlike B.2 (include-path move), this changes the
  POSIX heap funnel, the atomics provider, and the clock API consumers — so a full
  `run_e2e` is mandatory, on a branch, before merge.
- **Precision.** `time_ns` → `clock_us*1000` loses sub-µs precision; the consumers
  are ms-scale spins/deadlines (executor/service/action), so µs is ample. If a
  future RT path needs ns, add a dedicated `clock_ns` to the canonical ABI rather
  than resurrect A.
- **Ordering.** W1 before everything (gate guard); A deleted LAST (W5), after every
  consumer is migrated (W2-W4). W2 is the linchpin — the wrapper layer collapses
  ~20 call sites into one edit.
- Keep RFC-0034 (allocator funnel) + RFC-0035 (vtable ABI) invariant — this changes
  the *platform header surface*, not those ABIs.
