# Custom Platform Implementation Demo

This example demonstrates how to use nros-c with a hand-rolled
platform abstraction layer suitable for bare-metal / RTOS / custom
embedded targets.

## Features Demonstrated

- **Platform Abstraction Layer**: a reference implementation of the canonical
  `<nros/platform.h>` ABI (`src/platform_impl.c`) — the symbols you port to a
  new target
- **Guard Conditions**: Cross-thread/interrupt signaling for shutdown or event notification
- **Static Allocation**: application resources held in one static struct
- **Timer Callbacks**: Periodic publishing using the executor
- **Clean Shutdown**: Signal handler triggering guard condition for graceful termination

## How it is built (important)

The executable `baremetal_demo` runs on the **Rust POSIX platform port** (linked
via `DEPLOY native`), so it builds and runs on a desktop. `src/platform_impl.c`
implements the **same** `nros_platform_*` symbols, so it cannot be linked into
that binary too — two implementations of one ABI is a duplicate-definition
error. Instead CMake compiles it into a stand-alone `baremetal_platform_ref`
library purely as **compile coverage**, keeping the reference in lockstep with
the canonical header.

To make `platform_impl.c` the *actual* platform for a real target: build nros
**without** a Rust platform port (no `platform-posix` / `platform-<rtos>`
feature — the C side provides the symbols) and link this translation unit into
your firmware. See the header comment in `src/platform_impl.c`.

## Building

```bash
# First, build the nros-c library
cargo build --release -p nros-c

# Then build the example
cd examples/native/c/custom-platform
mkdir -p build && cd build
cmake .. -DCMAKE_BUILD_TYPE=Release
make
```

## Running

```bash
# Start zenoh router
zenohd --listen tcp/127.0.0.1:7447 &

# Run the demo
./baremetal_demo
```

## Platform Implementation

`src/platform_impl.c` implements the full canonical ABI declared in
`<nros/platform.h>`. A platform supplies every function declared `extern` there;
the header provides `nros_platform_malloc`/`free` and
`nros_platform_atomic_{store,load}_bool` as `static inline` (do not redefine
them). The surface, by group:

- **Clock** — `clock_ms`, `clock_us` (monotonic)
- **Allocation** — `alloc`, `realloc`, `dealloc`, `heap_used_bytes`, `heap_total_bytes`
- **Sleep / yield** — `sleep_us`, `sleep_ms`, `sleep_s`, `yield_now`
- **Random** — `random_u8/u16/u32/u64`, `random_fill`
- **Wall clock** — `time_now_ms`, `time_since_epoch_secs`, `time_since_epoch_nanos`
- **Tasks** — `task_init/join/detach/cancel/exit/free`
- **Mutex** — `mutex_*` and recursive `mutex_rec_*`
- **Condvar** — `condvar_init/drop/signal/signal_all/signal_from_isr/wait/wait_until`
- **Wake** — `wake_init/drop/wait_ms/signal/signal_from_isr/storage_size/storage_align`
- **Critical section** — `critical_section_acquire/release`
- **Logging** — `log_write`, `log_flush`

The bodies are POSIX-backed (so the reference compiles + runs on a desktop) and
each section notes the bare-metal alternative. For example, the monotonic clock
on a Cortex-M with the DWT cycle counter:

```c
uint64_t nros_platform_clock_us(void) {
    static uint64_t high = 0;
    static uint32_t last = 0;
    uint32_t now = DWT->CYCCNT;
    if (now < last) high += (1ULL << 32);   // 32-bit wrap
    last = now;
    return (high | now) / (SystemCoreClock / 1000000ULL);
}
```

and the critical section via PRIMASK:

```c
uint32_t nros_platform_critical_section_acquire(void) {
    uint32_t primask = __get_PRIMASK();
    __disable_irq();
    return primask;                 // token = prior posture
}
void nros_platform_critical_section_release(uint32_t token) {
    if (!token) __enable_irq();      // only re-enable if we disabled
}
```

## Memory Usage

The demo prints its static footprint at exit (the application resources held in
one `static` struct plus the serialize buffer) — representative of an embedded
build's `.bss` budget.

## Guard Conditions

Guard conditions provide a thread-safe mechanism for signaling events:

```c
// In signal handler (or interrupt handler on embedded):
nros_guard_condition_trigger(&shutdown_guard);

// In executor callback:
void shutdown_callback(void* context) {
    nros_executor_stop(&executor);
}
```

This pattern is useful for:
- Shutdown signals from interrupt handlers
- Waking up the executor from another thread
- Coordinating between tasks in an RTOS
