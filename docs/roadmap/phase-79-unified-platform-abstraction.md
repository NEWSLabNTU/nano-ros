# Phase 79: Unified Platform Abstraction Layer

**Goal**: Define a single platform interface (`nros-platform`) that all RMW backends consume, eliminating per-RMW platform crates and making the RMW layer fully platform-agnostic.

**Status**: In Progress (79.1–79.7 done)
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
- [ ] 79.8 — Migrate remaining platforms (FreeRTOS, NuttX, ThreadX, Zephyr, ESP32, STM32F4)
- [ ] 79.9 — Remove old per-RMW platform crates
- [ ] 79.10 — Update porting guide (book) to reflect unified interface
  - [ ] 79.10.1 — Rewrite `porting-platform/README.md` overview to describe unified nros-platform interface
  - [ ] 79.10.2 — Rewrite `porting-platform/zenoh-pico.md` — focus on zpico-platform-shim, not raw symbols
  - [ ] 79.10.3 — Rewrite `porting-platform/xrce-dds.md` — focus on xrce-platform-shim, not raw symbols
  - [ ] 79.10.4 — Add `porting-platform/implementing-a-platform.md` — how to implement nros-platform traits
  - [ ] 79.10.5 — Update `concepts/platform-model.md` to describe the unified architecture
  - [ ] 79.10.6 — Update `concepts/architecture.md` diagrams to show nros-platform layer
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

### 79.10 — Update documentation

Rewrite book documentation to reflect the unified platform interface.

- **79.10.1** — `porting-platform/README.md`: Replace the current per-RMW
  comparison with a description of the unified `nros-platform` interface.
  Explain that porting now means implementing one platform crate, not N.
- **79.10.2** — `porting-platform/zenoh-pico.md`: Reframe as a reference for
  the zpico-platform-shim symbol mapping, not a porting guide.
- **79.10.3** — `porting-platform/xrce-dds.md`: Same — reference for
  xrce-platform-shim symbol mapping.
- **79.10.4** — New `porting-platform/implementing-a-platform.md`: The main
  porting guide. How to create an `nros-platform-<name>` crate, implement the
  required methods, wire it into the feature system.
- **79.10.5** — `concepts/platform-model.md`: Update the platform model
  concept page with the unified architecture diagram.
- **79.10.6** — `concepts/architecture.md`: Update architecture diagrams to
  show the nros-platform layer between nros-node and RMW backends.

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

- [ ] `just test-qemu` passes with zenoh backend using `zpico-platform-shim` + `nros-platform-mps2-an385`
- [ ] `just test-qemu` passes with XRCE backend using `xrce-platform-shim` + `nros-platform-mps2-an385`
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
