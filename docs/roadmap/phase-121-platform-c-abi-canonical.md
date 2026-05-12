# Phase 121 — Platform C ABI Canonical + Crate Migration

**Goal:** Promote the C ABI declared in `<nros/platform.h>` to the canonical platform interface. Every platform port — current Rust crates and future C-native ports — provides the same flat set of `extern "C"` symbols. The Rust `nros_platform_api` traits stay as the ergonomic Rust surface, dispatched through `CffiPlatform` for cffi consumers. Rust platform crates expose the C ABI in-place via an `export_platform!` macro from `nros-platform-cffi`, gated behind each crate's own `cffi-export` feature — no sibling crates.

**Status:** 121.1 (header + Rust mirror), 121.2.a (macros), 121.2.{posix,freertos,nuttx,threadx,zephyr} (RTOS crate wiring), 121.2.wire-feature (nros-platform fan-out), and 121.4 (drift gate + C-stub harness + POSIX parity test) all landed. Remaining work: 121.2.embedded (five bare-metal / FSP crates at `packages/platforms/`); 121.2.rtic (future, on-demand); 121.3.* (C-native ports per RTOS, long arc); 121.5 (porter / internals docs); per-RTOS parity tests for freertos/nuttx/threadx/zephyr (require cross toolchains, deferred).

**Priority:** Medium. Not blocking active features. Unblocks (a) writing a platform port in C/C++/Zig without touching Rust, (b) sharing one ABI across the project's language surfaces, (c) eventually rehosting RTOS-native platform code (Zephyr, FreeRTOS, NuttX, ThreadX, ESP-IDF) in the SDK's native language so each port reads idiomatically to its kernel community.

**Depends on:**
- Phase 79 (unified platform abstraction) — Complete
- Phase 102 (`nros-rmw-cffi` C vtable) — Complete; same canonical-C-ABI rubric applied here

**Out of scope (deferred):**
- Re-implementing existing Rust platform crates in C in this phase. That is the long arc 121.3.* tracks; the immediate work is exposing the Rust impls through the canonical ABI.
- A C platform port that supersedes a Rust crate. When a C port lands, the corresponding Rust crate may be deprecated, but no Rust crate is removed in this phase.

---

## Overview

### Why a canonical C ABI for the platform tier

The platform abstraction sits at the lowest layer of nano-ros: ~45 free functions covering clock, alloc, sleep, yield, random, wall-clock time, tasks, mutexes (recursive + non-recursive), and condition variables. Every RTOS we target has a native implementation already — in C, because the kernels (Zephyr, FreeRTOS, NuttX, ThreadX, ESP-IDF) are themselves C. The existing Rust platform crates wrap that C surface; their value is providing the trait surface to Rust callers, not the wrapping itself.

If the **C ABI** is canonical:

- C-native ports skip Rust entirely. A Zephyr engineer writes a `nros_platform_zephyr.c` that declares the symbols and links directly against `nros-platform-cffi`.
- Rust ports stay single crates. Each gains a `cffi-export` feature that invokes a declarative macro from `nros-platform-cffi` on the crate's trait-implementing ZST. The macro emits the full set of `#[unsafe(no_mangle)] extern "C"` symbols in-place.
- One header is the single source of truth for documentation, signatures, and ABI versioning. cbindgen is no longer involved.

### Why free symbols (not a vtable struct)

The Phase 117 RMW ABI uses a runtime-pluggable vtable struct + `nros_rmw_register()` call because RMW backends genuinely swap at runtime (zenoh vs cyclonedds vs xrce within one binary across different test sessions). The platform abstraction is fixed for the life of a binary; there is no runtime swap. Free `extern "C"` symbols capture exactly that property — link-time resolution, zero indirection, no register call, no atomic-pointer load per dispatch.

The shape difference is intentional and is documented in `docs/design/portable-rmw-platform-interface.md`.

---

## Architecture

