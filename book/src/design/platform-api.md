# Platform API Design

The platform API (`nros-platform`) sits below the RMW backend. It exposes the OS or hardware primitives that zenoh-pico and XRCE-DDS need: a clock, optionally a heap, optionally threading, optionally networking. This page explains *why* the trait surface is grouped the way it is and what the **behavior contract** is for each method. For trait signatures and the implementation status matrix, see [Platform API Reference](../reference/platform-api.md). For implementing a new platform, see [Custom Platform](../porting/custom-platform.md).

## Trait groups and rationale

The traits cluster into seven concern groups. Each group is independent: a platform can provide some and stub others, and the RMW backend declares which it actually needs.

### Time -- `PlatformClock`

Every backend needs a monotonic clock. zenoh-pico uses milliseconds for socket timeouts and lease management; it uses microseconds for finer-grained protocol math (KeepAlive intervals, lease expiry calculations). We expose **both** `clock_ms` and `clock_us` rather than a single `clock_ns` because:

- 32-bit MCUs without a hardware microsecond tick cannot serve `clock_ns` accurately. Synthesizing nanoseconds from a 1 ms tick is a lie that hides clock resolution from the caller.
- `u64` nanoseconds wraps after ~584 years, but on a Cortex-M0 every multiply-divide on a 64-bit value costs cycles that the lease task does not have to spare.
- The two functions can share a single hardware source: `clock_us` returns the raw counter, `clock_ms` divides by 1000. Platforms with a slow tick (1 ms) implement `clock_us` as `clock_ms * 1000` and accept the resolution loss.

The clock is **monotonic and wraparound-free for system lifetime**. There is no failure mode -- if the platform cannot produce time, it cannot run nano-ros at all.

### Memory -- `PlatformAlloc`

Only zenoh-pico needs `PlatformAlloc`. XRCE-DDS does not allocate. The trait is a thin malloc/realloc/free shim because zenoh-pico's internal buffer types expect that contract.

Bare-metal platforms back this with a bump allocator (`linked-list-allocator` or `embedded-alloc`). RTOS platforms back it with `pvPortMalloc` (FreeRTOS), `tx_byte_allocate` (ThreadX), `k_malloc` (Zephyr), or `malloc` (NuttX, POSIX). zenoh-pico's working set is ~64 KB total; the allocator must have at least that much budget, ideally more.

### Threading -- `PlatformThreading`

