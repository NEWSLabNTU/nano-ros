# Platform API Design

This page is for a contributor changing nano-ros's platform layer: adding a
capability to the ABI, or deciding where a new kernel service belongs. It
explains why the surface is grouped and shaped the way it is. A porter who
wants to implement the ABI for a new RTOS reads
[Add a platform](../../extending/add-a-platform.html) instead.

The platform API sits below the RMW backend. It exposes the OS or hardware
primitives that zenoh-pico and XRCE-DDS need: a clock, optionally a heap,
optionally threading, optionally networking.

> **The headers are the specification.** The signatures, ownership rules,
> blocking allowance and failure modes of every symbol live in
> `packages/platform/nros-platform-api/include/nros/` (`platform.h`,
> `platform_net.h`, `platform_timer.h`) and the
> [Doxygen built from them](../../api/platform-cffi/index.html). This page is
> rationale, not specification: where the two disagree, the headers win. The
> Rust traits in `nros-platform-api` mirror the C signatures.

## Trait groups and rationale

The traits cluster into seven concern groups. Each group is independent: a platform can provide some and stub others, and the RMW backend declares which it actually needs.

### Time -- `PlatformClock`

Every backend needs a monotonic clock. The ABI states it once, in
nanoseconds: `nros_platform_clock_ns()`, with
`nros_platform_clock_resolution_ns()` beside it so a port on a 1 ms tick says
so instead of implying nanosecond precision. A single unit keeps every
consumer's arithmetic in one place; the resolution symbol keeps the unit
honest. There is no failure mode: a platform that cannot produce time cannot
run nano-ros at all.

### Memory -- `PlatformAlloc`

Only zenoh-pico needs `PlatformAlloc`. XRCE-DDS does not allocate. The trait is a thin malloc/realloc/free shim because zenoh-pico's internal buffer types expect that contract.

Bare-metal platforms back this with a bump allocator (`linked-list-allocator` or `embedded-alloc`). RTOS platforms back it with `pvPortMalloc` (FreeRTOS), `tx_byte_allocate` (ThreadX), `k_malloc` (Zephyr), or `malloc` (NuttX, POSIX). zenoh-pico's working set is ~64 KB total; the allocator must have at least that much budget, ideally more.

### Threading -- `PlatformThreading`

Three sub-areas: tasks (spawn/join/exit), mutexes (regular + recursive), and condition variables. Single-threaded targets (bare-metal) provide stub implementations: `task_init` returns -1 (so zenoh-pico's background-task spawn fails gracefully and the executor drives the session itself), and `mutex_lock`/`condvar_wait_until` are no-ops that always succeed. There is no unbounded `condvar_wait` in the ABI: a caller whose contract is unbounded loops over `condvar_wait_until` and re-tests its predicate.

The condvar API is the load-bearing one: zenoh-pico's blocking `z_get` and the C++ `Future::wait` both block on a condvar that the receive callback signals. On single-threaded platforms there is no thread to block, so the blocking C++ wait paths are not used (the [C++ action client status note](../../reference/cpp-api.html) covers the migration to non-blocking polling).

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

## Cross-cutting rules

Two contract rules apply to every trait method:

1. **Reentrancy.** Methods may be called from any context the executor enters: a publisher callback, a service handler, or directly from user code. Implementations must not assume a particular calling thread or critical-section state. Single-threaded platforms get this for free; RTOS platforms must use reentrant primitives.

2. **No panics across the FFI boundary.** All trait methods are exposed to C through the shim crates. Panicking through C is undefined behavior. Implementations return error codes (or `usize::MAX` for byte counts) instead of panicking. The exception is `PlatformClock` -- if the clock cannot be read, the system is fundamentally broken and there is nothing useful to return.

## The scalar / opaque-struct boundary

Not every kernel service unifies the same way. The ABI splits into two
classes, and the split is a **design boundary, not tech debt**:

- **Scalar services — fully unified.** Alloc, sleep, clock, time, yield,
  random. Their ABI is plain scalars (sizes, millisecond counts, byte
  buffers) with no host-type in the signature, so a single canonical
  `nros_platform_*` symbol works for every RTOS and every consumer
  (core, RMW, vendored zenoh-pico/XRCE) funnels through it. The allocation gate
  (`check-no-direct-kernel-alloc.sh`) enforces this for allocation; the
  same pattern extends to the other scalars (the vendored `z_*`/`uxr_*`
  funnel is checked by a CI relink lane).

- **Opaque-struct services — stay per-RTOS-vendored.** Threading
  (`mutex`/`condvar`/`task`), the wake primitive, and network sockets
  carry RTOS-defined structs (`SemaphoreHandle_t`, `TX_MUTEX`,
  `struct k_mutex`, socket/`NX_*` control blocks) whose layout the
  vendor owns. A single C ABI cannot name those types portably, so each
  port vendors its own body and nano-ros exposes only opaque storage
  (`*_storage_size`/`*_storage_align` + an init/drop pair). This is why
  the ThreadX board's `tx_byte_allocate` thread-stack / NetX-pool sites
  are **not** allocation-gate violations — they are the *task* and *net* opaque
  services, allowed on a documented symbol-scoped lint allowlist.

**Escape hatch (if an opaque service must move later).** The way out is
NOT to widen the C ABI with host types; it is to pin a **canonical
fixed-layout struct** in `platform.h`, give each port a compile-time
`size_probe` / `_Static_assert` that the canonical layout is ≥ the
vendor struct (so opaque storage stays sound), and let the port
memcpy/place the vendor object inside the canonical slot. **Net is the
first candidate** — `platform_net.h` already exposes 29 symbols over an
opaque socket handle — but it is deferred (out of scope for the platform split)
until there is a consumer that needs portable socket structs. Threads
and sync are the least likely to move (deepest struct coupling).

## Why free symbols (not a vtable struct)

The RMW layer uses a `NrosRmwVtable` fn-ptr struct because a single
binary registers multiple backends at runtime (bridge nodes). Platforms
are different: **one platform per binary, resolved at link time**. A
vtable would add an indirection on every clock read, every mutex lock,
every socket send — overhead with no upside, because there is nobody to
swap. Free symbols let the linker resolve calls direct, let LTO inline
across the boundary, and let static analysis treat them like any other
extern.

The full rationale is in
[`docs/design/0006-portable-rmw-platform-interface.md`](../../../docs/design/0006-portable-rmw-platform-interface.md)
under "Platform ABI: free symbols (no vtable)".