```
                              ┌──────────────────────────────┐
                              │ Rust caller                  │
                              │  (uses PlatformClock, etc.)  │
                              └──────────────┬───────────────┘
                                             │
                              ┌──────────────▼───────────────┐
                              │ nros-platform-api (traits)   │
                              └──────────────┬───────────────┘
                                             │
                  ┌──────────────────────────┴────────────────────────────┐
                  │                                                       │
   ┌──────────────▼───────────────┐                          ┌────────────▼─────────────┐
   │ Native Rust impl             │                          │ CffiPlatform (in         │
   │  (e.g. nros-platform-posix)  │                          │  nros-platform-cffi)     │
   │  impl PlatformClock for ...  │                          │  impl PlatformClock for  │
   │  #[cfg(feature="cffi-       │                          │   CffiPlatform           │
   │      export")]               │                          │                          │
   │  nros_platform_cffi::        │                          │                          │
   │      export_platform!(Self); │                          │                          │
   └──────────────┬───────────────┘                          └────────────┬─────────────┘
                  │ macro expands to                                      │ unsafe extern "C"
                  │ ~45 #[no_mangle] extern "C"                           │ {
                  │ fns delegating to trait                               │   nros_platform_clock_ms()
                  │                                                       │ }
                  └──────────────────────┬────────────────────────────────┘
                                         │
                            ┌────────────▼──────────────┐
                            │ <nros/platform.h>         │
                            │  CANONICAL C ABI          │
                            │  ~45 free extern C        │
                            │  symbols                  │
                            └────────────▲──────────────┘
                                         │
                            ┌────────────┴──────────────┐
                            │ C-native port            │
                            │  (future: zephyr.c,      │
                            │   freertos.c, nuttx.c,   │
                            │   threadx.c)             │
                            └───────────────────────────┘
```

The header is the contract. Both the macro-exported Rust path and future C-native ports supply the same symbol set.

---

## Work Items

### 121.1 — Canonical header + Rust mirror

- [x] **121.1.a** — Hand-write `packages/core/nros-platform-cffi/include/nros/platform.h` listing ~45 free `extern "C"` functions (clock, alloc, sleep, yield, random, time, tasks, mutex non-rec + rec, condvar). Include `nros_platform_ret_t` typedef + `NROS_PLATFORM_RET_OK / _ERROR / _UNSUPPORTED` macros.
- [x] **121.1.b** — Rewrite `packages/core/nros-platform-cffi/src/lib.rs`:
  - drop `NrosPlatformVtable` struct + `nros_platform_cffi_register` + `AtomicPtr<NrosPlatformVtable>` registry;
  - add `unsafe extern "C" { … }` block mirroring the header;
  - `CffiPlatform` trait impls dispatch directly to the extern symbols;
  - add `#[cfg(test)] mod test_stubs` supplying `#[unsafe(no_mangle)] extern "C"` defaults so `cargo test -p nros-platform-cffi` links.
- [x] **121.1.c** — Drop cbindgen: delete `build.rs`, `cbindgen.toml`, the cbindgen build-dep, and the now-unused `portable-atomic` runtime dep.
- [x] **121.1.d** — Refresh docs: `README.md`, `docs/mainpage.md`, `Doxyfile` (`INPUT = include/nros/platform.h`), `book/src/porting/custom-platform.md` C/C++ path, `docs/design/portable-rmw-platform-interface.md` R2 section.

**Files:**
- `packages/core/nros-platform-cffi/include/nros/platform.h` (new)
- `packages/core/nros-platform-cffi/include/nros/platform_vtable.h` (deleted)
- `packages/core/nros-platform-cffi/src/lib.rs`
- `packages/core/nros-platform-cffi/Cargo.toml`
- `packages/core/nros-platform-cffi/build.rs` (deleted)
- `packages/core/nros-platform-cffi/cbindgen.toml` (deleted)
- `packages/core/nros-platform-cffi/README.md`
- `packages/core/nros-platform-cffi/docs/mainpage.md`
- `packages/core/nros-platform-cffi/Doxyfile`
- `book/src/porting/custom-platform.md`
- `docs/design/portable-rmw-platform-interface.md`

