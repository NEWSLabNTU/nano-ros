# Implementing a Platform

This guide explains how to create an `nros-platform-<name>` crate that
provides OS-level primitives for a new platform. The platform crate is
RMW-agnostic -- it knows nothing about zenoh-pico or XRCE-DDS.

## Architecture

The unified platform architecture has three layers:

```
Board crate (nros-<board>)
  └── nros-platform-<board>          # Platform primitives (clock, memory, sleep, random, threading)
       └── nros-platform             # PlatformOps trait + ConcretePlatform type alias
            ├── zpico-platform-shim  # (inside zpico-sys) maps z_* symbols to ConcretePlatform
            └── xrce-platform-shim   # (inside xrce-sys) maps uxr_* symbols to ConcretePlatform
```

The `nros-platform` trait crate defines the `PlatformOps` trait and a
`ConcretePlatform` type alias that resolves to the active platform crate
based on Cargo features. The shim layers inside `zpico-sys` and `xrce-sys`
provide the `#[unsafe(no_mangle)] extern "C"` FFI symbols that the C
transport libraries require, forwarding each call to `ConcretePlatform`.

## Crate structure

```
packages/core/nros-platform-<name>/
├── Cargo.toml
└── src/
    ├── lib.rs              # Re-exports, ConcretePlatform impl
    ├── clock.rs            # Monotonic clock (most critical)
    ├── memory.rs           # Heap allocation (z_malloc / z_free)
    ├── sleep.rs            # Sleep / busy-wait
    ├── random.rs           # PRNG or hardware RNG
    ├── threading.rs        # Tasks, mutexes, condvars (or no-op stubs)
    └── network.rs          # smoltcp poll callback (if using smoltcp)
```

`Cargo.toml` must have **zero** `nros-*` dependencies. It may depend on:
- Hardware HAL crate (e.g., `cortex-m`, `stm32f4xx-hal`, `esp-hal`)
- `zpico-smoltcp` (if using smoltcp networking)
- `embedded-alloc` (for heap on bare-metal)
- RTOS bindings crate (e.g., FreeRTOS, ThreadX bindings)

## Required methods

### Clock (`clock_ms`)

The clock is the most critical primitive. Both zenoh-pico and XRCE-DDS
use it for session keep-alive, query timeouts, and transport timeouts.
It **must** be backed by a hardware timer or OS tick -- never by a software
counter that only advances when polled.

```rust
/// Return the current monotonic time in milliseconds.
fn clock_ms() -> u64;
```

Implementation options:
- Hardware timer (SysTick, GPT, DWT cycle counter)
- OS tick API (`xTaskGetTickCount`, `k_uptime_get`, `clock_gettime`)

Handle 32-bit timer wraps by tracking a wrap count in an atomic or using
a 64-bit counter.

### Memory allocation

```rust
fn alloc(size: usize) -> *mut u8;
fn realloc(ptr: *mut u8, size: usize) -> *mut u8;
fn dealloc(ptr: *mut u8);
```

Options:
- `embedded-alloc` `FreeListHeap` (bare-metal)
- RTOS heap (`pvPortMalloc` / `tx_byte_allocate`)
- System `malloc` (POSIX, NuttX)

zenoh-pico requires heap. XRCE-DDS does not.

### Sleep

```rust
fn sleep_ms(ms: u64);
```

On bare-metal, busy-wait using the clock. If using smoltcp, poll the
network stack during the busy-wait loop to avoid missing packets. Register
the poll callback during board crate init.

On RTOS, delegate to `vTaskDelay` / `tx_thread_sleep` / `k_sleep`.

### Random

```rust
fn random_u32() -> u32;
fn random_fill(buf: &mut [u8]);
```

A simple xorshift32 PRNG is sufficient. Seed with hardware entropy
(RNG peripheral, ADC noise, semihosting wall-clock time) during init.

### Threading

For single-threaded platforms (bare-metal), provide no-op stubs that
return success for mutex/condvar operations and return error for task
creation (not supported).

For RTOS platforms, implement real task/mutex/condvar operations by
mapping to the RTOS API (`xTaskCreate`, `tx_thread_create`,
`pthread_create`, etc.).

zenoh-pico requires recursive mutexes on RTOS platforms
(`configUSE_RECURSIVE_MUTEXES=1` on FreeRTOS).

## Wiring into nros-platform

After creating the platform crate:

1. **Add a feature** to `nros-platform/Cargo.toml`:

   ```toml
   [features]
   mps2-an385 = ["nros-platform-mps2-an385"]

   [dependencies]
   nros-platform-mps2-an385 = { path = "../nros-platform-mps2-an385", optional = true }
   ```

