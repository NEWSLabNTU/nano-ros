# Known Issues

Documented bugs, hardcoded values, and improvement opportunities.
Items here are candidates for future roadmap phases.

## 1. Hardcoded network configuration in board crates and examples

Board crate `Config` preset methods (`default()`, `listener()`, `server()`, etc.) have hardcoded IP addresses, MAC addresses, gateways, and zenoh locators tied to specific test/development networks.

**Affected files** (all `src/config.rs`):
- `packages/boards/nros-mps2-an385/` — 192.0.3.x (TAP), 192.168.100.x (Docker), 172.20.0.2 (Docker zenoh)
- `packages/boards/nros-mps2-an385-freertos/` — 192.0.3.x
- `packages/boards/nros-esp32-qemu/` — 192.0.3.x
- `packages/boards/nros-nuttx-qemu-arm/` — 192.0.3.x
- `packages/boards/nros-threadx-qemu-riscv64/` — 192.0.3.x, MAC 52:54:00:12:34:56
- `packages/boards/nros-threadx-linux/` — 192.0.3.x, hardcoded veth names (`veth-tx0`, `veth-tx1`)
- `packages/boards/nros-stm32f4/` — 192.168.1.x
- `packages/boards/nros-esp32/` — 192.168.1.x (wifi)

**Also hardcoded**:
- STM32F4 HSE oscillator frequency (8 MHz) — varies by board variant
- UART indices (UART0, USART2, USART3) — varies by board
- Baud rates (115200 everywhere)
- Serial zenoh locators (`serial/UART_0#baudrate=115200`)

Builder methods (`.with_ip()`, `.with_zenoh_locator()`, `.with_baudrate()`) exist for runtime override, but the defaults are QEMU/dev-board-specific. Users porting to real hardware must override everything.

**Impact**: Users cannot reuse examples without modifying source code for their network setup.

**Possible fix**: Environment-variable-driven defaults (read at build time via `build.rs`, similar to `NROS_EXECUTOR_MAX_CBS`), or a runtime config file/struct that users populate before calling `init()`.

## 2. Zenoh-pico free list allocator on bare-metal

Bare-metal platform crates use a custom first-fit free-list allocator for zenoh-pico's `z_malloc`/`z_free`:

**Implementations** (all `src/memory.rs`):
- `packages/zpico/zpico-platform-mps2-an385/` — 64 KB heap (128 KB with `link-tls`)
- `packages/zpico/zpico-platform-esp32/` — 32 KB heap
- `packages/zpico/zpico-platform-esp32-qemu/` — 32 KB heap
- `packages/zpico/zpico-platform-stm32f4/` — 64 KB heap

RTOS platforms already use native allocators:
- FreeRTOS → `pvPortMalloc`/`vPortFree` (in zenoh-pico's `src/system/freertos/system.c`)
- ThreadX → `tx_byte_allocate`/`tx_byte_release` (in `zpico-sys/c/platform/threadx/system.c`)
- ESP-IDF → `heap_caps_malloc` (in zenoh-pico's `src/system/espidf/system.c`)
- NuttX → libc `malloc`/`free` (POSIX-compatible)

**Concerns**:
- Fixed heap size — can't grow at runtime
- First-fit fragmentation risk over long-running sessions
- No `realloc` support on FreeRTOS/ThreadX (zenoh-pico's `z_realloc` returns NULL)
- Duplicated allocator code across 4 platform crates
- RTOS allocators would integrate with their memory debugging/stats tools

**Possible fix**: For platforms that run under an RTOS, delegate to the RTOS allocator. For true bare-metal, the free-list is fine but could be deduplicated into a shared crate. The DDS backend (Phase 70/71) uses `#[global_allocator]` which is a cleaner Rust-native approach.

## 3. `nano_ros_generate_interfaces()` requires explicit file listing

The native CMake function requires every `.msg`/`.srv`/`.action` file to be listed explicitly:

```cmake
nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" LANGUAGE CPP SKIP_INSTALL)
```

The **Zephyr wrapper** (`zephyr/cmake/nros_generate_interfaces.cmake`) already supports auto-discovery — omitting files causes it to glob `msg/*.msg`, `srv/*.srv`, `action/*.action` from local directories and fall back to ament index. But the **native CMake function** (`packages/codegen/packages/nros-codegen-c/cmake/NanoRosGenerateInterfaces.cmake`) does not.

Standard ROS 2 `rosidl_generate_interfaces()` also requires explicit listing, but users commonly expect auto-discovery.

**Impact**: Boilerplate in every CMakeLists.txt; easy to forget files.

**Possible fix**: Port the Zephyr auto-discovery logic to the native function. Add a glob path for bundled interfaces too (currently only Zephyr searches them automatically). Keep explicit listing as an option for fine-grained control.

## 4. Non-configurable compile-time constants

Several library constants are hardcoded without environment variable or Kconfig overrides:

| Constant | Value | File |
|----------|-------|------|
| `SERVICE_DEFAULT_TIMEOUT_MS` | 10,000 ms | `packages/zpico/nros-rmw-zenoh/src/shim/service.rs` |
| `MAX_PARAMS_PER_REQUEST` | 64 | `packages/core/nros-node/src/parameter_services.rs` |
| `PARAM_SERVICE_BUFFER_SIZE` | 4,096 bytes | `packages/core/nros-node/src/parameter_services.rs` |
| `DEFAULT_MAX_TIMERS` | 8 | `packages/core/nros-node/src/timer.rs` |
| `KEYEXPR_STRING_SIZE` | 256 | `packages/zpico/nros-rmw-zenoh/src/shim/mod.rs` |
| `LOCATOR_BUFFER_SIZE` | 128 | `packages/zpico/nros-rmw-zenoh/src/shim/mod.rs` |
| `CONFIG_PROPERTY_SIZE` | 256 | `packages/zpico/nros-rmw-zenoh/src/shim/mod.rs` |
| `MAX_SESSION_PROPERTIES` | 8 | `packages/zpico/nros-rmw-zenoh/src/shim/mod.rs` |
| `MANGLED_NAME_SIZE` | 64 | `packages/zpico/nros-rmw-zenoh/src/shim/mod.rs` |
| `QOS_STRING_SIZE` | 32 | `packages/zpico/nros-rmw-zenoh/src/shim/mod.rs` |

These are reasonable defaults but can't be tuned for constrained (smaller) or heavy (larger) workloads.

**Possible fix**: Follow the `NROS_EXECUTOR_MAX_CBS` pattern — read env var in `build.rs`, generate a config.rs constant, export via `links` metadata. Zephyr gets Kconfig integration via the existing bridging mechanism.
