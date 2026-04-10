# Platform Model

nano-ros uses three orthogonal compile-time feature axes to configure the library for a specific target. Each axis is mutually exclusive -- enabling two features from the same axis produces a `compile_error!()`. Zero features on an axis is valid (reduced functionality).

## The Three Axes

### RMW Backend (pick one)

The RMW backend determines which middleware transport the library uses to send and receive messages.

| Feature      | Backend                  | Description                                    |
|--------------|--------------------------|------------------------------------------------|
| `rmw-zenoh`  | zenoh-pico               | Peer-to-peer via Zenoh protocol                |
| `rmw-xrce`   | Micro-XRCE-DDS-Client   | Agent-based via DDS-XRCE protocol              |
| `rmw-cffi`   | C vtable adapter         | Third-party transport via C function pointers  |

See [RMW Backends](./rmw-backends.md) for a detailed comparison of Zenoh vs XRCE-DDS.

### Platform (pick one)

The platform feature selects which OS primitives (threading, mutexes, clock, sleep, network) are linked. Each platform crate provides the symbols that the RMW backend's C library requires at link time.

| Feature              | Target                        | Threading         | Network          |
|----------------------|-------------------------------|-------------------|------------------|
| `platform-posix`     | Linux, macOS                  | pthreads          | BSD sockets      |
| `platform-zephyr`    | Zephyr RTOS                   | k_thread_create   | Zephyr sockets   |
| `platform-bare-metal`| Cortex-M, RISC-V, ESP32       | Single-threaded   | smoltcp          |
| `platform-freertos`  | FreeRTOS                      | xTaskCreate       | lwIP             |
| `platform-nuttx`     | NuttX RTOS                    | pthreads          | BSD sockets      |
| `platform-threadx`   | Azure RTOS / ThreadX          | tx_thread_create  | NetX Duo         |

### ROS Edition (pick one)

The ROS edition controls wire-format compatibility with specific ROS 2 releases.

| Feature       | Description                                         |
|---------------|-----------------------------------------------------|
| `ros-humble`  | Humble Hawksbill wire format (no type hash)         |
| `ros-iron`    | Iron Irwini wire format (type hash in key expression)|

## Cross-Cutting Features

These features are orthogonal to the three axes above and can be combined freely.

| Feature                  | Description                                                                     |
|--------------------------|---------------------------------------------------------------------------------|
| `std`                    | Enables `std`-dependent APIs: `spin_blocking()`, `spin_period()`, system clock  |
| `alloc`                  | Enables heap-dependent APIs: boxed callbacks, `param-services`                  |
| `safety-e2e`             | CRC-32 integrity + sequence tracking (AUTOSAR E2E / EN 50159)                   |
| `param-services`         | ROS 2 parameter service handlers (`~/get_parameters`, etc.). Implies `alloc`.   |
| `ffi-sync`               | Wraps transport FFI calls in `critical_section::with()` for RTOS reentrancy     |
| `sync-spin`              | Use spin-lock mutex (default)                                                   |
| `sync-critical-section`  | Use `critical-section` mutex (for RTIC, Embassy)                                |
| `unstable-zenoh-api`     | Zero-copy receive path (Zenoh backend only)                                     |

## Mutual Exclusivity Enforcement

The `nros` facade crate enforces mutual exclusivity at compile time using `compile_error!()`. For example:

```rust,ignore
#[cfg(all(feature = "rmw-zenoh", feature = "rmw-xrce"))]
compile_error!("Only one RMW backend can be enabled: rmw-zenoh or rmw-xrce");

#[cfg(all(feature = "platform-posix", feature = "platform-zephyr"))]
compile_error!("Only one platform can be enabled");
```

This catches misconfiguration immediately at build time rather than producing subtle runtime failures.

## Example Configurations

**QEMU bare-metal Cortex-M3 with Zenoh:**
```toml
[dependencies]
nros = { default-features = false, features = [
    "rmw-zenoh",
    "platform-bare-metal",
    "ros-humble",
] }
```

**Zephyr with XRCE-DDS and safety features:**
```toml
[dependencies]
nros = { default-features = false, features = [
    "rmw-xrce",
    "platform-zephyr",
    "ros-humble",
    "alloc",
    "safety-e2e",
    "ffi-sync",
] }
```

**Desktop Linux for development and testing:**
```toml
[dependencies]
nros = { features = [
    "rmw-zenoh",
    "platform-posix",
    "ros-humble",
    "std",
    "param-services",
] }
```

**FreeRTOS with lwIP networking:**
```toml
[dependencies]
nros = { default-features = false, features = [
    "rmw-zenoh",
    "platform-freertos",
    "ros-humble",
] }
```

**ThreadX with NetX Duo (RISC-V):**
```toml
[dependencies]
nros = { default-features = false, features = [
    "rmw-zenoh",
    "platform-threadx",
    "ros-humble",
] }
```

**NuttX with XRCE-DDS over serial:**
```toml
[dependencies]
nros = { default-features = false, features = [
    "rmw-xrce",
    "platform-nuttx",
    "ros-humble",
] }
```

## Feature Propagation

Features propagate through the crate dependency graph using Cargo's `?` syntax for optional dependencies. When you set `rmw-zenoh` on the `nros` facade, it activates:

- `nros-node/rmw-zenoh` -- selects `ZenohSession` as the concrete session type
- `nros-rmw-zenoh` -- the Zenoh RMW implementation crate
- `zpico-sys` -- zenoh-pico C bindings

Platform features propagate similarly, activating the appropriate `nros-platform-*` crate that provides OS-level primitives (clock, memory, sleep, random, threading). The RMW transport libraries access these primitives through thin shim layers -- `zpico-platform-shim` (inside `zpico-sys`) and `xrce-platform-shim` (inside `xrce-sys`) -- which forward `z_*` and `uxr_*` FFI symbols to the unified `ConcretePlatform` type alias from `nros-platform`.

The default feature set is `std` only. No RMW backend or platform is selected by default -- users must explicitly choose their configuration.
