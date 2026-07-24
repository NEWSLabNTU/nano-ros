# Platform Differences

Per-platform behaviour comparison. The [Doxygen reference](../api/platform-cffi/index.html) defines what each vtable entry **must** do; this page covers **how** each shipped platform crate fulfils that contract and which optional capabilities each platform exposes.

## At a glance

| Platform | Clock source | Allocator | Threading | Networking | UDP multicast | Wall-clock | Notes |
|---|---|---|---|---|---|---|---|
| **POSIX** | `clock_gettime(CLOCK_MONOTONIC)` | libc `malloc` | pthreads | libc BSD sockets (Rust) | Yes | `clock_gettime(CLOCK_REALTIME)` | Canonical reference port. |
| **NuttX** | POSIX alias | libc `malloc` | pthreads | zenoh-pico `unix/network.c` (C) | Yes | POSIX | Most paths inherit POSIX behaviour. |
| **FreeRTOS** | `xTaskGetTickCount` | `pvPortMalloc` | FreeRTOS tasks | lwIP via `freertos-lwip-sys` (Rust) | — | Tick-based, no RTC | Multicast gated on lwIP `LWIP_IGMP=1` (untested). |
| **Zephyr** | `k_uptime_get` | `k_malloc` | Zephyr POSIX pthreads | Zephyr POSIX sockets (Rust) | Yes (native_sim via NSOS) | `clock_gettime(CLOCK_REALTIME)` | `native_sim` multicast routes through the NSOS `IPPROTO_IP` patch. |
| **ThreadX** | `tx_time_get` | `tx_byte_allocate` | ThreadX threads | NetX Duo BSD `network.c` (C) | — | `tx_time_get` fallback | Multicast gated on NetX Duo `nx_igmp_*` (untested). |
| **Bare-metal (MPS2-AN385)** | CMSDK Timer0 | bump allocator | single-threaded | nros-smoltcp (Rust) | Yes (smoltcp 0.12 IGMP) | monotonic fallback | Cortex-M3 QEMU. |
| **Bare-metal (STM32F4)** | DWT cycle counter | bump allocator | single-threaded | nros-smoltcp (Rust) | Yes | monotonic fallback | Cortex-M4F. |
| **Bare-metal (ESP32-S3)** | `esp_timer_get_time` | bump allocator | single-threaded | nros-smoltcp (Rust) | Yes | monotonic fallback | Xtensa LX7 (build-only board crate). |
| **Bare-metal (ESP32-C3 QEMU)** | `esp_timer_get_time` | bump allocator | single-threaded | nros-smoltcp (Rust) | Yes | monotonic fallback | RISC-V. |

> **Canonical interface contract.** What "Yes" / "—" / specific functions mean — buffer ownership, blocking allowance, valid value ranges — is in the [platform-cffi Doxygen reference](../api/platform-cffi/index.html). This page is a comparison table, not a spec.

## Behaviour notes by trait group

### Time (`PlatformClock`)

All ports surface a monotonic clock. Resolution varies:

- **POSIX / NuttX**: nanosecond-resolution `clock_gettime`, exposed at ms + µs.
- **FreeRTOS**: `xTaskGetTickCount` ticks (typically 1 ms). `clock_us` = `clock_ms × 1000` — the resolution lie is documented.
- **Zephyr**: `k_uptime_get` ms + `k_cyc_to_us_floor64(k_cycle_get_64())` µs.
- **ThreadX**: `tx_time_get` ticks (default 100 Hz; bump via `TX_TIMER_TICKS_PER_SECOND` for finer resolution).
- **Bare-metal (Cortex-M)**: hardware timer counter (CMSDK Timer0, DWT cycle counter). µs is the native resolution; ms is `µs / 1000`.
- **Bare-metal (ESP32)**: `esp_timer_get_time` returns µs natively.

The clock is monotonic and wraparound-free for the duration of `nros::init` → `nros::shutdown`. Platforms with 32-bit timers run a software extender on overflow.

### Memory (`PlatformAlloc`)

Only zenoh-pico calls `PlatformAlloc::{alloc, realloc, free}`. XRCE-DDS does not allocate. Recommended heap budget: **64 KB minimum** for zenoh-pico's working set; bare-metal ports typically allocate 128 KB+ via a bump allocator (`linked-list-allocator` or `embedded-alloc`).

