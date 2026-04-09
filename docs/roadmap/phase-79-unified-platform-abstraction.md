# Phase 79: Unified Platform Abstraction Layer

**Goal**: Define a single platform interface (`nros-platform`) that all RMW backends consume, eliminating per-RMW platform crates and making the RMW layer fully platform-agnostic.

**Status**: Not Started
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
       └── nros-platform (unified platform traits)
            ├── nros-platform-posix    (implements traits directly)
            ├── nros-platform-zephyr   (implements traits directly)
            ├── nros-platform-freertos (implements traits via cffi)
            └── nros-platform-cffi     (C vtable adapter)
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

- [ ] 79.1 — Create `nros-platform` trait crate with capability sub-traits
- [ ] 79.2 — Create `nros-platform-cffi` with C vtable registration
- [ ] 79.3 — Create `nros-platform-posix` (first native implementation)
- [ ] 79.4 — Create `zpico-platform-shim` (zenoh-pico forwarder)
- [ ] 79.5 — Create `xrce-platform-shim` (XRCE-DDS forwarder)
- [ ] 79.6 — Migrate `zpico-platform-mps2-an385` to `nros-platform-bare-metal` + board init
- [ ] 79.7 — Migrate `xrce-platform-mps2-an385` → verify same `nros-platform-bare-metal` works
- [ ] 79.8 — Migrate remaining platforms (FreeRTOS, NuttX, ThreadX, Zephyr, ESP32, STM32F4)
- [ ] 79.9 — Remove old per-RMW platform crates
- [ ] 79.10 — Update porting guide (book) to reflect unified interface
- [ ] 79.11 — Add `platform-cffi` feature to `nros` facade crate

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

### 79.6 — Migrate MPS2-AN385 bare-metal platform

Extract shared platform logic from `zpico-platform-mps2-an385` into
`nros-platform-bare-metal` (or a board-specific `nros-platform-mps2-an385`).
Board-specific init (timer setup, heap region) stays in the board crate.

Verify that `zpico-platform-shim` + `nros-platform-bare-metal` produces
identical behavior to the old `zpico-platform-mps2-an385`.

**Files:**
- `packages/boards/nros-platform-mps2-an385/` (new, or extend existing board crate)
- Remove `packages/zpico/zpico-platform-mps2-an385/`
- Remove `packages/xrce/xrce-platform-mps2-an385/`

### 79.7 — Verify XRCE on migrated MPS2-AN385

Run `just test-qemu` with XRCE backend on MPS2-AN385 using the same
`nros-platform-bare-metal` + `xrce-platform-shim`. This validates the
"write once, use for all RMW backends" promise.

### 79.8 — Migrate remaining platforms

Apply the same migration to:
- FreeRTOS (`zpico-platform-freertos` → `nros-platform-freertos`)
- NuttX (`zpico-platform-nuttx` → `nros-platform-nuttx`)
- ThreadX (`zpico-platform-threadx` → `nros-platform-threadx`)
- Zephyr (currently uses zenoh-pico's built-in Zephyr support — may need shim)
- ESP32 (`zpico-platform-esp32` → `nros-platform-esp32`)
- STM32F4 (`zpico-platform-stm32f4` → `nros-platform-stm32f4`)

### 79.9 — Remove old per-RMW platform crates

Delete all `zpico-platform-*` and `xrce-platform-*` crates. Update
dependency graph in all examples and board crates.

### 79.10 — Update porting guide

Rewrite `book/src/guides/porting-platform/` to reflect the unified interface.
The per-RMW subpages become reference appendices (symbol mappings); the main
guide focuses on implementing `nros-platform` traits.

### 79.11 — Add `platform-cffi` feature

Add `platform-cffi` to the `nros` facade crate's platform axis, with mutual
exclusivity enforcement against other platform features.

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

### Why both Rust traits and C vtable?

- **Rust traits** — zero-cost, type-safe, natural for platforms with Rust HAL
  crates (ESP32, STM32, bare-metal)
- **C vtable** — natural for platforms where the SDK is C-only (FreeRTOS
  `pvPortMalloc`, ThreadX `tx_byte_allocate`). Avoids awkward Rust wrappers
  around C APIs that will just be called from C shims anyway.

This mirrors the existing `nros-rmw` (Rust trait) + `nros-rmw-cffi` (C vtable)
dual interface.

### Board-specific vs. platform-generic

Some platform logic is board-specific (which hardware timer, heap region
address, DMA constraints). The split is:

- **`nros-platform-<rtos>`** — generic RTOS logic (e.g., `vTaskDelay`,
  `xSemaphoreCreateMutex` for FreeRTOS)
- **Board crate init** — hardware-specific setup (timer peripheral config,
  heap region, network device) called before platform traits are usable

For RTOS platforms, the platform crate is truly generic. For bare-metal, the
platform crate may need to be per-board (different timer peripherals).

## Acceptance Criteria

- [ ] `just test-qemu` passes with zenoh backend using `zpico-platform-shim` + `nros-platform-bare-metal`
- [ ] `just test-qemu` passes with XRCE backend using `xrce-platform-shim` + same `nros-platform-bare-metal`
- [ ] `just test-freertos` passes with unified `nros-platform-freertos`
- [ ] `just test-integration` passes (POSIX platform)
- [ ] No per-RMW platform crates remain (`zpico-platform-*`, `xrce-platform-*` deleted)
- [ ] Porting guide updated to document unified interface
- [ ] Adding a hypothetical new RMW backend requires zero platform crate changes

## Notes

- Zephyr is special: zenoh-pico has built-in Zephyr support via its own
  `system/zephyr/` platform layer compiled by CMake. The shim approach may
  need to coexist with or replace this built-in support.
- The `smoltcp_clock_now_ms` symbol is already shared between zpico-smoltcp
  and xrce-smoltcp — this validates the unification approach.
- This phase does not change the RMW trait layer or the executor. It only
  moves platform code from RMW-specific crates to shared crates.
