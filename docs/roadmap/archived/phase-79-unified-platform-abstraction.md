# Phase 79: Unified Platform Abstraction Layer

**Goal**: Define a single platform interface (`nros-platform`) that all RMW backends consume, eliminating per-RMW platform crates and making the RMW layer fully platform-agnostic.

**Status**: Complete (79.1–79.16 done, 79.12 abandoned)
**Priority**: Medium
**Depends on**: None (can proceed independently)

## Overview

### Problem

Today, each RMW backend has its own platform crate per board:

```
zpico-platform-mps2-an385   →  55 zenoh-pico symbols (z_malloc, z_clock_now, ...)
xrce-platform-mps2-an385    →  3 XRCE symbols (uxr_millis, uxr_nanos, ...)
```

This causes:
- **Duplicated platform logic** — the same hardware clock, allocator, and threading code is reimplemented for each RMW backend
- **N×M scaling** — adding a new platform requires one crate per RMW backend; adding a new RMW requires one platform crate per board
- **RMW backends are platform-aware** — zenoh-pico and XRCE shims contain board-specific code instead of being pure middleware adapters

### Solution

Introduce a unified platform abstraction following the same dual C/Rust trait pattern used by the RMW layer (`nros-rmw` traits + `nros-rmw-cffi` vtable adapter):

```
Application
  └── nros-node
       ├── nros-rmw  (RMW traits — already platform-agnostic)
       │    ├── nros-rmw-zenoh → zpico-platform-shim (z_* → nros-platform traits)
       │    └── nros-rmw-xrce  → xrce-platform-shim  (uxr_* → nros-platform traits)
       └── nros-platform (unified platform traits + ConcretePlatform alias)
            ├── nros-platform-posix        (POSIX: clock_gettime, malloc, pthreads)
            ├── nros-platform-zephyr       (Zephyr: k_uptime_get, k_mutex, ...)
            ├── nros-platform-freertos     (FreeRTOS: via cffi vtable)
            ├── nros-platform-mps2-an385   (bare-metal: CMSDK Timer0, zpico-alloc heap)
            ├── nros-platform-stm32f4      (bare-metal: DWT cycle counter)
            └── nros-platform-cffi         (C vtable adapter)
```

After this change:
- Platform crates are written **once per RTOS**, not once per RTOS per RMW
- RMW shim crates are thin forwarders, **identical across all platforms**
- Adding a new platform requires **one** crate, not N crates
- Adding a new RMW backend requires **one** shim crate, not M platform crates

## Architecture

### Capability sub-traits

The platform interface is split into independent capability traits. Each RMW
backend declares which capabilities it requires via trait bounds.

```rust
// nros-platform/src/traits.rs

/// Monotonic clock — the most critical primitive.
pub trait PlatformClock {
    /// Returns monotonic time in milliseconds.
    fn clock_ms() -> u64;

    /// Returns monotonic time in microseconds.
    fn clock_us() -> u64;
}

/// Heap allocation.
pub trait PlatformAlloc {
    fn alloc(size: usize) -> *mut u8;
    fn realloc(ptr: *mut u8, size: usize) -> *mut u8;
    fn dealloc(ptr: *mut u8);
}

/// Sleep / delay.
pub trait PlatformSleep {
    fn sleep_us(us: u64);
    fn sleep_ms(ms: u64);
    fn sleep_s(s: u64);
}

/// Pseudo-random number generation.
pub trait PlatformRandom {
    fn random_u8() -> u8;
    fn random_u16() -> u16;
    fn random_u32() -> u32;
    fn random_u64() -> u64;
    fn random_fill(buf: &mut [u8]);
}

/// Wall-clock time (for logging, not timing-critical).
pub trait PlatformTime {
    fn time_since_epoch_secs() -> u32;
    fn time_since_epoch_nanos() -> u32;
}

/// Threading primitives (tasks, mutexes, condvars).
pub trait PlatformThreading {
    type Task;
    type Mutex;
    type RecursiveMutex;
    type Condvar;

    fn task_spawn(entry: extern "C" fn(*mut c_void), arg: *mut c_void, ...) -> Self::Task;
    fn task_join(task: &mut Self::Task);

    fn mutex_create() -> Self::Mutex;
    fn mutex_lock(m: &Self::Mutex);
    fn mutex_try_lock(m: &Self::Mutex) -> bool;
    fn mutex_unlock(m: &Self::Mutex);
    fn mutex_destroy(m: &mut Self::Mutex);

    // Recursive mutex, condvar — similar pattern
    // ...
}

/// Smoltcp network poll (bare-metal only).
pub trait PlatformNetworkPoll {
    fn network_poll();
    fn smoltcp_clock_now_ms() -> u64;
}
```

