# Platform API Reference

nano-ros abstracts hardware and OS differences through the **nros-platform** trait system. Each platform (POSIX, FreeRTOS, NuttX, ThreadX, Zephyr, bare-metal) implements these traits once, and both RMW backends (zenoh-pico and XRCE-DDS) consume them through thin shim crates.

## Architecture

```mermaid
graph TD
    ZP["zenoh-pico (C)"] --> ZPSHIM["zpico-platform-shim<br/>(~80 extern C symbols)"]
    XRCE["XRCE-DDS (C)"] --> XSHIM["xrce-platform-shim<br/>(3 extern C symbols)"]
    ZPSHIM --> NP["nros-platform::ConcretePlatform"]
    XSHIM --> NP
    NP --> POSIX["nros-platform-posix"]
    NP --> FREERTOS["nros-platform-freertos"]
    NP --> NUTTX["nros-platform-nuttx"]
    NP --> THREADX["nros-platform-threadx"]
    NP --> ZEPHYR["nros-platform-zephyr"]
    NP --> BM["nros-platform-mps2-an385<br/>nros-platform-stm32f4<br/>nros-platform-esp32"]
```

## Platform Traits (`nros-platform`)

Platform implementations provide capabilities through independent sub-traits. Not all traits are required — each RMW backend declares what it needs.

### `PlatformClock` (required by all backends)

Monotonic clock. Must be backed by a hardware timer or OS tick.

| Method | Signature | Description |
|--------|-----------|-------------|
| `clock_ms` | `fn clock_ms() -> u64` | Monotonic time in milliseconds |
| `clock_us` | `fn clock_us() -> u64` | Monotonic time in microseconds |

### `PlatformAlloc` (zenoh-pico only)

Heap memory allocation. zenoh-pico requires ~64 KB heap for transport buffers.

| Method | Signature | Description |
|--------|-----------|-------------|
| `alloc` | `fn alloc(size: usize) -> *mut c_void` | Allocate `size` bytes; null on failure |
| `realloc` | `fn realloc(ptr: *mut c_void, size: usize) -> *mut c_void` | Reallocate block |
| `dealloc` | `fn dealloc(ptr: *mut c_void)` | Free block |

### `PlatformSleep` (zenoh-pico only)

| Method | Signature | Description |
|--------|-----------|-------------|
| `sleep_us` | `fn sleep_us(us: usize)` | Sleep for microseconds |
| `sleep_ms` | `fn sleep_ms(ms: usize)` | Sleep for milliseconds |
| `sleep_s` | `fn sleep_s(s: usize)` | Sleep for seconds |

> **Bare-metal note:** Implementations should poll the network stack (smoltcp) during busy-wait sleep to avoid missing packets.

### `PlatformRandom` (zenoh-pico only)

A simple xorshift32 PRNG is sufficient. Seed with hardware entropy during platform init.

| Method | Signature | Description |
|--------|-----------|-------------|
| `random_u8` | `fn random_u8() -> u8` | Random byte |
| `random_u16` | `fn random_u16() -> u16` | Random 16-bit |
| `random_u32` | `fn random_u32() -> u32` | Random 32-bit |
| `random_u64` | `fn random_u64() -> u64` | Random 64-bit |
| `random_fill` | `fn random_fill(buf: *mut c_void, len: usize)` | Fill buffer with random bytes |

### `PlatformTime` (zenoh-pico only)

Wall-clock time for logging. On bare-metal without an RTC, return monotonic time.

| Method | Signature | Description |
|--------|-----------|-------------|
| `time_now_ms` | `fn time_now_ms() -> u64` | System time in ms |
| `time_since_epoch` | `fn time_since_epoch() -> TimeSinceEpoch` | Seconds + nanoseconds since epoch |

### `PlatformThreading` (multi-threaded platforms)

Tasks, mutexes, and condition variables. Single-threaded platforms return no-ops (0) except `task_init` which returns -1.

**Tasks:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `task_init` | `fn task_init(task, attr, entry, arg) -> i8` | Spawn a new task; 0 = success |
| `task_join` | `fn task_join(task) -> i8` | Wait for task to complete |
| `task_detach` | `fn task_detach(task) -> i8` | Detach task |
| `task_cancel` | `fn task_cancel(task) -> i8` | Cancel task |
| `task_exit` | `fn task_exit()` | Exit current task |
| `task_free` | `fn task_free(task: *mut *mut TaskHandle)` | Free task resources |

