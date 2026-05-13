# Phase 121 — Platform C ABI Canonical + Crate Migration

**Goal:** Promote the C ABI declared in `<nros/platform.h>` to the canonical platform interface. Every platform port — current Rust crates and future C-native ports — provides the same flat set of `extern "C"` symbols. The Rust `nros_platform_api` traits stay as the ergonomic Rust surface, dispatched through `CffiPlatform` for cffi consumers. Rust platform crates expose the C ABI in-place via an `export_platform!` macro from `nros-platform-cffi`, gated behind each crate's own `cffi-export` feature — no sibling crates.

**Status:** Every named work item in the doc has now landed at least at the "authored against documented kernel API" level. **Runtime-verified on host** through CffiPlatform / direct extern: POSIX (14 cargo tests), NuttX (CMake build via POSIX-c reuse, 71 T symbols), FreeRTOS-Posix (`tests/freertos-c-smoke/`, `just freertos test-c-port`), ThreadX-linux (`tests/threadx-c-smoke/`, `just threadx_linux test-c-port`), Zephyr native_sim/64 (`tests/zephyr-c-smoke/`, `just zephyr test-c-port`). **Build-harness ready, runtime gated on SDK install**: ESP-IDF (`just esp_idf setup/build-c-port/test-c-port`, smoke app at `tests/esp-idf-c-smoke/`, gated on opt-in `just esp_idf setup` which clones ~5 GB IDF). 121.6.timer-macro landed (`nros_platform_export_timer!` shipped — uses `mem::transmute_copy` between caller's `#[repr(transparent)]` newtype handle and `*mut c_void`, gated by a compile-time `size_of` const assertion; TestPlatform self-test exercises it). 121.6.mcast landed across all six C ports: POSIX uses `getifaddrs` + per-family `IP_ADD_MEMBERSHIP`/`IPV6_ADD_MEMBERSHIP`; NuttX inherits via POSIX-source reuse; FreeRTOS + ESP-IDF use lwIP's POSIX-shaped `IP_ADD_MEMBERSHIP` on `INADDR_ANY` (lwIP doesn't ship `getifaddrs` — apps that need iface-pinned mcast post-set `IP_MULTICAST_IF`); Zephyr uses `zsock_setsockopt(IP_ADD_MEMBERSHIP)` (requires `CONFIG_NET_IPV4_IGMP=y`); ThreadX uses NetX Duo BSD's `IP_ADD_MEMBERSHIP` via `nx_bsd_setsockopt`. All ports drop loopback packets on read and populate the optional ZSlice sender-out parameter. 121.3.deprecate-rust per kernel stays gated on full runtime parity tests landing on its target board.

**Critical path to Rust-crate deprecation:** the platform C ABI is canonical so each kernel community can ship its platform support in the language idiomatic to that kernel. That requires the ABI to cover the **full** surface, not just the 39-symbol core — networking, timers, socket helpers. **The ABI extension (121.6) is the prerequisite for 121.3.deprecate-rust.** Until per-RTOS C net + timer impls exist, the Rust `nros-platform-<rtos>` crates remain canonical for those surfaces and can't be removed.

Remaining work: 121.6.{rust-mirror, macros, posix-c, freertos-c, threadx-c, zephyr-c, nuttx-c, esp-idf-c} (extended-surface Rust mirror + per-port implementations); then 121.3.deprecate-rust per crate; 121.2.rtic (future, on-demand); 121.3.build-verification (cross-kernel runtime tests as their SDKs land); 121.5 (porter / internals docs); per-RTOS parity tests for freertos/nuttx/threadx/zephyr/embedded (require cross toolchains, deferred).

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
- **121.2.embedded** — five embedded / bare-metal crates at `packages/platforms/`. Each already ships a complete `PlatformThreading` impl (either stubs for true bare-metal, or a delegating impl for FreeRTOS-backed targets), so the standard `nros_platform_export!` macro compiles against them as-is. No per-capability split, no stub-emission macros needed. Wired the same way as the RTOS five:
  - [x] **121.2.mps2-an385** — bare-metal Cortex-M3. `PlatformThreading` is stubbed (mutex/condvar return 0, task_init returns -1). Single-core no-preempt makes the stubs correct. Gained a `PlatformYield` impl returning `core::hint::spin_loop()`.
  - [x] **121.2.stm32f4** — bare-metal Cortex-M4F. Same stub-threading pattern. Same `PlatformYield` addition.
  - [x] **121.2.esp32** — RISC-V (ESP32-C3), single-threaded bare-metal config. Same pattern; same `PlatformYield` addition.
  - [x] **121.2.esp32-qemu** — QEMU variant of the above. Same `PlatformYield` addition.
  - [x] **121.2.orin-spe** — Cortex-R5 / FreeRTOS FSP. Trait impls delegate to `FreeRtosPlatform`. **Mutually exclusive with `nros-platform-freertos/cffi-export`** in the same binary: both would emit `#[no_mangle]` symbols for the same names. Pick one per binary; `nros-platform`'s `cffi-export` fan-out already prevents accidental double-emission because only one platform feature is active per build.
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

- [x] **121.3.posix** — POSIX C port shipped at `packages/core/nros-platform-posix-c/`. `clock_gettime`, `malloc`, `pthread_*`, `nanosleep`, `sched_yield` straight through. Builds standalone via CMake (`libnros_platform_posix.a`, 39 `T nros_platform_*` symbols) and via Cargo through `nros-platform-cffi`'s new `posix-c-port` feature (cc-rs invokes the same source file). Integration test `tests/c_port_posix.rs` runs eight host tests exercising clock monotonicity, blocking sleep, alloc/realloc/free round-trip, non-recursive + recursive mutex semantics, condvar signal/wake, and task_init/task_join round-trip.
- [x] **121.3.nuttx** — Sibling crate `packages/core/nros-platform-nuttx-c/` whose CMakeLists.txt compiles the very same `nros-platform-posix-c/src/platform.c` into `libnros_platform_nuttx.a`. NuttX's POSIX-compatibility layer (pthread, clock_gettime, nanosleep, sched_yield) gives bit-identical behaviour to the POSIX port; the C port mirrors the Rust crate's "delegate to PosixPlatform" pattern. Verified building under the POSIX simulator host build (39 `T nros_platform_*` symbols).
- [x] **121.3.freertos** — Native FreeRTOS C port at `packages/core/nros-platform-freertos-c/`. `xTaskGetTickCount` scaled by `configTICK_RATE_HZ`, `pvPortMalloc`/`vPortFree` (realloc emulated), `vTaskDelay(pdMS_TO_TICKS)`, `vTaskDelay(1)` for yield, `xTaskCreate` + self-`vTaskDelete`, `xSemaphoreCreate{Mutex,RecursiveMutex}`, condvars via mutex + counting semaphore + waiter counter (mirrors zenoh-pico's `_z_condvar_t`). Storage layouts for `task`/`mutex`/`condvar` byte-for-byte match the Rust `nros-platform-freertos::types`. CMakeLists.txt parametric on `FREERTOS_KERNEL_TARGET` + `FREERTOS_CONFIG_TARGET`; build requires the parent build to declare those imported targets. Integration test deferred (needs a target board build).
- [x] **121.3.threadx** — Native Azure RTOS ThreadX C port at `packages/core/nros-platform-threadx-c/`. `tx_time_get` scaled by `TX_TIMER_TICKS_PER_SECOND`, `tx_byte_allocate`/`tx_byte_release` against a caller-set byte pool (`nros_platform_threadx_set_byte_pool`), `tx_thread_sleep`, `tx_thread_relinquish`, `tx_thread_create` + `tx_thread_terminate` + `tx_thread_delete`, `tx_mutex_create(TX_INHERIT)` (recursive by design — `mutex_*` and `mutex_rec_*` share the primitive), condvars on `tx_semaphore` with the caller's mutex released around the wait (matches the Rust impl). CMakeLists.txt parametric on `THREADX_KERNEL_TARGET`; build requires the parent build to declare that imported target. Integration test deferred.
- [x] **121.3.zephyr** — Native Zephyr RTOS C port at `packages/core/nros-platform-zephyr-c/`. `k_uptime_get` (ms) + `k_cycle_get_64`→`k_cyc_to_us_floor64` (us), `k_malloc`/`k_free` (realloc emulated), `k_msleep`/`k_usleep`/`k_sleep`, `k_yield`, `sys_rand32_get`/`sys_rand_get`, `k_thread_create`+`k_thread_join`+`k_thread_abort`, `k_mutex_*` (recursive by design), `k_condvar_*` (requires Zephyr ≥ 2.5). Ships as a Zephyr module — the parent application registers it via `west.yml` and the `zephyr` interface library auto-supplies kernel headers. Integration test deferred.
- [x] **121.3.esp-idf** — Native Espressif ESP-IDF C port at `packages/core/nros-platform-esp-idf-c/`. FreeRTOS underneath (so task/mutex/condvar reuse the FreeRTOS-C pattern with storage layouts matching `nros-platform-freertos::types`), but ESP-IDF-specific overrides: `esp_timer_get_time()` for microsecond-resolution monotonic clock, `esp_random()` + `esp_fill_random()` for entropy, `time(NULL)` for the wall clock (SNTP / RTC drives the value), `esp_rom_delay_us` for sub-tick busy-waits, libc `malloc`/`realloc`/`free` (ESP-IDF redirects these to `heap_caps_malloc(MALLOC_CAP_DEFAULT)`). Built as an ESP-IDF component via `idf_component_register`. Integration test deferred.
- **121.3.build-verification** — header-level syntax checks (`gcc -c -fsyntax-only`) against the in-tree kernel headers prove the C source compiles cleanly against each kernel's API surface:
  - [x] **POSIX** — Cargo integration test (`cargo test -p nros-platform-cffi --features posix-c-port`) builds + runs eight semantic tests through `CffiPlatform`. Strongest evidence of correctness.
  - [x] **NuttX** — Host CMake build using the in-tree POSIX source; produces `libnros_platform_nuttx.a` with 39 `T nros_platform_*` symbols.
  - [x] **FreeRTOS** — Syntax check against `third-party/freertos/kernel/include` + the in-tree Posix port headers + the `examples/template_configuration/FreeRTOSConfig.h` template (with `INCLUDE_eTaskGetState=1` flipped on for `task_join`'s state-polling). Clean — no warnings, no errors.
  - [x] **ThreadX** — Syntax check against `third-party/threadx/kernel/common/inc` + `ports/linux/gnu/inc`. Clean.
  - [ ] **Zephyr** — Deferred. Verification requires a configured Zephyr build (autoconf.h, devicetree.h, …); the standalone include path is not enough. Run from within a Zephyr application that pulls the module.
  - [ ] **ESP-IDF** — Deferred. Verification requires an ESP-IDF project tree; `idf_component_register` doesn't function in a standalone build.
- [ ] **121.3.deprecate-rust** — Once a C port exists and parity is proven, deprecate the macro-exported path on the corresponding Rust platform crate (drop the `cffi-export` feature). The Rust trait impls themselves stay for in-process Rust callers; only the C-ABI emission goes away. Not started for POSIX or any other port — the Rust path is still useful as a side-by-side parity oracle.

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

### 121.6 — Extend the canonical C ABI to the full platform surface

**Goal.** Make the platform C ABI cover **every** capability the platform traits expose, not just the 39-symbol core in `<nros/platform.h>`. The point of a canonical C ABI is to let each kernel ship its platform support in the language idiomatic to that kernel (C on Zephyr / FreeRTOS / NuttX / ThreadX / ESP-IDF; Rust on bare-metal; future C++ / Zig / Ada as appropriate). Today the C ports cover only clock + alloc + sleep + yield + random + time + tasks + mutexes + condvars. Networking, timers, socket helpers, and platform-network-poll still live in Rust-only `net.rs` / timer wrappers inside each `nros-platform-<rtos>` crate, which blocks 121.3.deprecate-rust because removing the Rust crate orphans those surfaces.

Sequence rule: **extend the C ABI first, implement per port, then deprecate the corresponding Rust crate.** Skipping the extension step would leave each kernel community without a complete native-language path.

- [x] **121.6.headers** — Author two new canonical headers in `packages/core/nros-platform-cffi/include/nros/`:
  - **`platform_timer.h`** — `nros_platform_timer_create_{periodic,oneshot}`, `nros_platform_timer_destroy`, `nros_platform_timer_cancel`. Opaque `void *` handle (NULL on error) matches the Rust `PlatformTimer::TimerHandle` opacity. Callback context + threading rules documented inline.
  - **`platform_net.h`** — `nros_platform_tcp_*`, `nros_platform_udp_*`, `nros_platform_udp_mcast_*`, `nros_platform_socket_*`, `nros_platform_network_poll`. Mirrors `PlatformTcp` / `PlatformUdp` / `PlatformUdpMulticast` / `PlatformSocketHelpers` / `PlatformNetworkPoll` byte-for-byte. `(size_t) -1` sentinel for `read` / `send` errors matches the existing Rust trait convention.
- [x] **121.6.rust-mirror** — `nros-platform-cffi/src/lib.rs` now carries `unsafe extern "C" { … }` blocks for every symbol in `platform_timer.h` (4) + `platform_net.h` (28) = 32 additional declarations. Decls are unconditional (no feature gate) — extern declarations cost nothing at link time unless code references them; the per-symbol link cost only materialises if a future `impl PlatformTcp for CffiPlatform { … }` lands. Drift gate `scripts/check-platform-abi-mirror.sh` rewritten to walk multiple headers per run with a configurable `HEADERS_REQUIRE_MACRO` / `HEADERS_EXTERN_ONLY` split. Today: 71 symbols across 3 headers, gate clean.
- [x] **121.6.macros** — `nros_platform_export_net!($ty)` shipped — 28 emissions covering TCP / UDP / UDP-multicast / socket helpers / network-poll, 1:1 with the trait surface. Caller must implement `PlatformTcp`, `PlatformUdp`, `PlatformUdpMulticast`, `PlatformSocketHelpers`, and `PlatformNetworkPoll`. `nros-platform-posix` invokes the macro under its existing `cffi-export` feature (gained a no-op `PlatformNetworkPoll` impl because POSIX socket I/O is kernel-driven). `tests/cffi_export_parity.rs` in nros-platform-posix extended to pin all 59 symbols (31 core + 28 net) — 1/1 pass. `nros_platform_export_timer!` deferred — `PlatformTimer::TimerHandle` is an associated type, so the macro needs a per-platform handle-to-`*mut c_void` adapter; the design lands when the first kernel actually wires PlatformTimer through `CffiPlatform`.
- [x] **121.6.posix-c** — POSIX implementations landed at `packages/core/nros-platform-posix-c/src/{net.c,timer.c}`:
  - `net.c` mirrors `nros-platform-posix::net.rs` for full TCP + UDP unicast + socket helpers + `network_poll` (a kernel-driven no-op). Endpoint = `{ struct addrinfo *iptcp; }` and socket = `{ int fd; }` match zenoh-pico's `_z_sys_net_endpoint_t` / `_z_sys_net_socket_t` byte-for-byte. UDP multicast is stubbed (returns `-1` / `(size_t) -1`) — full `getifaddrs` + `IP_ADD_MEMBERSHIP` plumbing deferred; consumers needing multicast keep the Rust path.
  - `timer.c` uses `timer_create(CLOCK_MONOTONIC, SIGEV_THREAD)` with a heap-owned record carrying the kernel `timer_t` + caller's callback + `user_data`. `cancel` distinguishes prevent-fire vs already-fired via an atomic flag set by the trampoline.
  - `CMakeLists.txt` links all three sources into `libnros_platform_posix.a` (71 `T` symbols) + propagates `Threads::Threads` and `rt`.
  - `nros-platform-cffi`'s `posix-c-port` build.rs cc-compiles all three .c files; emits `cargo:rustc-link-lib=pthread` + `rt`.
  - New integration tests: `tests/c_port_posix_net.rs` (3 cases: TCP loopback round-trip with `socket_accept`, UDP loopback round-trip, `network_poll` no-op) + `tests/c_port_posix_timer.rs` (3 cases: periodic fires repeatedly, oneshot fires exactly once, cancel prevents fire) — 6/6 pass.
- [x] **121.6.freertos-c** — `packages/core/nros-platform-freertos-c/src/net.c` against lwIP BSD socket API (`lwip_socket`, `lwip_recv`, etc.) + `src/timer.c` against `xTimerCreate` / `xTimerStart` / `xTimerDelete`. CMakeLists.txt grew `FREERTOS_LWIP_TARGET` parameter for the parent build's lwIP target. Multicast stubbed. Both syntax-verified against in-tree FreeRTOS-Kernel + lwIP-contrib headers; runtime test requires a target board harness.
- [x] **121.6.threadx-c** — `src/net.c` against NetX Duo BSD socket layer (`nx_bsd_*` with `INT`-typed sockfd + `nx_bsd_timeval *`-typed `SO_RCVTIMEO`) + `src/timer.c` against `tx_timer_create` / `tx_timer_deactivate` / `tx_timer_delete`. Timer pool registered via new `nros_platform_threadx_set_timer_pool(void *)`. CMakeLists.txt grew `NETXDUO_TARGET` parameter. Timer syntax-verified against in-tree ThreadX headers; net needs NetX Duo build context.
- [x] **121.6.zephyr-c** — `src/net.c` against `zsock_*` (Zephyr BSD socket layer) + `src/timer.c` against `k_timer_init` / `k_timer_start` / `k_timer_stop` with `atomic_int fired/cancelled` flags. Ships as a Zephyr module — the parent application's `zephyr` interface target provides headers.
- [x] **121.6.nuttx-c** — Net + timer reuse the POSIX-c sources verbatim via the existing CMakeLists pattern. `NROS_PLATFORM_POSIX_C_SOURCE` cache var generalised to `NROS_PLATFORM_POSIX_C_SRC_DIR`; library compiles `{platform,net,timer}.c` from the sibling crate. Host-verified — `libnros_platform_nuttx.a` exports 71 `T nros_platform_*` symbols against the POSIX simulator port.
- [x] **121.6.esp-idf-c** — `src/net.c` against ESP-IDF's lwIP BSD socket layer (shares the lwIP wire shape with FreeRTOS-c so binaries can swap) + `src/timer.c` against `esp_timer_create` / `esp_timer_start_periodic` / `esp_timer_start_once` with `ESP_TIMER_TASK` dispatch. `idf_component_register` updated to list all three sources + add `lwip` to `REQUIRES`. Build harness: `just esp_idf setup` (opt-in — installs IDF at `esp-idf-workspace/esp-idf/`, default ref `v5.3`, target chips configurable) + `just esp_idf {build,test}-c-port` (drives `idf.py build` + `qemu-system-riscv32 -machine esp32c3` boot of the `tests/esp-idf-c-smoke/` project, which pulls the component via `EXTRA_COMPONENT_DIRS` and exercises clock + alloc + sleep + yield + random + periodic timer over 150 ms). Distinct from `just esp32 setup` which only installs Espressif's QEMU fork for the default esp-hal bare-metal path.

Per-port acceptance: matches the Rust crate's net+timer surface byte-for-byte (same return values, same sentinels, same blocking semantics). The Rust `nros-platform-<rtos>` crate becomes deprecate-able only when **all of**: (a) core 39 symbols, (b) net surface, (c) timer surface, (d) per-port runtime tests pass. That is the gate for 121.3.deprecate-rust per crate.

**Why this is the prerequisite for Rust-crate removal.** A platform package in its preferred language (Zephyr → C, FreeRTOS → C, NuttX → C, ThreadX → C, ESP-IDF → C, bare-metal / RTIC → Rust) needs the canonical ABI to cover every API the runtime calls. Today the runtime calls 39 core symbols + ~30 net + ~4 timer = ~73 symbols. Until the ABI covers the latter ~34, dropping the Rust crate breaks every binary that uses networking or timers.

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

- **Migration order.** Critical path is now: 121.1 (canonical header) → 121.2 (Rust-side macro export of the 39 core symbols — done) → 121.3 (native C ports of the 39 core symbols — done) → 121.4 (drift gate + tests — done) → **121.6 (extend the canonical ABI to cover net + timer; per-RTOS C impls)** → 121.3.deprecate-rust per kernel → 121.5 / archive. 121.6 is the gating step: every platform's preferred language can host a complete port only once the ABI covers every surface the runtime calls.
- **Bare-metal.** Bare-metal stays Rust indefinitely: there is no kernel to write idiomatic C against. **Each existing bare-metal crate already ships a complete `PlatformThreading` impl** — usually as stubs (mutex/condvar return 0, `task_init` returns -1), which is the correct behaviour on single-core no-preempt hardware. The standard `nros_platform_export!` macro therefore works without modification; no per-capability split is needed. 121.3 (native C ports) does not apply — there is no host kernel to write the port against.
- **RTIC.** RTIC apps use interrupt priorities + `critical_section`-driven priority-ceiling locks, not kernel threads. A future `nros-platform-rtic` crate would impl `PlatformThreading` with the same stub shape as the bare-metal crates and join 121.2 unchanged. Tracked as 121.2.rtic; no design difference from the embedded path.
- **orin-spe delegation collision.** `nros-platform-orin-spe`'s trait impls forward to `FreeRtosPlatform`. Enabling `cffi-export` on **both** crates in the same binary would emit two `#[no_mangle]` definitions of every `nros_platform_*` symbol and fail to link. `nros-platform`'s `cffi-export` fan-out is safe because exactly one `platform-<name>` feature is active per build, but a downstream crate that directly enables `nros-platform-orin-spe/cffi-export` AND `nros-platform-freertos/cffi-export` needs to drop one.
- **Why a macro, not a proc-macro.** A `macro_rules!` declarative macro is sufficient because the expansion is data-driven (a fixed list of trait methods) with no need for token-tree inspection or attribute parsing. Avoids the proc-macro crate boundary, build-time cost, and `syn`/`quote` dependency footprint. The macro lives in `nros-platform-cffi` and is invoked from each platform crate; trait-bound checking happens at the expansion site, so a missing trait impl in the platform crate fails the compile with a clear diagnostic.
- **ABI versioning.** Free-symbol ABIs have no struct field to carry a version. Breaking changes go through symbol renames (`nros_platform_clock_ms` → `nros_platform_clock_ms_v2`) just like libc. Document this in 121.5.a.
- **Why no `abi_version` field on platform.** The RMW vtable carries `abi_version` because the runtime accepts the struct from a backend that may have been compiled against an older header; the struct can grow new tail fields. Free symbols don't grow tail fields — they grow new symbol names. Versioning is the linker's job.
- **Open question — symbol weakness.** Should the macro mark its emissions `weak` so a C port can override per-symbol when linked alongside the Rust path? Defer until a real use case appears; until then, one path or the other is linked, never both.
- **Open question — split vs unified macro.** The eight capability-specific macros (`export_clock!`, `export_alloc!`, …) plus the convenience `export_platform!` is the proposed shape. Alternative: a single `export_platform!($ty, [clock, alloc, …])` with a bracketed capability list. Decide at 121.2.a write-up time; the bracketed form is friendlier to per-capability opt-out (bare-metal) but slightly clunkier in the common case.