**Acceptance:** `just check` + `cargo test -p nros-platform-cffi` pass; `<nros/platform.h>` opens cleanly under `-Wpedantic -Werror`; no consumer outside the crate referenced the deleted header.

---

### 121.2 — In-crate macro export from platform-trait impls

Instead of one sibling `-cffi` shim crate per RTOS, ship a declarative macro from `nros-platform-cffi` that any platform crate invokes (under a `cffi-export` feature) on its own trait-implementing ZST. The macro emits the full set of `#[unsafe(no_mangle)] extern "C"` symbols declared in `<nros/platform.h>`, each delegating to the corresponding trait method on the supplied type. One source of truth for the symbol set; zero per-RTOS boilerplate; symbol-set drift becomes structurally impossible because adding an ABI symbol means editing exactly three things in `nros-platform-cffi` (the header, the `unsafe extern "C"` mirror, the macro emission).

Carve the macro by capability so consumers that lack a capability (bare-metal without threading, say) opt in selectively:

- `nros_platform_cffi::export_clock!($ty)`
- `nros_platform_cffi::export_alloc!($ty)`
- `nros_platform_cffi::export_sleep!($ty)`
- `nros_platform_cffi::export_yield!($ty)`
- `nros_platform_cffi::export_random!($ty)`
- `nros_platform_cffi::export_time!($ty)`
- `nros_platform_cffi::export_threading!($ty)`
- `nros_platform_cffi::export_platform!($ty)` — convenience wrapper that calls all of the above (the common case).

Macro emission (illustrative):

```rust
#[macro_export]
macro_rules! export_clock {
    ($ty:ty) => {
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_clock_ms() -> u64 {
            <$ty as ::nros_platform_api::PlatformClock>::clock_ms()
        }
        #[unsafe(no_mangle)]
        pub extern "C" fn nros_platform_clock_us() -> u64 {
            <$ty as ::nros_platform_api::PlatformClock>::clock_us()
        }
    };
}
```

Caller-side (per platform crate):

```rust
#[cfg(feature = "cffi-export")]
nros_platform_cffi::export_platform!(crate::PosixPlatform);
```

Trait-bound failures at the macro call site produce a clear compile error pointing at the platform crate — exactly the drift gate the design wants. Sibling crates would have caught the same drift at link time, one symbol at a time; the macro catches it at compile time, all at once.

- [x] **121.2.a** — Authored the `nros_platform_export_*!` macro family in `nros-platform-cffi/src/lib.rs`. Eight macros (one per capability + a convenience `nros_platform_export!`) cover every symbol declared in `<nros/platform.h>`. Macro emission lives in the caller crate; `nros-platform-cffi` itself never invokes the macros (it would emit symbols and collide with whichever platform crate is also exporting).
- [x] **121.2.posix** — `nros-platform-posix` gained a `cffi-export` Cargo feature; `src/lib.rs` invokes `nros_platform_cffi::nros_platform_export!(PosixPlatform)` under it.
- [x] **121.2.freertos** — Same for `nros-platform-freertos`.
- [x] **121.2.nuttx** — Same for `nros-platform-nuttx`.
- [x] **121.2.threadx** — Same for `nros-platform-threadx`.
- [x] **121.2.zephyr** — Same for `nros-platform-zephyr`.
- **121.2.embedded** — five embedded / bare-metal crates at `packages/platforms/`. Each already ships a complete `PlatformThreading` impl (either stubs for true bare-metal, or a delegating impl for FreeRTOS-backed targets), so the standard `nros_platform_export!` macro compiles against them as-is. No per-capability split, no stub-emission macros needed. Wire the same way as the RTOS five:
  - [ ] **121.2.mps2-an385** — bare-metal Cortex-M3. `PlatformThreading` is stubbed (mutex/condvar return 0, task_init returns -1). Single-core no-preempt makes the stubs correct.
  - [ ] **121.2.stm32f4** — bare-metal Cortex-M4F. Same stub-threading pattern.
  - [ ] **121.2.esp32** — Xtensa, single-threaded bare-metal config (source comment: "single-threaded bare-metal, all no-ops"). Same pattern.
  - [ ] **121.2.esp32-qemu** — QEMU variant of the above.
  - [ ] **121.2.orin-spe** — Cortex-R5 / FreeRTOS FSP. Trait impls delegate to `FreeRtosPlatform`. **Mutually exclusive with `nros-platform-freertos/cffi-export`** in the same binary: both would emit `#[no_mangle]` symbols for the same names. Pick one per binary; `nros-platform`'s `cffi-export` fan-out already prevents accidental double-emission because only one platform feature is active per build.
