# Bare-Metal Platform Demo

This example demonstrates how to use nros-c with bare-metal/embedded platform patterns.

## Features Demonstrated

- **Platform Abstraction Layer**: How to implement platform-specific time and atomic operations
- **Guard Conditions**: Cross-thread/interrupt signaling for shutdown or event notification
- **Static Allocation**: All memory allocated at compile time (no malloc/free)
- **Timer Callbacks**: Periodic publishing using the executor
- **Clean Shutdown**: Signal handler triggering guard condition for graceful termination

## Building

```bash
# First, build the nros-c library
cargo build --release -p nros-c

# Then build the example
cd examples/native-c-baremetal-demo
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

The `platform_impl.c` file shows example implementations for the required platform functions:

- `nros_platform_time_ns()` - Get monotonic time in nanoseconds
- `nros_platform_sleep_ns()` - Sleep for specified nanoseconds
- `nros_platform_atomic_store_bool()` - Atomic boolean store
- `nros_platform_atomic_load_bool()` - Atomic boolean load

### Real Embedded Examples

For STM32 with HAL:
```c
uint64_t nros_platform_time_ns(void) {
    return (uint64_t)HAL_GetTick() * 1000000ULL;
}
```

For Cortex-M with DWT cycle counter:
```c
uint64_t nros_platform_time_ns(void) {
    static uint64_t high_bits = 0;
    static uint32_t last_count = 0;
    uint32_t count = DWT->CYCCNT;
    if (count < last_count) {
        high_bits += (1ULL << 32);
    }
    last_count = count;
    return (high_bits | count) * (1000000000ULL / SystemCoreClock);
}
```

## Memory Usage

The demo prints memory usage at exit:
```
Static app struct: 1592 bytes
Serialize buffer:  64 bytes
Total static:      1656 bytes
```

This shows the total static memory footprint, suitable for systems without dynamic allocation.

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
