# Phase 121 — Platform C ABI Canonical + Crate Migration

**Goal:** Promote the C ABI declared in `<nros/platform.h>` to the canonical platform interface. Every platform port — current Rust crates and future C-native ports — provides the same flat set of `extern "C"` symbols. The Rust `nros_platform_api` traits stay as the ergonomic Rust surface, dispatched through `CffiPlatform` for cffi consumers and re-exported from `-cffi` shim crates for Rust-native ports.

**Status:** 121.1 (header + Rust mirror) landed. Remaining work: shim crates for each Rust platform impl (121.2.*); C-native ports for RTOSes whose underlying SDKs are C (121.3.*); test stubs + CI gates (121.4); docs + bookwork (121.5).

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

- C-native ports skip the Rust shim entirely. A Zephyr engineer writes a `nros_platform_zephyr.c` that declares the symbols and links directly against `nros-platform-cffi`.
- Rust ports become `-cffi` sibling crates that re-export their `impl PlatformX for ...` as `#[unsafe(no_mangle)] extern "C"` symbols.
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
   │                              │                          │   CffiPlatform           │
   └──────────────┬───────────────┘                          └────────────┬─────────────┘
                  │                                                       │
                  │ exported by sibling -cffi shim crate                  │ unsafe extern "C"
                  │ as `#[unsafe(no_mangle)] extern "C"`                  │ {
                  │                                                       │   nros_platform_clock_ms()
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

The header is the contract. Both Rust shim crates and future C ports supply the same symbol set.

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

### 121.2 — Rust-impl-as-cffi shim crates

One sibling shim per existing Rust platform crate. Each shim contains a single source file that re-exports the wrapped crate's trait impl as `#[unsafe(no_mangle)] extern "C"` symbols named per `<nros/platform.h>`. The shim is a `cdylib` + `staticlib` (and `rlib` for cross-crate Rust linkage) so downstream binaries can pick either form.

Pattern (illustrative):

```rust
use nros_platform_posix::PosixPlatform;
use nros_platform_api::PlatformClock;

#[unsafe(no_mangle)]
pub extern "C" fn nros_platform_clock_ms() -> u64 {
    PosixPlatform::clock_ms()
}
/* ... one such function per ABI symbol ... */
```

- [ ] **121.2.posix** — `packages/core/nros-platform-posix-cffi`.
- [ ] **121.2.freertos** — `packages/core/nros-platform-freertos-cffi`.
- [ ] **121.2.nuttx** — `packages/core/nros-platform-nuttx-cffi`.
- [ ] **121.2.threadx** — `packages/core/nros-platform-threadx-cffi`.
- [ ] **121.2.zephyr** — `packages/core/nros-platform-zephyr-cffi`.
- [ ] **121.2.baremetal** — applies to the `nros-platform-baremetal` crate if present, or to bare-metal config in `nros-platform`. Defer until a bare-metal consumer needs the C ABI.

**Files (per shim):**
- `packages/core/nros-platform-<rtos>-cffi/Cargo.toml`
- `packages/core/nros-platform-<rtos>-cffi/src/lib.rs`
- Wire `nros-platform`'s `platform-cffi` feature to depend on `nros-platform-<rtos>-cffi` when the corresponding `platform-<rtos>` feature is also active.

**Acceptance:** for each shim, `cargo build -p nros-platform-<rtos>-cffi` succeeds; linking the shim against `nros-platform-cffi` and a no-op test binary resolves every symbol; smoke test verifies dispatch round-trips (e.g. `nros_platform_clock_ms()` returns monotonically increasing values).

---

### 121.3 — C-native platform ports (long arc)

Replacing each Rust shim with a hand-written C port written against the host RTOS's idiomatic API. The result is a tiny C file (or directory) that each kernel's contributor community can read at a glance.

These are independent of 121.2 — 121.2 unblocks Rust callers immediately, 121.3 lets contributors who don't write Rust ship a port.

- [ ] **121.3.posix** — POSIX C port (`platform_posix.c`). Lowest cost — `clock_gettime`, `malloc`, `pthread_*` straight through. Strongest correctness target since POSIX is the default test bed.
- [ ] **121.3.freertos** — FreeRTOS C port. `xTaskGetTickCount`, `pvPortMalloc`/`vPortFree`, `xTaskCreate`, `xSemaphoreCreateRecursiveMutex`, condvar via counting semaphores.
- [ ] **121.3.nuttx** — NuttX C port. POSIX-shaped (uses `pthread_*`, `sem_timedwait`); large parts share with 121.3.posix.
- [ ] **121.3.threadx** — ThreadX C port. `tx_thread_*`, `tx_mutex_*`, `tx_event_flags_*` for condvar.
- [ ] **121.3.zephyr** — Zephyr C port. `k_uptime_get`, `k_thread_create`, `k_mutex_*`, `k_condvar_*` (Zephyr ≥ 2.5).
- [ ] **121.3.esp-idf** — ESP-IDF C port (separate from FreeRTOS one because IDF exposes additional ergonomic helpers and a different randomness source).
- [ ] **121.3.deprecate-rust** — Once a C port exists and parity is proven, deprecate the corresponding Rust crate. Move it to `archived/` only after one release cycle.

**Files (per port):**
- `packages/core/nros-platform-<rtos>-c/CMakeLists.txt`
- `packages/core/nros-platform-<rtos>-c/src/platform.c`
- `packages/core/nros-platform-<rtos>-c/include/...` if helpers are needed

**Acceptance:** per port, the C source compiles under the kernel's standard build, links into the platform-cffi consumer harness, passes the same smoke tests as the Rust shim, and a side-by-side test run shows behavioural parity with the Rust version it deprecates.

---

### 121.4 — Test infrastructure

- [ ] **121.4.a** — Replace the in-crate `#[cfg(test)] mod test_stubs` in `nros-platform-cffi/src/lib.rs` with a proper `tests/c_stubs/` harness mirroring `nros-rmw-cffi/tests/c_stubs/`. Lets C-side stubs counters-and-all be reusable from integration tests.
- [ ] **121.4.b** — Add a CI gate that ensures every symbol declared in `<nros/platform.h>` has a matching `unsafe extern "C"` declaration in `lib.rs` (mismatch detection — drift was the original cbindgen rationale, and we still need a check).  Could be a small `grep`+diff in `just check`, or a `bindgen --check` workflow run against the header.
- [ ] **121.4.c** — Per-shim integration test verifying every symbol is supplied and behaviourally matches the wrapped Rust impl (no silent drop-throughs).

**Files:**
- `packages/core/nros-platform-cffi/tests/c_stubs/...` (new)
- `packages/core/nros-platform-cffi/tests/symbol_parity.rs` (new)
- `scripts/check-platform-abi-mirror.sh` (new) + hook into `just check`

**Acceptance:** CI fails when a header symbol is missing from the Rust mirror; CI fails when a shim crate omits a symbol; counters-based stub harness verifies every dispatch path.

---

### 121.5 — Docs + roadmap hygiene

- [ ] **121.5.a** — Add a `docs/internals/platform-c-abi.md` page explaining the canonical ABI, the shim pattern, the rationale for free symbols vs vtable, and how to write a new port. Cross-link from `docs/design/portable-rmw-platform-interface.md`.
- [ ] **121.5.b** — Update `book/src/internals/platform-abstraction.md` (if present) to describe the new layering.
- [ ] **121.5.c** — Archive this phase doc when 121.2 + 121.3 + 121.4 close.

**Files:**
- `docs/internals/platform-c-abi.md`
- `book/src/internals/platform-abstraction.md`
- `docs/roadmap/archived/phase-121-platform-c-abi-canonical.md` (move on completion)

**Acceptance:** porter doc reads end-to-end; design doc no longer mentions cbindgen for the vtable surface.

---

## Notes

- **Migration order.** 121.2 (Rust shims) is the cheapest step and unblocks any C consumer that already links the existing Rust crate. 121.3 (native C ports) only matters when contributors actively want to write C — there is no behavioural gain until then. Sequence: 121.2 → 121.4 → 121.3 over time → 121.5 / archive.
- **Bare-metal.** Bare-metal stays Rust indefinitely: there is no kernel to write idiomatic C against. Its shim is the last 121.2 item to land, and 121.3 does not apply.
- **ABI versioning.** Free-symbol ABIs have no struct field to carry a version. Breaking changes go through symbol renames (`nros_platform_clock_ms` → `nros_platform_clock_ms_v2`) just like libc. Document this in 121.5.a.
- **Why no `abi_version` field on platform.** The RMW vtable carries `abi_version` because the runtime accepts the struct from a backend that may have been compiled against an older header; the struct can grow new tail fields. Free symbols don't grow tail fields — they grow new symbol names. Versioning is the linker's job.
- **Open question — symbol weakness.** Should the Rust shim crates mark their exports `weak` so a C port can override per-symbol? Defer until a real use case appears.