- [ ] **121.2.rtic** — future scope. RTIC apps use priority-ceiling locks via `critical_section`, not kernel threads. A `nros-platform-rtic` crate (does not exist yet) would impl `PlatformThreading` with stubs identical in shape to the bare-metal crates, then enable `cffi-export` the same way. Track here so the assumption is recorded; defer crate authorship until a downstream RTIC user appears.
- [x] **121.2.wire-feature** — `nros-platform`'s `cffi-export` feature is the fan-out point; turning it on alongside any `platform-<rtos>` (or future `platform-<embedded>`) feature activates the corresponding crate's `cffi-export`. Orthogonal to `platform-cffi`, which selects `CffiPlatform` as `ConcretePlatform`.

**Files (per crate):**
- `packages/core/nros-platform-cffi/src/lib.rs` (macro definitions, one-time)
- `packages/core/nros-platform-<rtos>/Cargo.toml` or `packages/platforms/nros-platform-<board>/Cargo.toml` (add `cffi-export` feature + optional `nros-platform-cffi` dep)
- `packages/core/nros-platform-<rtos>/src/lib.rs` or `packages/platforms/nros-platform-<board>/src/lib.rs` (one feature-gated macro invocation)
- `packages/core/nros-platform/Cargo.toml` (fan-out entry under `[features].cffi-export`)

**Acceptance:** per crate, `cargo build -p nros-platform-<name> --features cffi-export` succeeds (host toolchain for POSIX/threadx-linux, cross toolchain for the embedded crates); the per-platform parity test (121.4.c) verifies macro emission covers the full header for the host-runnable case (POSIX done; freertos/nuttx/threadx/zephyr/embedded deferred to follow-up since the linker needs cross targets).

**Why this beats sibling crates:**
- One symbol-set definition (the macro) instead of N copies across N shim crates.
- A new ABI symbol lands in three places inside one crate, not N+3.
- Trait-bound check at macro expansion is a stronger drift gate than link-time symbol resolution.
- No extra crate to register in the workspace, no extra `Cargo.toml` to maintain per platform.

**Trade-offs accepted:**
- Macro expansion adds ~45 items to the platform crate's compile unit when `cffi-export` is on. Compile cost is negligible; debugger stack frames may show macro-expansion line numbers.
- A consumer that wanted to substitute a single symbol (override `nros_platform_random_u32` with a hardware-RNG variant while inheriting everything else) cannot easily do so — they would have to fork the macro emission. Unlikely to matter; if it ever does, add an `export_platform_except!($ty, [random_u32])` variant.

---

### 121.3 — C-native platform ports (long arc)

Replacing each Rust platform crate with a hand-written C port against the host RTOS's idiomatic API. The result is a tiny C file (or directory) that each kernel's contributor community can read at a glance.

These are independent of 121.2 — 121.2 unblocks Rust callers immediately via macro export, 121.3 lets contributors who don't write Rust ship a port directly against the canonical ABI. A C port and the macro-exported Rust impl provide the same symbol set; only one may be linked into a given binary.