**Mutexes:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `mutex_init` | `fn mutex_init(m) -> i8` | Create mutex |
| `mutex_drop` | `fn mutex_drop(m) -> i8` | Destroy mutex |
| `mutex_lock` | `fn mutex_lock(m) -> i8` | Lock (blocking) |
| `mutex_try_lock` | `fn mutex_try_lock(m) -> i8` | Try lock (non-blocking) |
| `mutex_unlock` | `fn mutex_unlock(m) -> i8` | Unlock |

Recursive mutex variants (`mutex_rec_*`) have the same signatures.

**Condition Variables:**

| Method | Signature | Description |
|--------|-----------|-------------|
| `condvar_init` | `fn condvar_init(cv) -> i8` | Create condvar |
| `condvar_drop` | `fn condvar_drop(cv) -> i8` | Destroy condvar |
| `condvar_signal` | `fn condvar_signal(cv) -> i8` | Wake one waiter |
| `condvar_signal_all` | `fn condvar_signal_all(cv) -> i8` | Wake all waiters |
| `condvar_wait` | `fn condvar_wait(cv, m) -> i8` | Wait (unlocks mutex, re-locks on wake) |
| `condvar_wait_until` | `fn condvar_wait_until(cv, m, abstime) -> i8` | Wait with timeout (ms since boot) |

### `PlatformNetworkPoll` (bare-metal only)

| Method | Signature | Description |
|--------|-----------|-------------|
| `network_poll` | `fn network_poll()` | Poll smoltcp network stack for pending I/O |

Not required for platforms with OS-level networking (POSIX, Zephyr, NuttX, FreeRTOS, ThreadX).

### `PlatformTcp` (zenoh-pico networking)

TCP socket operations. Socket and endpoint parameters are opaque `*mut c_void` pointers to platform-specific types whose sizes are auto-detected from C headers at build time.

| Method | Signature | Description |
|--------|-----------|-------------|
| `create_endpoint` | `fn create_endpoint(ep, address, port) -> i8` | Resolve address + port into an endpoint handle (getaddrinfo) |
| `free_endpoint` | `fn free_endpoint(ep)` | Free endpoint resources (freeaddrinfo) |
| `open` | `fn open(sock, endpoint, timeout_ms) -> i8` | Open a TCP client connection |
| `listen` | `fn listen(sock, endpoint) -> i8` | Open a TCP listening socket |
| `close` | `fn close(sock)` | Shutdown + close a TCP socket |
| `read` | `fn read(sock, buf, len) -> usize` | Read up to `len` bytes; `usize::MAX` on error |
| `read_exact` | `fn read_exact(sock, buf, len) -> usize` | Read exactly `len` bytes; `usize::MAX` on error |
| `send` | `fn send(sock, buf, len) -> usize` | Send `len` bytes; `usize::MAX` on error |

### `PlatformUdp` (zenoh-pico networking)

UDP unicast socket operations.

| Method | Signature | Description |
|--------|-----------|-------------|
| `create_endpoint` | `fn create_endpoint(ep, address, port) -> i8` | Resolve address + port (SOCK_DGRAM) |
| `free_endpoint` | `fn free_endpoint(ep)` | Free endpoint resources |
| `open` | `fn open(sock, endpoint, timeout_ms) -> i8` | Open a UDP socket |
| `close` | `fn close(sock)` | Close a UDP socket |
| `read` | `fn read(sock, buf, len) -> usize` | Receive up to `len` bytes (recvfrom) |
| `read_exact` | `fn read_exact(sock, buf, len) -> usize` | Receive exactly `len` bytes |
| `send` | `fn send(sock, buf, len, endpoint) -> usize` | Send `len` bytes to endpoint (sendto) |

### `PlatformSocketHelpers` (zenoh-pico networking)

Cross-cutting socket operations used by zenoh-pico's transport layer.

| Method | Signature | Description |
|--------|-----------|-------------|
| `set_non_blocking` | `fn set_non_blocking(sock) -> i8` | Set socket to non-blocking mode |
| `accept` | `fn accept(sock_in, sock_out) -> i8` | Accept a pending connection |
| `close` | `fn close(sock)` | Close a socket (shutdown + close) |
| `wait_event` | `fn wait_event(peers, mutex) -> i8` | Wait for socket events; yields on multi-threaded platforms |