### Compile-time resolution

Same pattern as `ConcreteSession` in `session.rs`:

```rust
// nros-platform/src/resolve.rs

#[cfg(feature = "platform-posix")]
pub type ConcretePlatform = nros_platform_posix::PosixPlatform;

#[cfg(feature = "platform-zephyr")]
pub type ConcretePlatform = nros_platform_zephyr::ZephyrPlatform;

#[cfg(feature = "platform-freertos")]
pub type ConcretePlatform = nros_platform_freertos::FreeRtosPlatform;

#[cfg(feature = "platform-bare-metal")]
pub type ConcretePlatform = nros_platform_bare_metal::BareMetalPlatform;

// ...
```

### RMW shim crates

Thin forwarders from RMW-specific C symbols to `ConcretePlatform` trait methods.
These are **platform-independent** — the same code works for all platforms.

```rust
// zpico-platform-shim/src/clock.rs
use nros_platform::{ConcretePlatform, PlatformClock};

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_now() -> usize {
    ConcretePlatform::clock_ms() as usize
}

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_elapsed_ms(time: *mut usize) -> core::ffi::c_ulong {
    let prev = unsafe { *time } as u64;
    let now = ConcretePlatform::clock_ms();
    now.wrapping_sub(prev) as core::ffi::c_ulong
}
```

```rust
// xrce-platform-shim/src/lib.rs
use nros_platform::{ConcretePlatform, PlatformClock};

#[unsafe(no_mangle)]
pub extern "C" fn uxr_millis() -> i64 {
    ConcretePlatform::clock_ms() as i64
}

#[unsafe(no_mangle)]
pub extern "C" fn uxr_nanos() -> i64 {
    ConcretePlatform::clock_us() as i64 * 1000
}
```

### C vtable adapter (nros-platform-cffi)

For platforms where the SDK is most naturally called from C (FreeRTOS,
ThreadX), a C vtable adapter allows the platform crate to be written in C:

```c
// nros_platform_vtable.h
typedef struct {
    uint64_t (*clock_ms)(void);
    uint64_t (*clock_us)(void);
    void *(*alloc)(size_t size);
    void *(*realloc)(void *ptr, size_t size);
    void (*dealloc)(void *ptr);
    void (*sleep_ms)(uint64_t ms);
    uint8_t (*random_u8)(void);
    // ...
} nros_platform_vtable_t;

void nros_platform_cffi_register(const nros_platform_vtable_t *vtable);
```

```rust
// nros-platform-cffi/src/lib.rs
pub struct CffiPlatform;

impl PlatformClock for CffiPlatform {
    fn clock_ms() -> u64 {
        unsafe { (get_vtable().clock_ms)() }
    }
}
```

## Work Items