- [ ] **121.3.posix** — POSIX C port (`platform_posix.c`). Lowest cost — `clock_gettime`, `malloc`, `pthread_*` straight through. Strongest correctness target since POSIX is the default test bed.
- [ ] **121.3.freertos** — FreeRTOS C port. `xTaskGetTickCount`, `pvPortMalloc`/`vPortFree`, `xTaskCreate`, `xSemaphoreCreateRecursiveMutex`, condvar via counting semaphores.
- [ ] **121.3.nuttx** — NuttX C port. POSIX-shaped (uses `pthread_*`, `sem_timedwait`); large parts share with 121.3.posix.
- [ ] **121.3.threadx** — ThreadX C port. `tx_thread_*`, `tx_mutex_*`, `tx_event_flags_*` for condvar.
- [ ] **121.3.zephyr** — Zephyr C port. `k_uptime_get`, `k_thread_create`, `k_mutex_*`, `k_condvar_*` (Zephyr ≥ 2.5).
- [ ] **121.3.esp-idf** — ESP-IDF C port (separate from FreeRTOS one because IDF exposes additional ergonomic helpers and a different randomness source).
- [ ] **121.3.deprecate-rust** — Once a C port exists and parity is proven, deprecate the macro-exported path on the corresponding Rust platform crate (drop the `cffi-export` feature). The Rust trait impls themselves stay for in-process Rust callers; only the C-ABI emission goes away.

**Files (per port):**
- `packages/core/nros-platform-<rtos>-c/CMakeLists.txt`
- `packages/core/nros-platform-<rtos>-c/src/platform.c`
- `packages/core/nros-platform-<rtos>-c/include/...` if helpers are needed

**Acceptance:** per port, the C source compiles under the kernel's standard build, links into the platform-cffi consumer harness, passes the same smoke tests as the macro-exported Rust path, and a side-by-side test run shows behavioural parity with the Rust version it deprecates.

---

### 121.4 — Test infrastructure

- [x] **121.4.a** — `tests/c_stubs/platform_stubs.{c,h}` define every `nros_platform_*` symbol with a per-category counter (clock / alloc / sleep / yield / random / time / task / mutex / condvar) plus a TOTAL counter. Gated behind a new `c-stub-test` Cargo feature; `build.rs` (restored) compiles the C sources via `cc` only when the feature is on. The pre-existing `#[cfg(test)] mod test_self_export` is gated against the same feature so the two stub providers never collide.
- [x] **121.4.b** — Drift gate `scripts/check-platform-abi-mirror.sh` parses every `nros_platform_*` declaration from `<nros/platform.h>` and verifies each appears in both the `unsafe extern "C" {}` block and a macro emission. Hooked into `just check` as `check-platform-abi-mirror`. 39 symbols clean today.
- [x] **121.4.c.posix** — Two parity tests: `tests/c_stub_platform.rs` in `nros-platform-cffi` (under `c-stub-test`) dispatches every symbol through `CffiPlatform`, asserts TOTAL counter equals 39; `tests/cffi_export_parity.rs` in `nros-platform-posix` (under `cffi-export`) takes the address of every exported symbol and exercises `clock_ms` end-to-end.
- [ ] **121.4.c.remaining** — Replicate `cffi_export_parity.rs` for `nros-platform-{freertos,nuttx,threadx,zephyr}` (cross-compile only; needs cross toolchain + QEMU runner) and the five `packages/platforms/*` crates once 121.2.embedded lands.

**Files (landed):**
- `packages/core/nros-platform-cffi/tests/c_stubs/{platform_stubs.c,platform_stubs.h}`
- `packages/core/nros-platform-cffi/tests/c_stub_platform.rs`
- `packages/core/nros-platform-cffi/build.rs`
- `packages/core/nros-platform-cffi/Cargo.toml` (`c-stub-test` feature + optional `cc` build-dep)
- `packages/core/nros-platform-posix/tests/cffi_export_parity.rs`
- `scripts/check-platform-abi-mirror.sh`
- `justfile` (`check-platform-abi-mirror` recipe wired into `check`)

**Acceptance:** drift gate fails on missing symbols (verified clean — 39/39). Counter-based C-stub harness verifies every dispatch path through `CffiPlatform`. POSIX parity test exercises every macro-emitted symbol. Trait-bound check at macro-expansion site provides the compile-time impl-drift guard.

---

### 121.5 — Docs + roadmap hygiene