### `PlatformUdpMulticast` (zenoh-pico networking, desktop platforms)

UDP multicast for zenoh scouting. Not required on embedded platforms that use `tcp/` locators with scouting disabled.

| Method | Signature | Description |
|--------|-----------|-------------|
| `mcast_open` | `fn mcast_open(sock, endpoint, lep, timeout_ms, iface) -> i8` | Open multicast socket + join group |
| `mcast_listen` | `fn mcast_listen(sock, endpoint, timeout_ms, iface, join) -> i8` | Listen on multicast group |
| `mcast_close` | `fn mcast_close(sockrecv, socksend, rep, lep)` | Leave group + close sockets |
| `mcast_read` | `fn mcast_read(sock, buf, len, lep, addr) -> usize` | Receive multicast datagram |
| `mcast_read_exact` | `fn mcast_read_exact(sock, buf, len, lep, addr) -> usize` | Receive exact bytes |
| `mcast_send` | `fn mcast_send(sock, buf, len, endpoint) -> usize` | Send multicast datagram |

### Networking implementation status

| Platform | TCP | UDP | Multicast | Socket Helpers | Implementation |
|----------|-----|-----|-----------|----------------|----------------|
| POSIX | Rust | Rust | Rust | Rust | `nros-platform-posix/src/net.rs` via libc |
| Bare-metal | Rust | Rust | — | Rust | Board platform crates via nros-smoltcp |
| FreeRTOS | Rust | Rust | — | Rust | `nros-platform-freertos/src/net.rs` via lwIP (freertos-lwip-sys) |
| Zephyr | Rust | Rust | stubbed | Rust | `nros-platform-zephyr/src/net.rs` via Zephyr POSIX sockets |
| NuttX | C | C | C | C | zenoh-pico `unix/network.c` (POSIX-compatible) |
| ThreadX | C | C | — | C | `c/platform/threadx/network.c` via NetX Duo BSD |

### `PlatformNetworkPoll` (bare-metal only)

| Method | Signature | Description |
|--------|-----------|-------------|
| `network_poll` | `fn network_poll()` | Poll smoltcp network stack for pending I/O |

Not required for platforms with OS-level networking (POSIX, Zephyr, NuttX, FreeRTOS, ThreadX).

### `PlatformLibc` (bare-metal only)

Standard C library functions needed by zenoh-pico on targets without a C runtime. Provides `strlen`, `strcmp`, `strncmp`, `strchr`, `strncpy`, `memcpy`, `memmove`, `memset`, `memcmp`, `memchr`, `strtoul`, `errno_ptr`.

## Zenoh-pico Shim Symbols (`zpico-platform-shim`)

The shim translates ~80 `extern "C"` symbols expected by zenoh-pico into calls on `ConcretePlatform`. These symbols are resolved at link time. The count breaks down as: 46 system symbols (always active) + ~25 networking symbols (active when the `network` feature is enabled) + 7–12 libc stubs (bare-metal only).

### Clock (7 symbols)

| C Symbol | Platform Method |
|----------|----------------|
| `z_clock_now` | `PlatformClock::clock_ms` |
| `z_clock_elapsed_us` | `PlatformClock::clock_us` |
| `z_clock_elapsed_ms` | `PlatformClock::clock_ms` |
| `z_clock_elapsed_s` | `PlatformClock::clock_ms` / 1000 |
| `z_clock_advance_us` | pointer arithmetic |
| `z_clock_advance_ms` | pointer arithmetic |
| `z_clock_advance_s` | pointer arithmetic |

### Memory (3 symbols)

| C Symbol | Platform Method |
|----------|----------------|
| `z_malloc` | `PlatformAlloc::alloc` |
| `z_realloc` | `PlatformAlloc::realloc` |
| `z_free` | `PlatformAlloc::dealloc` |

### Sleep (3 symbols)

| C Symbol | Platform Method |
|----------|----------------|
| `z_sleep_us` | `PlatformSleep::sleep_us` |
| `z_sleep_ms` | `PlatformSleep::sleep_ms` |
| `z_sleep_s` | `PlatformSleep::sleep_s` |

### Random (5 symbols)

