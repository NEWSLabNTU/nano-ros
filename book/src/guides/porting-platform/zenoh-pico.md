# Zenoh-pico Symbol Reference

This page documents the ~55 FFI symbols that zenoh-pico requires at link time.
These symbols are provided by `zpico-platform-shim` (inside `zpico-sys`), which
forwards each `z_*` / `_z_*` call to the `ConcretePlatform` type alias from
`nros-platform`. When porting to a new platform, you implement an
`nros-platform-<name>` crate (see [Implementing a Platform](./implementing-a-platform.md))
rather than providing these symbols directly.

This page serves as a reference for understanding what the shim layer maps
and what capabilities your `nros-platform-<name>` crate must provide.

## Platform crate structure

```
packages/core/nros-platform-<name>/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── clock.rs
    ├── memory.rs
    ├── sleep.rs
    ├── random.rs
    ├── time.rs
    ├── threading.rs          # no-op stubs or real RTOS impl
    ├── socket_stubs.rs       # if using smoltcp
    └── network.rs            # if using smoltcp
```

`Cargo.toml` must have **zero** `nros-*` dependencies. It may depend on:
- Hardware HAL crate (e.g., `stm32f4xx-hal`, `esp-hal`)
- `zpico-smoltcp` (if using smoltcp networking)
- `embedded-alloc` (for heap on bare-metal)
- RTOS bindings crate

## Required FFI symbols

### Clock (critical)

The clock is the most important primitive. zenoh-pico uses it for session
keep-alive, query timeouts, and transport timeouts. It **must** be backed
by a hardware timer or OS tick — never by a software counter that only
advances when polled.

```c
// Monotonic clock — returns an opaque timestamp (lower 32 bits of ms)
usize   z_clock_now(void);

// Elapsed time since a previous z_clock_now() value
c_ulong z_clock_elapsed_us(usize *time);
c_ulong z_clock_elapsed_ms(usize *time);
c_ulong z_clock_elapsed_s(usize *time);

// Advance a timestamp value by a duration
void    z_clock_advance_us(usize *clock, c_ulong duration);
void    z_clock_advance_ms(usize *clock, c_ulong duration);
void    z_clock_advance_s(usize *clock, c_ulong duration);
```

If using smoltcp (bare-metal networking), also provide:

```c
// Called by zpico-smoltcp for TCP/IP timestamping
u64 smoltcp_clock_now_ms(void);
```

**Implementation checklist:**

1. Identify a hardware timer (SysTick, GPT, DWT) or use the OS tick API
   (`xTaskGetTickCount`, `k_uptime_get`, `clock_gettime`)
2. Handle 32-bit timer wraps — track a wrap count in an atomic or use a
   64-bit counter
3. Never advance the clock inside `smoltcp_network_poll()` — read the
   hardware timer directly
4. Verify with QEMU: use `-icount shift=auto` to synchronize virtual
   time with wall-clock time

**Reference implementations:**

| Platform | Clock source | File |
|----------|-------------|------|
| MPS2-AN385 | CMSDK APB Timer0 (25 MHz) | `nros-platform-mps2-an385/src/clock.rs` |
| STM32F4 | ARM DWT cycle counter | `nros-platform-stm32f4/src/clock.rs` |
| ESP32-C3 | `esp_hal::time::Instant` | `nros-platform-esp32/src/clock.rs` |
| FreeRTOS | `xTaskGetTickCount()` | Use OS tick directly |
| NuttX | `clock_gettime(CLOCK_MONOTONIC)` | POSIX API |

### Memory

zenoh-pico allocates heap during session open and entity creation. Typical
minimum: 64 KB heap.

```c
void *z_malloc(usize size);
void *z_realloc(void *ptr, usize size);
void  z_free(void *ptr);
```

**Options:**
- `embedded-alloc` `FreeListHeap` (bare-metal)
- RTOS heap (`pvPortMalloc` / `tx_byte_allocate`)
- System `malloc` (POSIX, NuttX)