2. **Add the `ConcretePlatform` alias** in `nros-platform/src/lib.rs`:

   ```rust
   #[cfg(feature = "mps2-an385")]
   pub type ConcretePlatform = nros_platform_mps2_an385::Platform;
   ```

3. **Propagate the feature** through `nros` facade and `nros-node`.

## Sleep poll callback (bare-metal)

On bare-metal platforms using smoltcp, the sleep implementation must poll
the network stack during busy-wait loops. Register the poll callback in
the board crate's init sequence:

```rust
// In the board crate's init_hardware():
nros_platform_mps2_an385::set_sleep_poll(|| {
    smoltcp_network_poll();
});
```

This ensures network packets are processed even during sleep periods.

## Reference implementation

`nros-platform-mps2-an385` is the simplest reference. It provides:

- **Clock**: CMSDK APB Timer0 (25 MHz) with 32-bit wrap detection
- **Memory**: `embedded-alloc` `FreeListHeap` (64 KB heap)
- **Sleep**: Busy-wait with smoltcp poll callback
- **Random**: xorshift32 seeded from IP address
- **Threading**: No-op stubs (single-threaded bare-metal)
- **Network**: smoltcp poll via registered callback

Source: `packages/core/nros-platform-mps2-an385/`

## How symbols flow: system.c vs shim

zenoh-pico's C code needs two categories of symbols at link time:

1. **System symbols** (`z_clock_now`, `z_malloc`, `_z_mutex_lock`, `z_sleep_ms`, ...)
   — clock, memory, threading, sleep, random
2. **Socket symbols** (`_z_socket_close`, `_z_open_tcp`, `_z_read_tcp`, ...)
   — TCP/UDP networking

### System symbols

Most platforms use the **shim** path: `zpico-platform-shim` inside `zpico-sys`
provides all system `extern "C"` symbols, forwarding them to your
`nros-platform-<name>` crate via `ConcretePlatform`:

```
zenoh-pico C code → z_clock_now() → zpico-platform-shim → ConcretePlatform::clock_ms()
```

zenoh-pico's C `system.c` is **not compiled** — deleted from the build copy
before CMake runs, or skipped in the cc-based builds.

**Exception:** ThreadX and Zephyr still use their own C `system.c` because
their APIs require platform-specific adaptations not yet ported to Rust.

### Socket symbols

Socket symbols come from **transport-specific** code, not from the platform
crate:

| Transport | Socket provider | When |
|-----------|----------------|------|
| zpico-smoltcp | `zpico-smoltcp` crate (TCP/UDP) + shim `socket-stubs` (helpers) | Bare-metal ethernet |
| lwIP | C `freertos/lwip/network.c` | FreeRTOS |
| BSD sockets | C `unix/network.c` | POSIX, NuttX |
| NetX Duo | C `threadx/network.c` | ThreadX |
| Serial | zpico-serial (no TCP sockets) | Bare-metal serial |

The shim provides `_z_socket_close`, `_z_socket_set_non_blocking`,
`_z_socket_accept`, `_z_socket_wait_event` stubs for bare-metal. When
smoltcp is active (ethernet transport), `_z_socket_close` forwards to
`_z_close_tcp` from `zpico-smoltcp`. When serial-only (no smoltcp),
socket stubs are no-ops.

### FreeRTOS C macro workaround

FreeRTOS exposes its API as C preprocessor macros:
- `xSemaphoreCreateRecursiveMutex()` → `xQueueCreateMutex(4)`
- `xSemaphoreTakeRecursive(h, t)` → `xQueueTakeMutexRecursive(h, t)`
- `xSemaphoreGive(h)` → `xQueueGenericSend(h, NULL, 0, 0)`

Rust FFI can't call macros, but the underlying `xQueue*` functions are
real symbols. `nros-platform-freertos/src/ffi.rs` declares these real
functions and provides safe Rust wrappers.

## Common pitfalls

- **Poll-driven clocks**: Never increment the clock only when polled.
  Use a free-running hardware timer. See
  [Platform Porting Pitfalls](../platform-porting-pitfalls.md).
- **Stack overflow on RTOS**: The `Executor` has an inline arena on the
  task stack. Ensure the application task stack is large enough (e.g.,
  16384 words / 64 KB for action examples on FreeRTOS).
- **Missing recursive mutexes**: zenoh-pico uses recursive mutex locking.
  Ensure `_z_mutex_rec_*` functions map to real recursive mutexes on RTOS.
- **QEMU clock sync**: Use `-icount shift=auto` to synchronize QEMU's
  virtual clock with wall-clock time during WFI.
