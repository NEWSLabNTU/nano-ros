# Custom Platform Implementation Demo

This example demonstrates how to use nros-c with a hand-rolled
platform abstraction layer suitable for bare-metal / RTOS / custom
embedded targets.

## Features Demonstrated

- **Platform Abstraction Layer**: a reference implementation of the canonical
  `<nros/platform.h>` ABI (`src/native_c_custom_platform/src/platform_impl.c`) — the symbols you port to a
  new target
- **Guard Conditions**: Cross-thread/interrupt signaling for shutdown or event notification
- **Static Allocation**: application resources held in one static struct
- **Timer Callbacks**: Periodic publishing using the executor
- **Clean Shutdown**: Signal handler triggering guard condition for graceful termination

## How it is built (important)

The executable `baremetal_demo` runs on the **Rust POSIX platform port** — the
one `[image.linux] board = "linux"` in the bringup's `system.toml` selects — so it builds and
runs on a desktop. `src/native_c_custom_platform/src/platform_impl.c`
implements the **same** `nros_platform_*` symbols, so it cannot be linked into
that binary too — two implementations of one ABI is a duplicate-definition
error. Instead CMake compiles it into a stand-alone `baremetal_platform_ref`
library purely as **compile coverage**, keeping the reference in lockstep with
the canonical header.

To make `platform_impl.c` the *actual* platform for a real target: build nros
**without** a Rust platform port (no `platform-posix` / `platform-<rtos>`
feature — the C side provides the symbols) and link this translation unit into
your firmware. See the header comment in
`src/native_c_custom_platform/src/platform_impl.c`.

## Building

A single-node workspace: `src/native_c_custom_platform` is the program, with
its own `main()`, and `src/native_c_custom_platform_bringup` holds the
`system.toml` that says what to build. The `linux` image builds that same
`main` (an owned entry, RFC-0071 D19); it keeps its hand-written `main`
because a user-supplied platform is one of the manual-entry demos RFC-0122 D10
keeps.

```bash
cp -r examples/linux/c/custom-platform ~/my-platform && cd ~/my-platform
source /path/to/nano-ros/activate.sh     # puts that checkout's nros on PATH
nros build linux
```

Nothing to build by hand first: `find_package(nano_ros)` builds the `nros-c`
library the example links. The board, the RMW and the domain come from the
bringup's `system.toml` (`[image.linux]` + `[system]`, RFC-0098 D3/D5), not
from a `-D` flag.

## Running

```bash
# Start zenoh router
ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' ros2 run rmw_zenoh_cpp rmw_zenohd &

# Run the demo
./build/posix-zenoh-linux-native_c_custom_platform_bringup/cmake/pkg/native_c_custom_platform/baremetal_demo
```

## Platform Implementation

`src/native_c_custom_platform/src/platform_impl.c` implements the full canonical ABI declared in
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
// Creation is ONE call, on the node, with the callback bound then and there.
// The node must already be bound to an executor (`rclc_executor_node_init`):
// the flag lives in that executor's arena.
rclc_node_create_guard_condition(&node, &shutdown_guard, shutdown_callback, NULL);

// In a signal handler (or an interrupt handler on embedded), from any thread:
rclc_guard_condition_trigger(&shutdown_guard);

// Dispatched on the executor's thread, at its next spin:
void shutdown_callback(void* context) {
    rclc_executor_cancel(&executor);
}
```

This pattern is useful for:
- Shutdown signals from interrupt handlers
- Waking up the executor from another thread
- Coordinating between tasks in an RTOS