- [x] 79.1 — Create `nros-platform` trait crate with capability sub-traits
- [x] 79.2 — Create `nros-platform-cffi` with C vtable registration
- [x] 79.3 — Create `nros-platform-posix` (first native implementation)
- [x] 79.4 — Create `zpico-platform-shim` (zenoh-pico forwarder)
- [x] 79.5 — Create `xrce-platform-shim` (XRCE-DDS forwarder)
- [x] 79.6 — Create `nros-platform-mps2-an385` (bare-metal platform crate)
- [x] 79.7 — Verify `xrce-platform-shim` + `nros-platform-mps2-an385` symbol equivalence
- [x] 79.8 — Create remaining bare-metal platform crates (STM32F4, ESP32, ESP32-QEMU)
- [x] 79.9 — Migrate board crates and remove old per-RMW platform crates
- [x] 79.10 — Update book documentation for unified platform architecture
  - [x] 79.10.1 — `concepts/architecture.md` — Mermaid diagrams updated with nros-platform layer
  - [x] 79.10.2 — `concepts/platform-model.md` — feature propagation + ConcretePlatform description
  - [x] 79.10.3 — `concepts/rmw-backends.md` — added shim annotations
  - [x] 79.10.4 — `guides/porting-platform/README.md` — rewritten with unified architecture diagram
  - [x] 79.10.5 — `guides/porting-platform/zenoh-pico.md` — reframed as symbol reference
  - [x] 79.10.6 — `guides/porting-platform/xrce-dds.md` — updated paths + shim reference
  - [x] 79.10.7 — Created `guides/porting-platform/implementing-a-platform.md` (main porting guide)
  - [x] 79.10.8 — `guides/board-crate.md` — updated to nros-platform-* references
  - [x] 79.10.9 — `guides/esp32.md` — crate tree updated to nros-platform-esp32
  - [x] 79.10.10 — `platforms/README.md` — updated to three-crate pattern
- [x] 79.11 — Add `platform-cffi` feature to `nros` facade crate
- [-] 79.12 — ~~Make RMW crates fully platform-agnostic~~ (abandoned — see design notes)
- [x] 79.13 — Move shim crates into -sys crates (board crates become RMW-agnostic)
  - [x] 79.13.1 — `zpico-sys/bare-metal` activates `zpico-platform-shim/active`; `link-tcp` activates `smoltcp`
  - [x] 79.13.2 — `xrce-sys/bare-metal,freertos,threadx` activate `xrce-platform-shim/active`
  - [x] 79.13.3 — Removed `zpico-platform-shim` dependency from all 4 board crates
  - [x] 79.13.4 — Board crates only depend on `nros-platform-<board>` (RMW-agnostic)
- [x] 79.14 — Use shim for ALL platforms (not just bare-metal)
  - [x] 79.14.1 — POSIX/NuttX/bare-metal use shim; FreeRTOS/ThreadX keep C system.c
  - [x] 79.14.2 — `extern crate` in -sys crates forces linker to include shim symbols
  - [x] 79.14.3 — `nros` facade activates `nros-platform/platform-posix` for POSIX builds
  - [x] 79.14.4 — Example builds + 337 unit tests pass
  - [x] 79.14.5 — Create `nros-platform-freertos` crate (FreeRTOS task/mutex/alloc via extern "C" FFI)
  - [x] 79.14.6 — Create `nros-platform-nuttx` crate (type alias to PosixPlatform)
  - [x] 79.14.7 — Create `nros-platform-threadx` crate (ThreadX thread/mutex/byte_pool via extern "C" FFI)
- [x] 79.16 — Fix nros-platform-freertos struct layout + activate shim (29/29 tests pass)
  - [x] 79.16.1 — Add `#[repr(C)]` types matching FreeRTOS `_z_task_t`, `_z_condvar_t` layouts
  - [x] 79.16.2 — Add event group FFI (`xEventGroupCreate`, `xEventGroupSetBits`, `xEventGroupWaitBits`)
  - [x] 79.16.3 — Implement task wrapper (store fun/arg in struct, signal join_event, self-suspend)
  - [x] 79.16.4 — Implement condvar with waiter counting (mutex-protected, matching C semantics)
  - [x] 79.16.5 — Activate shim for FreeRTOS in zpico-sys + skip system.c
  - [x] 79.16.6 — Verify FreeRTOS E2E tests pass (29/29 — Rust, C, C++)
  - [x] 79.16.7 — Seed platform RNG with IP+MAC hash (fix duplicate zenoh session IDs)