| C Symbol | Platform Method |
|----------|----------------|
| `z_random_u8` | `PlatformRandom::random_u8` |
| `z_random_u16` | `PlatformRandom::random_u16` |
| `z_random_u32` | `PlatformRandom::random_u32` |
| `z_random_u64` | `PlatformRandom::random_u64` |
| `z_random_fill` | `PlatformRandom::random_fill` |

### Time (6 symbols)

| C Symbol | Platform Method |
|----------|----------------|
| `z_time_now` | `PlatformTime::time_now_ms` |
| `z_time_now_as_str` | formatted string from `time_since_epoch` |
| `z_time_elapsed_us` | `PlatformTime::time_now_ms` delta |
| `z_time_elapsed_ms` | `PlatformTime::time_now_ms` delta |
| `z_time_elapsed_s` | `PlatformTime::time_now_ms` delta |
| `_z_get_time_since_epoch` | `PlatformTime::time_since_epoch` |

### Threading (22 symbols)

| C Symbol | Platform Method |
|----------|----------------|
| `_z_task_init` | `PlatformThreading::task_init` |
| `_z_task_join` | `PlatformThreading::task_join` |
| `_z_task_detach` | `PlatformThreading::task_detach` |
| `_z_task_cancel` | `PlatformThreading::task_cancel` |
| `_z_task_exit` | `PlatformThreading::task_exit` |
| `_z_task_free` | `PlatformThreading::task_free` |
| `_z_mutex_init` | `PlatformThreading::mutex_init` |
| `_z_mutex_drop` | `PlatformThreading::mutex_drop` |
| `_z_mutex_lock` | `PlatformThreading::mutex_lock` |
| `_z_mutex_try_lock` | `PlatformThreading::mutex_try_lock` |
| `_z_mutex_unlock` | `PlatformThreading::mutex_unlock` |
| `_z_mutex_rec_init` | `PlatformThreading::mutex_rec_init` |
| `_z_mutex_rec_drop` | `PlatformThreading::mutex_rec_drop` |
| `_z_mutex_rec_lock` | `PlatformThreading::mutex_rec_lock` |
| `_z_mutex_rec_try_lock` | `PlatformThreading::mutex_rec_try_lock` |
| `_z_mutex_rec_unlock` | `PlatformThreading::mutex_rec_unlock` |
| `_z_condvar_init` | `PlatformThreading::condvar_init` |
| `_z_condvar_drop` | `PlatformThreading::condvar_drop` |
| `_z_condvar_signal` | `PlatformThreading::condvar_signal` |
| `_z_condvar_signal_all` | `PlatformThreading::condvar_signal_all` |
| `_z_condvar_wait` | `PlatformThreading::condvar_wait` |
| `_z_condvar_wait_until` | `PlatformThreading::condvar_wait_until` |

### Networking — TCP (8 symbols, `network` feature)

| C Symbol | Platform Method |
|----------|----------------|
| `_z_create_endpoint_tcp` | `PlatformTcp::create_endpoint` |
| `_z_free_endpoint_tcp` | `PlatformTcp::free_endpoint` |
| `_z_open_tcp` | `PlatformTcp::open` |
| `_z_listen_tcp` | `PlatformTcp::listen` |
| `_z_close_tcp` | `PlatformTcp::close` |
| `_z_read_tcp` | `PlatformTcp::read` |
| `_z_read_exact_tcp` | `PlatformTcp::read_exact` |
| `_z_send_tcp` | `PlatformTcp::send` |

### Networking — UDP unicast (8 symbols, `network` feature)

| C Symbol | Platform Method |
|----------|----------------|
| `_z_create_endpoint_udp` | `PlatformUdp::create_endpoint` |
| `_z_free_endpoint_udp` | `PlatformUdp::free_endpoint` |
| `_z_open_udp_unicast` | `PlatformUdp::open` |
| `_z_listen_udp_unicast` | stub (returns -1) |
| `_z_close_udp_unicast` | `PlatformUdp::close` |
| `_z_read_udp_unicast` | `PlatformUdp::read` |
| `_z_read_exact_udp_unicast` | `PlatformUdp::read_exact` |
| `_z_send_udp_unicast` | `PlatformUdp::send` |

### Networking — UDP multicast (6 symbols, `network` feature)