The RTOS allocators (`pvPortMalloc`, `tx_byte_allocate`, `k_malloc`, libc `malloc`) honour their respective heap-region configurations. On bare-metal the heap region is defined in the linker script and consumed at startup by the bump allocator.

### Threading (`PlatformThreading`)

Multi-threaded ports (POSIX, NuttX, FreeRTOS, Zephyr, ThreadX) expose real tasks, mutexes, and condition variables. zenoh-pico's lease task and read task spawn into these. Single-threaded bare-metal ports stub `task_init` to return `-1` so the spawn fails gracefully; the application drives `spin_once` from a single context.

Recursive mutexes are required by zenoh-pico. FreeRTOS uses `xSemaphoreCreateRecursiveMutex`; ThreadX uses `tx_mutex_create(..., TX_INHERIT, ...)`; POSIX uses `PTHREAD_MUTEX_RECURSIVE`.

### Sleep / Random / Wall-time (`PlatformSleep`, `PlatformRandom`, `PlatformTime`)

`PlatformSleep` on RTOS ports uses the kernel's tickless sleep (`vTaskDelay`, `tx_thread_sleep`, `k_msleep`, libc `usleep`). On bare-metal with smoltcp, the sleep helper **also runs `network_poll()`** in a busy loop — packets must keep flowing during otherwise-idle waits or the zenoh lease times out.

`PlatformRandom` is a 32-bit xorshift PRNG seeded with hardware entropy (`getrandom`, RNG peripheral) or a build-time constant on platforms without entropy. Used for session IDs and protocol nonces; **not cryptographic**.

`PlatformTime` returns wall-clock for log timestamps. Bare-metal platforms with no RTC fall back to the monotonic clock — log timestamps then count from boot, not Unix epoch.

### Networking (`PlatformTcp`, `PlatformUdp`, `PlatformSocketHelpers`, `PlatformUdpMulticast`, `PlatformNetworkPoll`)

The split into four traits maps directly onto zenoh-pico's `unix/network.c` interface. Each port wires the four traits independently:

- **C-backed networking** (NuttX, ThreadX): the platform shim forwards into zenoh-pico's bundled `unix/network.c` (NuttX) or a NetX-Duo equivalent (ThreadX) — the C code talks to BSD-shape sockets directly.
- **Rust-backed networking** (POSIX, FreeRTOS, Zephyr, bare-metal): the platform crate implements TCP / UDP in Rust. POSIX wraps libc, FreeRTOS wraps lwIP via `freertos-lwip-sys`, Zephyr wraps the POSIX layer, bare-metal goes through `nros-smoltcp`.

`PlatformUdpMulticast` (RTPS SPDP, zenoh scouting) ships fully wired on POSIX, NuttX, Zephyr, and bare-metal (smoltcp 0.12 IGMP group join). FreeRTOS and ThreadX have no multicast yet — gated by lwIP's `IGMP=1` (untested) and NetX Duo's `nx_igmp_*` (untested).

`PlatformNetworkPoll` is bare-metal only. The implementation advances the smoltcp state machine; without it, smoltcp would only receive when the application explicitly asked. `PlatformSleep` and the `wait_event` helper both run `network_poll()` while waiting so packets continue flowing.

### Libc (`PlatformLibc`)

zenoh-pico uses `strlen`, `memcpy`, `errno`, etc. directly. Bare-metal targets that link `picolibc` or `newlib-nano` satisfy these for free; the trait exists for `no_std` targets without a C runtime, which forward to Rust implementations.

## Common pitfalls

For platform-specific gotchas (DMA buffer placement, ephemeral port conflicts, picolibc errno TLS, QEMU clock synchronization, Z_FEATURE_INTEREST mutex exhaustion, etc.), see [Platform Porting Pitfalls](../internals/platform-porting-pitfalls.md).

For the design rationale (why these trait groups, why both ms and µs, why split `PlatformNetworkPoll`), see [Platform API Design](../design/platform-api.md).

For writing a new port, see [Custom Platform](../porting/custom-platform.md). For the canonical interface spec, see the [Doxygen reference](../api/platform-cffi/index.html).