- [x] 79.15 — Migrate ThreadX zenoh-pico from C system.c to Rust shim
  - [x] 79.15.1 — Wire `zpico-platform-shim` for ThreadX (activate shim in zpico-sys when `threadx` feature enabled)
  - [x] 79.15.2 — Delete `c/platform/threadx/system.c`; task creation kept in C `task.c` (struct layout dependency)
  - [x] 79.15.3 — Verify ThreadX Linux talker/listener E2E (10/10 messages)
  - [x] 79.15.4 — Verify ThreadX RISC-V QEMU E2E (23/23 tests pass)

### 79.1 — Create `nros-platform` trait crate

Define the capability sub-traits (`PlatformClock`, `PlatformAlloc`,
`PlatformSleep`, `PlatformRandom`, `PlatformTime`, `PlatformThreading`,
`PlatformNetworkPoll`) and the `ConcretePlatform` type alias resolution.

**Files:**
- `packages/core/nros-platform/Cargo.toml`
- `packages/core/nros-platform/src/lib.rs`
- `packages/core/nros-platform/src/traits.rs`
- `packages/core/nros-platform/src/resolve.rs`

### 79.2 — Create `nros-platform-cffi` with C vtable

C vtable struct mirroring all trait methods. `nros_platform_cffi_register()`
stores the vtable in a static atomic pointer. `CffiPlatform` implements all
traits by dispatching through the vtable.

**Files:**
- `packages/core/nros-platform-cffi/Cargo.toml`
- `packages/core/nros-platform-cffi/src/lib.rs`
- `packages/core/nros-platform-cffi/include/nros/platform_vtable.h`

### 79.3 — Create `nros-platform-posix`

Direct implementation using POSIX APIs: `clock_gettime`, `malloc`,
`nanosleep`, `pthread_*`, `/dev/urandom`.

**Files:**
- `packages/core/nros-platform-posix/Cargo.toml`
- `packages/core/nros-platform-posix/src/lib.rs`

### 79.4 — Create `zpico-platform-shim`

Thin `extern "C"` forwarders: `z_clock_now` → `ConcretePlatform::clock_ms()`,
`z_malloc` → `ConcretePlatform::alloc()`, etc. All ~55 symbols, no
platform-specific code.

**Files:**
- `packages/zpico/zpico-platform-shim/Cargo.toml`
- `packages/zpico/zpico-platform-shim/src/lib.rs` (clock, memory, sleep, random, time)
- `packages/zpico/zpico-platform-shim/src/threading.rs`
- `packages/zpico/zpico-platform-shim/src/socket_stubs.rs`

### 79.5 — Create `xrce-platform-shim`

Thin `extern "C"` forwarders: `uxr_millis` → `ConcretePlatform::clock_ms()`,
`uxr_nanos` → `ConcretePlatform::clock_us()`. Only 2-3 symbols.

**Files:**
- `packages/xrce/xrce-platform-shim/Cargo.toml`
- `packages/xrce/xrce-platform-shim/src/lib.rs`

### 79.6 — Create `nros-platform-mps2-an385`

Created a unified platform crate at `packages/boards/nros-platform-mps2-an385/`
that provides all platform primitives for the MPS2-AN385 bare-metal board.
Both `zpico-platform-shim` and `xrce-platform-shim` can use it via the
`platform-mps2-an385` feature on `nros-platform`.

Key design: the board crate (`nros-mps2-an385`) owns lifecycle (init, run,
device management) and delegates to the platform crate for system primitives.
Sleep uses a registerable poll callback (`set_poll_callback`) so the board
crate can wire in smoltcp polling without coupling the platform crate to
any specific transport.

**Files:**
- `packages/boards/nros-platform-mps2-an385/Cargo.toml`
- `packages/boards/nros-platform-mps2-an385/src/lib.rs` (Mps2An385Platform)
- `packages/boards/nros-platform-mps2-an385/src/clock.rs` (CMSDK Timer0)
- `packages/boards/nros-platform-mps2-an385/src/memory.rs` (zpico-alloc heap)
- `packages/boards/nros-platform-mps2-an385/src/random.rs` (xorshift PRNG)
- `packages/boards/nros-platform-mps2-an385/src/sleep.rs` (busy-wait + poll callback)
- `packages/boards/nros-platform-mps2-an385/src/libc_stubs.rs` (strlen, memcpy, etc.)
- `packages/boards/nros-platform-mps2-an385/src/timing.rs` (DWT cycle counter)