### Sleep

```c
i8 z_sleep_us(usize time);
i8 z_sleep_ms(usize time);
i8 z_sleep_s(usize time);
```

All return `0` (`_Z_RES_OK`). On bare-metal, busy-wait using the clock.
On RTOS, delegate to `vTaskDelay` / `tx_thread_sleep` / `k_sleep`.

If using smoltcp, poll the network stack during busy-wait sleep to avoid
missing packets.

### Random

zenoh-pico needs randomness for session IDs, SN initialization, and
scouting nonces.

```c
u8   z_random_u8(void);
u16  z_random_u16(void);
u32  z_random_u32(void);
u64  z_random_u64(void);
void z_random_fill(void *buf, usize len);
```

A simple xorshift32 PRNG is sufficient. Seed it with hardware entropy
(RNG peripheral, ADC noise, semihosting wall-clock time) during init.

### Time

System time (wall clock). Used for logging and `z_time_now_as_str()`.

```c
u64         z_time_now(void);
const char *z_time_now_as_str(char *buf, c_ulong buflen);
c_ulong     z_time_elapsed_us(u64 *time);
c_ulong     z_time_elapsed_ms(u64 *time);
c_ulong     z_time_elapsed_s(u64 *time);
i8          _z_get_time_since_epoch(ZTimeSinceEpoch *t);
```

Where `ZTimeSinceEpoch` is:

```c
#[repr(C)]
struct ZTimeSinceEpoch {
    u32 secs,
    u32 nanos,
}
```

On bare-metal without an RTC, return monotonic time or zeros.

### Threading

For single-threaded platforms (bare-metal, RTIC), provide no-op stubs.
For RTOS platforms, implement real task/mutex/condvar operations.

**Task operations:**

```c
i8   _z_task_init(ZTask *task, ZTaskAttr *attr, void*(*fun)(void*), void *arg);
i8   _z_task_join(ZTask *task);
i8   _z_task_detach(ZTask *task);
i8   _z_task_cancel(ZTask *task);
void _z_task_exit(void);
void _z_task_free(ZTask **task);
```

**Mutex operations:**

```c
i8 _z_mutex_init(ZMutex *m);
i8 _z_mutex_drop(ZMutex *m);
i8 _z_mutex_lock(ZMutex *m);
i8 _z_mutex_try_lock(ZMutex *m);
i8 _z_mutex_unlock(ZMutex *m);
```

**Recursive mutex operations:**

```c
i8 _z_mutex_rec_init(ZMutexRec *m);
i8 _z_mutex_rec_drop(ZMutexRec *m);
i8 _z_mutex_rec_lock(ZMutexRec *m);
i8 _z_mutex_rec_try_lock(ZMutexRec *m);
i8 _z_mutex_rec_unlock(ZMutexRec *m);
```

**Condition variable operations:**

```c
i8 _z_condvar_init(ZCondvar *cv);
i8 _z_condvar_drop(ZCondvar *cv);
i8 _z_condvar_signal(ZCondvar *cv);
i8 _z_condvar_signal_all(ZCondvar *cv);
i8 _z_condvar_wait(ZCondvar *cv, ZMutex *m);
i8 _z_condvar_wait_until(ZCondvar *cv, ZMutex *m, u64 *abstime);
```

**Single-threaded stubs:** Return `0` for all mutex/condvar operations.
Return `-1` for `_z_task_init` (task creation is not supported).

**RTOS implementations:** Map to `xTaskCreate`/`xSemaphoreCreateMutex`
(FreeRTOS), `tx_thread_create`/`tx_mutex_create` (ThreadX), or
`pthread_create`/`pthread_mutex_init` (NuttX, POSIX). zenoh-pico requires
recursive mutexes (`configUSE_RECURSIVE_MUTEXES=1` on FreeRTOS).

