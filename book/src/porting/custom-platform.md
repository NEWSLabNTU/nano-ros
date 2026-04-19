# Custom Platform

This guide walks through porting nano-ros to a new RTOS or bare-metal environment. A "platform" provides the OS-level primitives that nano-ros needs at runtime: clock, memory, sleep, threading, and networking. The core library is `#![no_std]` and makes zero platform calls directly -- everything flows through your platform crate.

## What you implement

All platform traits are defined in `nros-platform/src/traits.rs`. Your platform crate implements some or all of them as inherent methods on a zero-sized type (ZST). The set you need depends on your RMW backend.

### Required for all backends

| Trait | Methods | Purpose |
|---|---|---|
| `PlatformClock` | `clock_ms()`, `clock_us()` | Monotonic time. Must use a hardware timer or OS tick -- never a software counter that only advances when polled. |

### Required for zenoh-pico (rmw-zenoh)

| Trait | Methods | Purpose |
|---|---|---|
| `PlatformAlloc` | `alloc()`, `realloc()`, `dealloc()` | Heap allocation. zenoh-pico needs ~64 KB. |
| `PlatformSleep` | `sleep_us()`, `sleep_ms()`, `sleep_s()` | Delay. On bare-metal with smoltcp, poll the network during busy-wait. |
| `PlatformRandom` | `random_u8()` through `random_u64()`, `random_fill()` | PRNG for session IDs and protocol nonces. |
| `PlatformTime` | `time_now_ms()`, `time_since_epoch()` | Wall-clock time for logging. Return monotonic time if no RTC. |
| `PlatformThreading` | Tasks, mutexes, recursive mutexes, condvars (19 methods) | OS threading primitives. Single-threaded platforms provide no-op stubs. |

### Networking

| Trait | Methods | Purpose |
|---|---|---|
| `PlatformTcp` | `open()`, `read()`, `send()`, `close()`, ... | TCP client and server sockets. |
| `PlatformUdp` | `open()`, `read()`, `send()`, `close()`, ... | UDP unicast sockets. |
| `PlatformSocketHelpers` | `set_non_blocking()`, `accept()`, `close()`, `wait_event()` | Socket utility operations. |

### Optional

| Trait | When needed |
|---|---|
| `PlatformUdpMulticast` | Desktop platforms using zenoh scouting. Not needed for embedded client mode. |
| `PlatformNetworkPoll` | Bare-metal platforms using smoltcp. Called during sleep to process packets. |
| `PlatformLibc` | Bare-metal targets without a C runtime. Provides `strlen`, `memcpy`, etc. |

For full method signatures, see the [Platform API Reference](../reference/platform-api.md).

## Wiring into nros

Five files need changes to register a new platform. This example adds a fictional "MyOS" platform.

### 1. Create the platform crate

```
packages/core/nros-platform-myos/
  Cargo.toml
  src/
    lib.rs
```

The crate must have **zero** `nros-*` dependencies. It may depend on your RTOS bindings, HAL crates, or `embedded-alloc`.

### 2. Add the feature to nros-platform

In `packages/core/nros-platform/Cargo.toml`:

```toml
[features]
platform-myos = ["dep:nros-platform-myos"]

[dependencies]
nros-platform-myos = { version = "0.1.0", path = "../nros-platform-myos", optional = true }
```

### 3. Add the ConcretePlatform alias

In `packages/core/nros-platform/src/resolve.rs`:

```rust
#[cfg(feature = "platform-myos")]
pub type ConcretePlatform = nros_platform_myos::MyOsPlatform;
```

### 4. Propagate through the nros facade

In `packages/core/nros/Cargo.toml`, add `platform-myos` to the feature list so users can write `nros = { features = ["rmw-zenoh", "platform-myos"] }`.

### 5. Activate the shim(s)

Each RMW backend has its own platform shim crate. Enable the ones your
platform will support:

```toml
# packages/zpico/zpico-sys/Cargo.toml  (needed for rmw-zenoh)
[features]
myos = [
    "dep:zpico-platform-shim",
    "zpico-platform-shim?/active",
    "zpico-platform-shim?/network",
]

# packages/xrce/xrce-sys/Cargo.toml  (needed for rmw-xrce)
[features]
myos = [
    "dep:xrce-platform-shim",
    "xrce-platform-shim?/active",
]
```

Shim feature flags:

| Feature | When to enable |
|---------|----------------|
| `active` | Always, when this platform should claim the shim's symbols |
| `network` | Rust-native TCP/UDP path — forwards `_z_open_tcp`/`_z_read_tcp`/etc. to your `PlatformTcp`/`PlatformUdp` impl. Omit if you compile zenoh-pico's C `network.c` directly (see "Networking" below). |
| `skip-clock-symbols` | Your platform provides its own `_z_time_elapsed_us` etc. (e.g. NuttX ships clock functions in libc). Skips the shim's default clock forwarders to avoid duplicate-symbol link errors. |
| `network-smoltcp-bridge` | Bare-metal smoltcp integration (implies `network`). |

The shim picks up `ConcretePlatform` automatically through Cargo
feature unification — no changes inside the shim itself.