### 79.7 — Verify XRCE symbol equivalence

Verified that `xrce-platform-shim` + `nros-platform-mps2-an385` produces
identical FFI symbols (`uxr_millis`, `uxr_nanos`, `smoltcp_clock_now_ms`)
with identical logic to the old `xrce-platform-mps2-an385`. No XRCE
bare-metal QEMU tests exist yet (XRCE examples are native POSIX and Zephyr
only), so runtime verification is deferred to when XRCE bare-metal examples
are created.

### 79.8 — Create remaining bare-metal platform crates

Created unified platform crates for all bare-metal boards:

- `nros-platform-stm32f4` — DWT-based clock, 64 KB heap, PHY detection, pin configs
- `nros-platform-esp32` — `esp_hal::time::Instant` clock, 32 KB heap
- `nros-platform-esp32-qemu` — same clock as ESP32, includes libc sprintf/snprintf stubs

RTOS platforms (FreeRTOS, NuttX, ThreadX, Zephyr) do not have dedicated Rust
platform crates — they use zenoh-pico's built-in C platform layers or POSIX.
Creating unified RTOS platform crates is deferred to when those backends are
refactored to use the nros-platform interface.

**Files:**
- `packages/boards/nros-platform-stm32f4/` (clock, memory, random, sleep, phy, pins, timing, libc_stubs)
- `packages/boards/nros-platform-esp32/` (clock, memory, random, sleep, timing)
- `packages/boards/nros-platform-esp32-qemu/` (clock, memory, random, sleep, timing, libc_stubs)

### 79.9 — Migrate board crates and remove old per-RMW platform crates

Migrated all board crates to use the unified platform layer:

1. **Updated board crate deps**: replaced `zpico-platform-<name>` with
   `nros-platform-<name>` + `zpico-platform-shim`
2. **Moved network modules**: `network.rs` (transport-specific smoltcp poll
   callback + global state) moved from old platform crates into each board
   crate, since it depends on board-specific device types
3. **Fixed zpico-platform-shim socket stubs**: `_z_socket_close` now forwards
   to `_z_close_tcp` and `_z_socket_wait_event` calls `smoltcp_poll()` via
   link-time resolved extern imports (no Cargo dependency on zpico-smoltcp)
4. **Updated all import paths**: `zpico_platform_<name>` → `nros_platform_<name>`
5. **Registered sleep poll callback**: board crates call
   `nros_platform_<name>::sleep::set_poll_callback(smoltcp_network_poll)` so
   busy-wait sleep polls the network stack
6. **Deleted old crates**: `zpico-platform-mps2-an385`, `zpico-platform-stm32f4`,
   `zpico-platform-esp32`, `zpico-platform-esp32-qemu`, `xrce-platform-mps2-an385`
7. **Removed from workspace**: cleaned up `Cargo.toml` exclude list

### 79.10 — Update book documentation

10 files in `book/src/` reference the old per-RMW platform architecture
(`zpico-platform-*`, `xrce-platform-*`). These need updating to describe the
unified `nros-platform` layer.

**Concept pages (architecture/model):**
- **79.10.1** `concepts/architecture.md` — Mermaid diagrams at lines 89,
  134-143, 399 show old `zpico-platform-*` directly. Add nros-platform layer
  between board crates and RMW shims. Show the two-layer platform awareness.
- **79.10.2** `concepts/platform-model.md` — Feature propagation description
  at line 145 says features activate `zpico-platform-*` crates. Update to
  describe the unified nros-platform trait interface and the two-layer design
  (C compilation in -sys via RMW features, Rust symbols via nros-platform
  traits + shim crates).
- **79.10.3** `concepts/rmw-backends.md` — Note that the ~55 zenoh-pico
  symbols are now provided by zpico-platform-shim via the nros-platform
  interface, not per-board platform crates.