| C Symbol | Platform Method |
|----------|----------------|
| `_z_open_udp_multicast` | `PlatformUdpMulticast::mcast_open` |
| `_z_listen_udp_multicast` | `PlatformUdpMulticast::mcast_listen` |
| `_z_close_udp_multicast` | `PlatformUdpMulticast::mcast_close` |
| `_z_read_udp_multicast` | `PlatformUdpMulticast::mcast_read` |
| `_z_read_exact_udp_multicast` | `PlatformUdpMulticast::mcast_read_exact` |
| `_z_send_udp_multicast` | `PlatformUdpMulticast::mcast_send` |

### Networking — Socket helpers (4 symbols, `network` feature)

| C Symbol | Platform Method |
|----------|----------------|
| `_z_socket_set_non_blocking` | `PlatformSocketHelpers::set_non_blocking` |
| `_z_socket_accept` | `PlatformSocketHelpers::accept` |
| `_z_socket_close` | `PlatformSocketHelpers::close` |
| `_z_socket_wait_event` | `PlatformSocketHelpers::wait_event` |

### smoltcp bridge (bare-metal only, `network-smoltcp-bridge` feature)

| C Symbol | Description |
|----------|-------------|
| `smoltcp_init` | Initialize smoltcp bridge (called from board crate `init_hardware()`) |
| `smoltcp_cleanup` | Clean up smoltcp state |
| `smoltcp_poll` | Poll network stack + advance sockets |
| `smoltcp_clock_now_ms` | `PlatformClock::clock_ms` (for smoltcp driver timestamping) |

## XRCE-DDS Shim Symbols (`xrce-platform-shim`)

The XRCE-DDS shim is minimal — only 3 symbols:

| C Symbol | Platform Method |
|----------|----------------|
| `uxr_millis` | `PlatformClock::clock_ms` |
| `uxr_nanos` | `PlatformClock::clock_us` * 1000 |
| `smoltcp_clock_now_ms` | `PlatformClock::clock_ms` |

## Platform Implementations

| Crate | Target | Clock Source | Allocator | Threading | Networking |
|-------|--------|-------------|-----------|-----------|------------|
| `nros-platform-posix` | Linux/macOS | `clock_gettime` | libc `malloc` | pthreads | libc BSD sockets (Rust) |
| `nros-platform-nuttx` | NuttX QEMU | POSIX (alias) | libc `malloc` | pthreads | zenoh-pico C `network.c` |
| `nros-platform-freertos` | FreeRTOS | `xTaskGetTickCount` | `pvPortMalloc` | FreeRTOS tasks | lwIP via freertos-lwip-sys (Rust) |
| `nros-platform-threadx` | ThreadX | `tx_time_get` | `tx_byte_allocate` | ThreadX threads | NetX Duo C `network.c` |
| `nros-platform-zephyr` | Zephyr | `k_uptime_get` | `k_malloc` | Zephyr POSIX pthreads | Zephyr POSIX sockets (Rust) |
| `nros-platform-mps2-an385` | Cortex-M3 | CMSDK Timer0 | bump allocator | single-threaded | nros-smoltcp (Rust) |
| `nros-platform-stm32f4` | STM32F4 | DWT cycle counter | bump allocator | single-threaded | nros-smoltcp (Rust) |
| `nros-platform-esp32` | ESP32 | `esp_timer_get_time` | bump allocator | single-threaded | nros-smoltcp (Rust) |
| `nros-platform-esp32-qemu` | ESP32 QEMU | `esp_timer_get_time` | bump allocator | single-threaded | nros-smoltcp (Rust) |

## Compile-Time Resolution

Exactly one platform feature must be enabled. The `ConcretePlatform` type alias resolves to the active backend:

```rust
// In nros-platform/src/resolve.rs:
#[cfg(feature = "platform-posix")]
pub type ConcretePlatform = nros_platform_posix::PosixPlatform;

#[cfg(feature = "platform-freertos")]
pub type ConcretePlatform = nros_platform_freertos::FreeRtosPlatform;

#[cfg(feature = "platform-threadx")]
pub type ConcretePlatform = nros_platform_threadx::ThreadxPlatform;
// ... etc.
```

The shim crates use `ConcretePlatform` directly — no dynamic dispatch, no generics propagation:

```rust
// In zpico-platform-shim/src/shim.rs:
use nros_platform::ConcretePlatform;

#[unsafe(no_mangle)]
pub extern "C" fn z_clock_now() -> usize {
    ConcretePlatform::clock_ms() as usize
}
```