Three sub-areas: tasks (spawn/join/exit), mutexes (regular + recursive), and condition variables. Single-threaded targets (bare-metal) provide stub implementations: `task_init` returns -1 (so zenoh-pico's lease task spawn fails gracefully and the application drives lease-keepalive itself), and `mutex_lock`/`condvar_wait` are no-ops that always succeed.

The condvar API is the load-bearing one: zenoh-pico's blocking `z_get` and the C++ `Future::wait` both block on a condvar that the receive callback signals. On single-threaded platforms there is no thread to block, so the blocking C++ wait paths are not used (the [C++ action client status note](../reference/cpp-api.md) covers the migration to non-blocking polling).

### Sleep / Random / Wall-time

Three small zenoh-pico-only traits grouped for convenience:

- **`PlatformSleep`** -- delay APIs. On bare-metal with smoltcp, the implementation must call `network_poll()` while busy-waiting, otherwise packets queue and the lease times out.
- **`PlatformRandom`** -- a 32-bit xorshift PRNG seeded with hardware entropy (or a user-supplied seed). Used for session IDs and protocol nonces, not cryptography.
- **`PlatformTime`** -- wall-clock time for log timestamps. On bare-metal with no RTC, return monotonic time as a fallback.

### Networking -- `PlatformTcp` / `PlatformUdp` / `PlatformSocketHelpers`

zenoh-pico's network layer is split into three traits because the original C interface (`unix/network.c`) has three concerns:

- TCP and UDP each have their own `open`/`read`/`send`/`close` because the backend opens different socket types for each.
- `PlatformSocketHelpers` carries the cross-cutting operations -- `set_non_blocking`, `accept`, generic `close`, and `wait_event` -- that apply to either socket family.

Sockets and endpoints are opaque `*mut c_void` pointers; their underlying types vary per platform (POSIX `int`, lwIP `struct netconn*`, Zephyr socket descriptor, smoltcp `SocketHandle`). The shim layer auto-detects the type sizes from C headers at build time so the FFI boundary stays type-erased.

`PlatformUdpMulticast` is split out as a fourth networking trait because embedded targets that connect to a fixed `tcp/host:port` locator never multicast and should not pay the code-size cost of multicast plumbing.

### NetworkPoll -- `PlatformNetworkPoll`

Bare-metal only. `network_poll()` advances the smoltcp state machine, processing pending RX/TX. Platforms with kernel-level networking (Linux, lwIP-on-FreeRTOS, NetX-on-ThreadX, Zephyr sockets) drive their own NIC and don't need this.

`PlatformSleep` and the `wait_event` helper both call `network_poll()` while waiting, so packets keep flowing during otherwise-idle time. Without this hook, smoltcp would only receive when the application explicitly asked for it -- a recipe for dropped TCP segments.

### Libc -- `PlatformLibc`

zenoh-pico uses `strlen`, `memcpy`, `errno`, etc. directly. Bare-metal targets that link `picolibc` or `newlib-nano` get these for free. Targets without a C runtime (some `no_std` builds) provide the trait, which forwards to Rust implementations of the same functions.

This trait exists because the alternative -- patching zenoh-pico to call platform shims for every libc function -- would require modifying upstream sources we don't control.

## Why `clock_ms` *and* `clock_us`, not `clock_ns`

Summarized from above:

| API | Pros | Cons | Verdict |
|---|---|---|---|
| `clock_ns` only | Single function, finest resolution | 64-bit math on every call; lies on 1 ms-tick MCUs | Rejected |
| `clock_ms` only | Cheap, fits zenoh-pico's lease math | Insufficient resolution for sub-millisecond protocol timing | Insufficient |
| Both `clock_ms` *and* `clock_us` | Each call is cheap and honest about its resolution | Two functions to implement | **Chosen** |

## Behavior contracts

Each trait below has a contract table. Columns:

- **Method** -- name (matches the trait definition).
- **Blocking?** -- whether the method may suspend the caller.
- **May fail?** -- whether the method has a meaningful failure mode.
- **Unsupported fallback** -- what to do when the platform cannot provide the capability.
- **Notes** -- extra constraints (monotonicity, reentrancy, side effects).

### `PlatformClock`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `clock_ms` | No | No | Required for any backend | Monotonic; wraparound-free for system lifetime |
| `clock_us` | No | No | Required for any backend | Same monotonic base as `clock_ms` |

### `PlatformAlloc`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `alloc` | No | Yes (null) | Required for zenoh-pico | Caller checks for null and propagates as RMW error |
| `realloc` | No | Yes (null) | Required for zenoh-pico | Existing block must be preserved on failure |
| `dealloc` | No | No | Required for zenoh-pico | `dealloc(null)` is a no-op |

### `PlatformSleep`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `sleep_us` | Yes | No | Required for zenoh-pico | Bare-metal must call `network_poll()` during busy-wait |
| `sleep_ms` | Yes | No | Required for zenoh-pico | Same |
| `sleep_s` | Yes | No | Required for zenoh-pico | Same; typically implemented as `sleep_ms(s * 1000)` |

### `PlatformRandom`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `random_u8` / `_u16` / `_u32` / `_u64` | No | No | Required for zenoh-pico | xorshift32 is sufficient; not cryptographic |
| `random_fill` | No | No | Required for zenoh-pico | Fills `len` bytes; no upper bound check |

### `PlatformTime` (wall-clock)

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `time_now_ms` | No | No | Required for zenoh-pico | Return `clock_ms` if no RTC |
| `time_since_epoch` | No | No | Required for zenoh-pico | Return `(monotonic_s, 0)` if no RTC |

### `PlatformThreading` -- tasks

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `task_init` | No | Yes (-1) | Return -1 on single-threaded | zenoh-pico's lease task must degrade gracefully |
| `task_join` | Yes | Yes | Return 0 (success) | Single-threaded never spawned a task to join |
| `task_detach` | No | Yes | Return 0 | Same |
| `task_cancel` | No | Yes | Return 0 | Same |
| `task_exit` | No | No | No-op | Caller is the only thread |
| `task_free` | No | No | No-op | No allocation to free |

### `PlatformThreading` -- mutexes (regular and recursive)

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `mutex_init` / `mutex_rec_init` | No | Yes | Return 0 | Single-threaded has no mutex state |
| `mutex_drop` / `mutex_rec_drop` | No | Yes | Return 0 | Same |
| `mutex_lock` / `mutex_rec_lock` | Yes | Yes | Return 0 (success) | Single-threaded: no contention possible |
| `mutex_try_lock` / `mutex_rec_try_lock` | No | Yes | Return 0 | Always "succeeds" on single-threaded |
| `mutex_unlock` / `mutex_rec_unlock` | No | Yes | Return 0 | Same |

### `PlatformThreading` -- condition variables

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `condvar_init` | No | Yes | Return 0 | No state on single-threaded |
| `condvar_drop` | No | Yes | Return 0 | Same |
| `condvar_signal` | No | Yes | Return 0 | No waiter to wake |
| `condvar_signal_all` | No | Yes | Return 0 | Same |
| `condvar_wait` | Yes | Yes | Return 0 | Single-threaded must use polling instead -- avoid this path |
| `condvar_wait_until` | Yes | Yes (timeout) | Return 0 immediately | Same; blocking C++ `Future::wait` deadlocks on single-threaded (use non-blocking polling instead) |

### `PlatformTcp`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `create_endpoint` | Yes (DNS) | Yes | Required for zenoh-pico | Backed by `getaddrinfo` or platform equivalent |
| `free_endpoint` | No | No | Required | Mirrors `freeaddrinfo` |
| `open` | Yes | Yes | Required | Connect with timeout in ms |
| `listen` | No | Yes | Optional (server mode) | Bare-metal client typically returns -1 |
| `close` | No | No | Required | Shutdown + close |
| `read` | No (after `set_non_blocking`) | Yes (`usize::MAX`) | Required | Returns 0 if no data; **must be non-blocking** for zenoh-pico's poll loop |
| `read_exact` | Yes | Yes | Required | Used for length-prefixed framing |
| `send` | Yes | Yes | Required | May block on socket buffer full |

### `PlatformUdp`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `create_endpoint` | Yes (DNS) | Yes | Required for zenoh-pico | Same as TCP but `SOCK_DGRAM` |
| `free_endpoint` | No | No | Required | |
| `open` | No | Yes | Required | UDP socket open is non-blocking |
| `close` | No | No | Required | |
| `read` | No | Yes | Required | recvfrom; returns 0 if no datagram |
| `read_exact` | Yes | Yes | Required | Rarely used (UDP is message-oriented) |
| `send` | Yes | Yes | Required | sendto |

### `PlatformSocketHelpers`

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `set_non_blocking` | No | Yes | Required | **Critical**: enables non-blocking `read` |
| `accept` | No (after `set_non_blocking`) | Yes | Optional | Server-mode only |
| `close` | No | No | Required | Generic socket close |
| `wait_event` | Yes | Yes | Required | Multi-threaded: yields to scheduler. Single-threaded: spins + `network_poll` |

### `PlatformUdpMulticast` (optional)

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `mcast_open` | Yes | Yes | Stub returns -1 | Skipped on embedded targets without scouting |
| `mcast_listen` | Yes | Yes | Stub returns -1 | Same |
| `mcast_close` | No | No | No-op | |
| `mcast_read` / `mcast_read_exact` | Varies | Yes | Stub returns `usize::MAX` | |
| `mcast_send` | Yes | Yes | Stub returns `usize::MAX` | |

### `PlatformNetworkPoll` (bare-metal only)

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `network_poll` | No | No | OS-driven platforms: no-op | Advances smoltcp once; called by `sleep_*` and `wait_event` |

### `PlatformLibc` (bare-metal without libc)

| Method | Blocking? | May fail? | Unsupported fallback | Notes |
|---|---|---|---|---|
| `strlen`, `strcmp`, `strncmp`, `strchr`, `strncpy` | No | No | Linker provides if libc present | Same semantics as the C standard |
| `memcpy`, `memmove`, `memset`, `memcmp`, `memchr` | No | No | Same | Same |
| `strtoul` | No | Yes (errno) | Same | Used by zenoh-pico to parse locator strings |
| `errno_ptr` | No | No | Same | Returns pointer to thread-local (or static) errno |

## Cross-cutting rules

Two contract rules apply to every trait method:

1. **Reentrancy.** Methods may be called from any context the executor enters: a publisher callback, a service handler, or directly from user code. Implementations must not assume a particular calling thread or critical-section state. Single-threaded platforms get this for free; RTOS platforms must use reentrant primitives.

2. **No panics across the FFI boundary.** All trait methods are exposed to C through the shim crates. Panicking through C is undefined behavior. Implementations return error codes (or `usize::MAX` for byte counts) instead of panicking. The exception is `PlatformClock` -- if the clock cannot be read, the system is fundamentally broken and there is nothing useful to return.