**Porting guides:**
- **79.10.4** `guides/porting-platform/README.md` — Rewrite overview diagram
  to show: `nros-platform-<board>` → `zpico-platform-shim`/`xrce-platform-shim`
  → RMW. Explain that porting now means creating one `nros-platform-<name>`
  crate that works for all RMW backends.
- **79.10.5** `guides/porting-platform/zenoh-pico.md` — Reframe as RMW shim
  reference. Update file paths from `zpico-platform-<name>` to
  `nros-platform-<name>`. Note symbols are now in zpico-platform-shim.
- **79.10.6** `guides/porting-platform/xrce-dds.md` — Same: update paths,
  note xrce-platform-shim, update example from `xrce-platform-mps2-an385`.
- **79.10.7** New `guides/porting-platform/implementing-a-platform.md` — The
  main porting guide. How to create `nros-platform-<name>`, implement methods,
  wire features, register sleep poll callback, handle libc stubs.

**Board/platform-specific pages:**
- **79.10.8** `guides/board-crate.md` — Update dependency pattern (line 4:
  wraps `zpico-platform-*` → wraps `nros-platform-*` + shim). Describe the
  new init flow with sleep poll callback registration.
- **79.10.9** `guides/esp32.md` — Update crate tree at lines 110-111 from
  `zpico-platform-esp32` to `nros-platform-esp32`.
- **79.10.10** `platforms/README.md` — Update two-crate pattern (lines 12-15)
  to three-crate pattern: nros-platform-<board> (primitives) +
  zpico-platform-shim (FFI) + board crate (lifecycle).

### 79.11 — Add `platform-cffi` feature

Add `platform-cffi` to the `nros` facade crate's platform axis, with mutual
exclusivity enforcement against other platform features.

### 79.12 — ~~Make RMW crates fully platform-agnostic~~ (abandoned)

Investigated and abandoned. The RMW crates are **inherently platform-aware**
for two reasons:

1. **zpico-sys FFI functions only exist when a platform is compiled.** The
   `shim` module in `nros-rmw-zenoh` imports `zpico_open`, `zpico_publish`,
   etc. which are only compiled when zpico-sys has a platform feature.
   Without the platform feature, the shim module can't compile. The
   `#[cfg(any(feature = "platform-*"))]` gates are structurally necessary.

2. **XRCE Zephyr transport is platform-specific.** `nros-rmw-xrce` gates
   `pub mod zephyr;` behind `platform-zephyr` because the module contains
   `extern "C"` declarations for Zephyr-only transport callbacks compiled
   by Zephyr's CMake build system.

The platform feature flow remains:
```
nros/platform-posix → nros-node → nros-rmw-zenoh/platform-posix → zpico-sys/posix
```

Phase 79's contribution to platform decoupling is the **Rust symbol layer**
(nros-platform traits + shim crates), not the C compilation layer. The C
compilation layer is an upstream zenoh-pico design constraint.

## Design Decisions

### Why capability sub-traits instead of one big trait?

XRCE-DDS only needs `PlatformClock`. Forcing it to require `PlatformAlloc +
PlatformThreading` would break the heap-less, single-threaded promise. Sub-traits
let each RMW shim declare exactly what it needs:

```rust
// zpico-platform-shim requires all capabilities
where P: PlatformClock + PlatformAlloc + PlatformSleep + PlatformRandom
       + PlatformTime + PlatformThreading

// xrce-platform-shim requires only clock
where P: PlatformClock
```

### Why keep networking outside the unified layer?

zenoh-pico wants BSD sockets or smoltcp. XRCE-DDS wants custom transport
callbacks. A future DDS backend might want raw UDP multicast. The networking
abstraction level differs too much between RMW backends to unify cleanly.

Instead, `PlatformNetworkPoll` covers the minimal shared concern (smoltcp poll
callback + clock for bare-metal). Actual socket/transport logic stays in the
RMW-specific transport crates (`zpico-smoltcp`, `xrce-smoltcp`).

### Two layers of platform awareness

Platform awareness in the RMW stack has two distinct layers:

1. **C compilation layer** (build-time) — the `-sys` crate needs `platform-*`
   features to select which C source files to compile and which `#define`s to
   set. This is a C preprocessor concern (struct layouts, `#ifdef` branches).
   Currently flows: `nros` → RMW crate → `-sys` crate.

2. **Rust symbol layer** (link-time) — the shim crate provides `extern "C"`
   symbols (`z_clock_now`, `uxr_millis`, etc.) that the compiled C library
   resolves at link time. This is fully decoupled from the RMW crate.

Phase 79.1–79.13 unified layer 2 for bare-metal. Phase 79.14 extends this to
ALL platforms by having zpico-sys skip its C platform files (`system.c`) and
use the shim instead. The C `network.c` (socket implementations) is kept
since networking is outside the nros-platform scope.

### Why use the shim for all platforms?

Before 79.14, RTOS/POSIX platforms used zenoh-pico's built-in C platform
files (`system.c`) while bare-metal used the shim. This meant two different
code paths for the same operations:

- **POSIX clock**: C `clock_gettime()` in `unix/system.c` vs Rust `libc::clock_gettime()` in `nros-platform-posix`
- **FreeRTOS mutex**: C `xSemaphoreCreateMutex()` in `freertos/system.c` vs Rust wrapper (future `nros-platform-freertos`)

By routing all platforms through the shim, we get:
- **One code path** for all platform operations
- **nros-platform crates are always the source of truth** — no behavioral divergence between C and Rust implementations
- **LTO eliminates overhead** — the `extern "C"` → Rust → libc/RTOS chain is inlined at link time in release builds
- **Testable** — platform implementations can be tested on the host, not just in QEMU/hardware

### Platform symbol providers per platform (79.14)

Each platform needs two categories of symbols: **system** (clock, malloc,
threading, sleep, random) and **socket** (TCP/UDP open, read, send, close).

| Platform   | System symbols                          | Socket symbols              | Notes                                                                    |
|------------|-----------------------------------------|-----------------------------|--------------------------------------------------------------------------|
| POSIX      | Shim → `nros-platform-posix`            | C `unix/network.c`          | POSIX APIs via `libc` crate                                              |
| NuttX      | Shim → `nros-platform-nuttx` (= posix)  | C `unix/network.c`          | POSIX-compatible                                                         |
| Bare-metal | Shim → `nros-platform-<board>`          | Shim `socket-stubs`         | No C runtime; zpico-smoltcp provides TCP/UDP                             |
| FreeRTOS   | C `freertos/system.c`                   | C `freertos/lwip/network.c` | Shim tested but hangs — condvar/task semantics need work                 |
| ThreadX    | Shim → `nros-platform-threadx` + C `task.c` | C `threadx/network.c`  | Task creation in C (struct layout); all else via shim                    |
| Zephyr     | Zephyr CMake module                     | Zephyr CMake module         | Entire platform compiled by Zephyr's build system                        |

**FreeRTOS macro workaround:** FreeRTOS exposes its API as C macros
(`xSemaphoreCreateRecursiveMutex` → `xQueueCreateMutex(4)`,
`xSemaphoreTakeRecursive` → `xQueueTakeMutexRecursive`, etc.). Rust FFI
can't call macros, but we can call the underlying real functions directly.
`nros-platform-freertos/src/ffi.rs` declares the `xQueue*` functions and
provides safe Rust wrappers. However, **E2E tests hang** when using the shim
because zenoh-pico's threading requires:
1. A task wrapper that signals an event group on completion + self-suspends
2. A condvar with atomic waiter counting using critical sections
3. Proper `pdMS_TO_TICKS()` conversion in sleep

The C `system.c` implements all of this correctly. The Rust shim's simpler
implementations don't match the required semantics. FreeRTOS stays on C
`system.c` until these are resolved.

**ThreadX:** Same situation — `nros-platform-threadx` exists but hasn't been
tested end-to-end. ThreadX stays on C `system.c` for now.

### Cross-archive linking solved via `extern crate` (79.14)