Build-script contract: the shim's `build.rs` reads
`DEP_ZPICO_SOCKET_SIZE` / `DEP_ZPICO_ENDPOINT_SIZE` (exported by
`zpico-sys`'s `build.rs`) so it can size socket-handle slabs correctly.
If you replace `zpico-sys` with a custom transport crate, export the
same `DEP_*` variables from its `build.rs` or the shim will fall back
to defaults.

## Rust path

This is the recommended approach. Create a ZST and implement each capability as inherent methods (not trait impls). The shim calls these methods directly through the `ConcretePlatform` type alias.

### Skeleton

```rust
// packages/core/nros-platform-myos/src/lib.rs
#![no_std]
use core::ffi::c_void;

/// Zero-sized type implementing platform methods for MyOS.
pub struct MyOsPlatform;

// -- Clock --
impl MyOsPlatform {
    pub fn clock_ms() -> u64 {
        // Call your RTOS tick API, e.g.:
        // unsafe { myos_get_tick_count() as u64 }
        todo!()
    }

    pub fn clock_us() -> u64 { Self::clock_ms() * 1000 }
}

// -- Alloc --
impl MyOsPlatform {
    pub fn alloc(size: usize) -> *mut c_void {
        // unsafe { myos_malloc(size) }
        todo!()
    }

    pub fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void {
        // If your RTOS lacks realloc: alloc new, copy, free old
        todo!()
    }

    pub fn dealloc(ptr: *mut c_void) {
        // unsafe { myos_free(ptr) }
        todo!()
    }
}

// -- Sleep --
impl MyOsPlatform {
    pub fn sleep_us(us: usize) { Self::sleep_ms(us.div_ceil(1000)); }
    pub fn sleep_ms(ms: usize) {
        // unsafe { myos_thread_sleep(ms as u32) }
        todo!()
    }
    pub fn sleep_s(s: usize) { Self::sleep_ms(s * 1000); }
}

// -- Threading (stubs for single-threaded, real impls for RTOS) --
impl MyOsPlatform {
    pub fn mutex_init(m: *mut c_void) -> i8 {
        // Create a mutex via your RTOS API. Store the handle in `m`.
        // Return 0 on success, -1 on failure.
        todo!()
    }
    pub fn mutex_lock(m: *mut c_void) -> i8 { todo!() }
    pub fn mutex_unlock(m: *mut c_void) -> i8 { todo!() }
    // ... remaining threading methods (see traits.rs for the full list)
}
```

### Key points

- **Inherent methods, not trait impls.** The shim calls `ConcretePlatform::clock_ms()` directly. The traits in `traits.rs` document the contract, but the ZST uses inherent `impl` blocks.
- **`c_void` pointers for handles.** Mutex, condvar, and task handles are opaque `#[repr(C)]` structs sized to hold your RTOS handle. Cast the `*mut c_void` to your internal type.
- **Recursive mutexes are required.** zenoh-pico locks the same mutex recursively. On FreeRTOS this maps to `xSemaphoreCreateRecursiveMutex`; on pthreads, `PTHREAD_MUTEX_RECURSIVE`.
- **Seed the PRNG.** A deterministic seed (like FreeRTOS `rand()` starting from 1) causes duplicate zenoh session IDs across QEMU instances. Seed from hardware entropy, IP address, or semihosting wall-clock.

### Reference implementation

`packages/core/nros-platform-freertos/src/lib.rs` is a complete real-world example covering all categories: clock via `xTaskGetTickCount`, heap via `pvPortMalloc`/`vPortFree`, sleep via `vTaskDelay`, xorshift32 PRNG, and full threading with tasks, recursive mutexes, and condvars built on counting semaphores.

## C/C++ path

If your platform is easier to implement in C, use the `nros-platform-cffi` adapter. It provides a vtable-based bridge so you never write Rust.

### 1. Fill in the vtable

The vtable struct (`nros_platform_vtable_t`) is currently defined in
Rust (`packages/core/nros-platform-cffi/src/lib.rs` as
`NrosPlatformVtable`, `#[repr(C)]`). A C header is not yet
auto-generated — until cbindgen produces one, mirror the struct
manually in C, matching the Rust field order exactly:

```c
// my_platform_vtable.h — hand-mirrored from NrosPlatformVtable
#include <stdint.h>
#include <stddef.h>

typedef struct nros_platform_vtable_t {
    // -- Clock --
    uint64_t (*clock_ms)(void);
    uint64_t (*clock_us)(void);
    // -- Alloc --
    void* (*alloc)(size_t size);
    void* (*realloc)(void* ptr, size_t size);
    void  (*dealloc)(void* ptr);
    // -- Sleep --
    void (*sleep_us)(size_t us);
    void (*sleep_ms)(size_t ms);
    void (*sleep_s)(size_t s);
    // -- Random / Time / Threading / Sockets follow in the order
    //    defined in nros-platform-cffi/src/lib.rs --
    /* ... */
} nros_platform_vtable_t;

extern int32_t nros_platform_cffi_register(const nros_platform_vtable_t* vtable);
```

Then fill in the callbacks:

```c
static uint64_t my_clock_ms(void) {
    return myos_get_ticks();  // your RTOS tick API
}

static void *my_alloc(size_t size) {
    return myos_malloc(size);
}

static void my_dealloc(void *ptr) {
    myos_free(ptr);
}

static void *my_realloc(void *ptr, size_t size) {
    return myos_realloc(ptr, size);
}

static int8_t my_mutex_init(void *m) {
    myos_mutex_t *mx = (myos_mutex_t *)m;
    *mx = myos_mutex_create();
    return (*mx != NULL) ? 0 : -1;
}

// ... fill in remaining function pointers

static const nros_platform_vtable_t my_vtable = {
    .clock_ms           = my_clock_ms,
    .clock_us           = my_clock_us,
    .alloc              = my_alloc,
    .realloc            = my_realloc,
    .dealloc            = my_dealloc,
    .sleep_us           = my_sleep_us,
    .sleep_ms           = my_sleep_ms,
    .sleep_s            = my_sleep_s,
    .random_u32         = my_random_u32,
    // ... all random, time, task, mutex, condvar fields (see platform_vtable.h)
    .mutex_init         = my_mutex_init,
    .mutex_lock         = my_mutex_lock,
    .mutex_unlock       = my_mutex_unlock,
    // ... remaining fields
};
```

### 2. Register at boot

Call `nros_platform_cffi_register()` before opening an executor session:

```c
int main(void) {
    myos_init();
    nros_platform_cffi_register(&my_vtable);

    // Now use the nano-ros C or C++ API normally
    nros_executor_t exec;
    nros_executor_open(&exec, &config);
    // ...
}
```

### 3. Build configuration

Enable the `platform-cffi` feature instead of a platform-specific feature:

```toml
nros = { features = ["rmw-zenoh", "platform-cffi"] }
```

All function pointers in the vtable are required. For capabilities your platform does not support (e.g., threading on single-threaded bare-metal), provide stubs that return 0 for mutex/condvar operations and -1 for `task_init`.

## Networking

There are two paths for providing TCP/UDP sockets to zenoh-pico.

### Option A: Rust networking (preferred)

Implement `PlatformTcp`, `PlatformUdp`, and `PlatformSocketHelpers` on your ZST. These methods map to your OS socket API (BSD sockets, lwIP, NetX Duo, etc.).

```rust
impl MyOsPlatform {
    pub fn tcp_open(sock: *mut c_void, endpoint: *const c_void, timeout_ms: u32) -> i8 {
        // Parse endpoint, call connect(), store fd in sock
        todo!()
    }

    pub fn tcp_read(sock: *const c_void, buf: *mut u8, len: usize) -> usize {
        // Call recv() on the socket fd
        todo!()
    }

    pub fn tcp_send(sock: *const c_void, buf: *const u8, len: usize) -> usize {
        // Call send() on the socket fd
        todo!()
    }

    pub fn tcp_close(sock: *mut c_void) {
        // Close the socket fd
    }
}
```

Activate the `network` shim feature in `zpico-sys` so the shim provides the `_z_open_tcp`, `_z_read_tcp`, etc. C symbols by forwarding to your Rust methods.

For bare-metal with smoltcp, use `zpico-smoltcp` as the networking driver. It provides `PlatformTcp` and `PlatformUdp` implementations using smoltcp's TCP/UDP sockets. Your platform crate implements `PlatformNetworkPoll` so the sleep loop can process packets.

### Option B: Keep zenoh-pico's C network.c

If your platform already has a working zenoh-pico `network.c` (e.g., `freertos/lwip/network.c` or `unix/network.c`), you can compile it directly instead of implementing the Rust networking traits.

In this case, do **not** activate the `network` shim feature in zpico-sys. Instead, link the appropriate `network.c` through your build system. The C file provides the `_z_open_tcp`, `_z_read_tcp`, etc. symbols directly, bypassing the Rust shim.

This is the approach used by platforms with mature C networking stacks (lwIP on FreeRTOS, BSD sockets on NuttX, NetX Duo on ThreadX).

## Common pitfalls

- **Poll-driven clocks.** If the clock only advances when you call a function, timeouts and keep-alives break silently. Use a free-running hardware timer.
- **Stack overflow on RTOS.** The `Executor` has an inline arena on the task stack. Use at least 16384 words (64 KB) for the application task on action examples.
- **Deterministic PRNG seeds.** Duplicate zenoh session IDs cause silent connection failures. Seed from a source that varies across instances.
- **Missing recursive mutexes.** zenoh-pico re-enters the same mutex. Non-recursive mutexes deadlock.
- **QEMU clock drift.** Use `-icount shift=auto` for QEMU targets so the virtual clock tracks wall time during WFI.

## Next steps

- [Custom Board Package](custom-board.md) -- create a board crate that ties your platform to specific hardware
- [Platform API Reference](../reference/platform-api.md) -- complete method signatures for all traits
- [Platform Porting Pitfalls](../internals/platform-porting-pitfalls.md) -- QEMU networking and timing issues