- [ ] **121.5.a** — Add a `docs/internals/platform-c-abi.md` page explaining the canonical ABI, the macro-export pattern, the rationale for free symbols vs vtable, and how to write a new port (both Rust-via-macro and pure C). Cross-link from `docs/design/portable-rmw-platform-interface.md`.
- [ ] **121.5.b** — Update `book/src/internals/platform-abstraction.md` (if present) to describe the new layering.
- [ ] **121.5.c** — Archive this phase doc when 121.2 + 121.3 + 121.4 close.

**Files:**
- `docs/internals/platform-c-abi.md`
- `book/src/internals/platform-abstraction.md`
- `docs/roadmap/archived/phase-121-platform-c-abi-canonical.md` (move on completion)

**Acceptance:** porter doc reads end-to-end; design doc no longer mentions cbindgen for the vtable surface.

---

## Notes

- **Migration order.** 121.2 (macro export from Rust platform crates) is the cheapest step and unblocks any C consumer that already links the existing Rust crate. 121.3 (native C ports) only matters when contributors actively want to write C — there is no behavioural gain until then. Sequence: 121.2 → 121.4 → 121.3 over time → 121.5 / archive.
- **Bare-metal.** Bare-metal stays Rust indefinitely: there is no kernel to write idiomatic C against. **Each existing bare-metal crate already ships a complete `PlatformThreading` impl** — usually as stubs (mutex/condvar return 0, `task_init` returns -1), which is the correct behaviour on single-core no-preempt hardware. The standard `nros_platform_export!` macro therefore works without modification; no per-capability split is needed. 121.3 (native C ports) does not apply — there is no host kernel to write the port against.
- **RTIC.** RTIC apps use interrupt priorities + `critical_section`-driven priority-ceiling locks, not kernel threads. A future `nros-platform-rtic` crate would impl `PlatformThreading` with the same stub shape as the bare-metal crates and join 121.2 unchanged. Tracked as 121.2.rtic; no design difference from the embedded path.
- **orin-spe delegation collision.** `nros-platform-orin-spe`'s trait impls forward to `FreeRtosPlatform`. Enabling `cffi-export` on **both** crates in the same binary would emit two `#[no_mangle]` definitions of every `nros_platform_*` symbol and fail to link. `nros-platform`'s `cffi-export` fan-out is safe because exactly one `platform-<name>` feature is active per build, but a downstream crate that directly enables `nros-platform-orin-spe/cffi-export` AND `nros-platform-freertos/cffi-export` needs to drop one.
- **Why a macro, not a proc-macro.** A `macro_rules!` declarative macro is sufficient because the expansion is data-driven (a fixed list of trait methods) with no need for token-tree inspection or attribute parsing. Avoids the proc-macro crate boundary, build-time cost, and `syn`/`quote` dependency footprint. The macro lives in `nros-platform-cffi` and is invoked from each platform crate; trait-bound checking happens at the expansion site, so a missing trait impl in the platform crate fails the compile with a clear diagnostic.
- **ABI versioning.** Free-symbol ABIs have no struct field to carry a version. Breaking changes go through symbol renames (`nros_platform_clock_ms` → `nros_platform_clock_ms_v2`) just like libc. Document this in 121.5.a.
- **Why no `abi_version` field on platform.** The RMW vtable carries `abi_version` because the runtime accepts the struct from a backend that may have been compiled against an older header; the struct can grow new tail fields. Free symbols don't grow tail fields — they grow new symbol names. Versioning is the linker's job.
- **Open question — symbol weakness.** Should the macro mark its emissions `weak` so a C port can override per-symbol when linked alongside the Rust path? Defer until a real use case appears; until then, one path or the other is linked, never both.
- **Open question — split vs unified macro.** The eight capability-specific macros (`export_clock!`, `export_alloc!`, …) plus the convenience `export_platform!` is the proposed shape. Alternative: a single `export_platform!($ty, [clock, alloc, …])` with a bracketed capability list. Decide at 121.2.a write-up time; the bracketed form is friendlier to per-capability opt-out (bare-metal) but slightly clunkier in the common case.