When zpico-sys's C objects reference `z_clock_now` etc., the linker needs
to find those symbols in the shim's rlib. Initially this failed because
the linker didn't search the shim's archive for symbols needed by C objects.

**Solution:** Add `extern crate zpico_platform_shim;` to zpico-sys's Rust
code. This forces rustc to include the shim's rlib in the link, making its
`extern "C"` symbols available to the C objects in the same link.

### System.c vs network.c split

zenoh-pico's platform C files serve two purposes:

1. **`system.c`** — clock, malloc, sleep, random, threading (tasks, mutexes, condvars)
2. **`network.c`** — TCP/UDP socket operations (BSD sockets, lwIP, NetX Duo)

The shim replaces `system.c` only. `network.c` is kept because:
- Networking is outside the nros-platform scope (different per transport)
- `network.c` doesn't conflict with shim symbols (different function names)
- Socket implementations vary by networking stack (BSD, lwIP, NetX), not by platform

### Why both Rust traits and C vtable?

- **Rust traits** — zero-cost, type-safe, natural for platforms with Rust HAL
  crates (ESP32, STM32, bare-metal)
- **C vtable** — natural for platforms where the SDK is C-only (FreeRTOS
  `pvPortMalloc`, ThreadX `tx_byte_allocate`). Avoids awkward Rust wrappers
  around C APIs that will just be called from C shims anyway.

This mirrors the existing `nros-rmw` (Rust trait) + `nros-rmw-cffi` (C vtable)
dual interface.

### RTOS platforms vs. bare-metal boards

For **RTOS platforms** (FreeRTOS, Zephyr, NuttX, ThreadX), the platform crate
is generic — `vTaskDelay`, `pthread_mutex_init`, `malloc` work the same
regardless of board. The board crate handles hardware-specific init.

For **bare-metal boards**, the platform crate must be per-board because the
clock reads specific hardware registers (CMSDK Timer0 for MPS2-AN385, DWT
for STM32F4, `esp_hal::time::Instant` for ESP32). The board crate owns
lifecycle (init, run, device management) and delegates to the platform crate
for system primitives.

```
RTOS:       nros-platform-freertos (generic) + nros-mps2-an385-freertos (board)
Bare-metal: nros-platform-mps2-an385 (per-board) + nros-mps2-an385 (board)
```

Generic building blocks shared across bare-metal boards (xorshift PRNG,
threading no-op stubs) are reimplemented in each platform crate for now.
A shared utility crate could extract these later if
the duplication becomes significant.

### Sleep and transport decoupling

On bare-metal, sleep must poll the network stack to avoid missing packets.
Rather than coupling the platform crate to a specific transport (smoltcp),
the sleep module provides a registerable poll callback:

```rust
// Board crate registers during init:
nros_platform_mps2_an385::sleep::set_poll_callback(smoltcp_poll_fn);
```

This keeps the platform crate transport-agnostic while allowing the board
crate to wire in the correct poll behavior.

## Acceptance Criteria

- [x] No per-RMW platform crates remain (`zpico-platform-*`, `xrce-platform-*` deleted)
- [x] ALL platforms route through nros-platform shim (POSIX, bare-metal, RTOS)
- [x] Board crates are RMW-agnostic (only depend on `nros-platform-<board>`)
- [x] Unit tests pass (337/337)
- [x] POSIX examples build and link via shim (`extern crate` force-link)
- [ ] `just test-qemu` passes with zenoh backend via shim
- [x] ThreadX uses shim (not C system.c) — 79.15
- [ ] Porting guide updated to document unified interface (79.10)
- [x] RTOS platform crates created: freertos, nuttx, threadx (79.14.5–7)

## Notes

- Zephyr is special: zenoh-pico has built-in Zephyr support via its own
  `system/zephyr/` platform layer compiled by CMake. The shim approach may
  need to coexist with or replace this built-in support.
- The `smoltcp_clock_now_ms` symbol is already shared between zpico-smoltcp
  and xrce-smoltcp — this validates the unification approach.
- This phase does not change the RMW trait layer or the executor. It only
  moves platform code from RMW-specific crates to shared crates.