### Sockets

If using smoltcp (bare-metal), socket operations are handled by
`zpico-smoltcp`. Your platform crate provides thin shims:

```c
#[repr(C)]
struct ZSysNetSocket { i8 _handle, bool _connected }

i8   _z_socket_set_non_blocking(const ZSysNetSocket *sock);  // Return 0
i8   _z_socket_accept(const ZSysNetSocket *in, ZSysNetSocket *out);  // Return -1
void _z_socket_close(ZSysNetSocket *sock);
i8   _z_socket_wait_event(void *peers, ZMutexRecRef *mutex);  // Return 0
```

If using OS sockets (POSIX, NuttX, Zephyr), zenoh-pico's built-in socket
layer handles everything — no socket stubs needed.

### libc stubs (bare-metal only)

Bare-metal targets without a C runtime need standard C library functions:

```c
usize  strlen(const char *s);
int    strcmp(const char *s1, const char *s2);
int    strncmp(const char *s1, const char *s2, usize n);
char  *strchr(const char *s, int c);
char  *strncpy(char *dest, const char *src, usize n);
void  *memcpy(void *dest, const void *src, usize n);
void  *memmove(void *dest, const void *src, usize n);
void  *memset(void *dest, int c, usize n);
int    memcmp(const void *s1, const void *s2, usize n);
void  *memchr(const void *s, int c, usize n);
c_ulong strtoul(const char *nptr, char **endptr, int base);
int   *__errno(void);
```

RTOS platforms (FreeRTOS, NuttX, ThreadX, Zephyr) ship their own libc —
you do not need these stubs.

### Network poll callback (smoltcp only)

If using smoltcp for networking, provide a poll callback that
`zpico-smoltcp` calls to process network events:

```c
void smoltcp_network_poll(void);
```

And a Rust API for the board crate to register the network state:

```rust
pub unsafe fn set_network_state(
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
    device: *mut (),
);

pub unsafe fn clear_network_state();
```

## Step-by-step procedure

1. **Create the platform crate** (`nros-platform-<name>`) -- see
   [Implementing a Platform](./implementing-a-platform.md) for the full guide
2. **Implement and verify the clock** — this is the #1 cause of porting
   failures. Print `clock_ms()` in a loop and verify monotonic advance
3. **Implement remaining primitives** — memory, random, sleep, time,
   threading, sockets. Each module is independent
4. **Wire into `nros-platform`** — add a feature and `ConcretePlatform` alias
5. **Create the board crate** — see [Board Crate Implementation](../board-crate.md)
6. **Add the platform feature** to `nros` with mutual exclusivity checks
7. **Write an example** — see [Creating Examples](../creating-examples.md)
8. **Add test infrastructure** — `just test-<name>` recipe + nextest group

## Platform capability summary

| Capability | Bare-metal | RTOS (FreeRTOS/ThreadX) | POSIX-like (NuttX/Zephyr) |
|------------|-----------|-------------------------|---------------------------|
| Clock | Hardware timer | OS tick API | `clock_gettime` |
| Memory | `embedded-alloc` | RTOS heap | System `malloc` |
| Sleep | Busy-wait + poll | `vTaskDelay` | `nanosleep` |
| Threading | No-op stubs | Real tasks + mutexes | pthreads |
| Sockets | smoltcp shims | lwIP or NetX sockets | BSD sockets |
| Random | Seeded xorshift | RTOS RNG or xorshift | `/dev/urandom` |
| libc | Hand-written stubs | RTOS libc | System libc |
| Network poll | `smoltcp_network_poll` | Stack-specific poll | Not needed |

## Common pitfalls

See [Platform Porting Pitfalls](../../advanced/platform-porting-pitfalls.md) for
detailed failure modes including poll-driven clocks, DMA buffer placement,
QEMU I/O starvation, recursive mutexes, stack sizing, and heap sizing.
