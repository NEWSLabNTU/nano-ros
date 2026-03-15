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

## 3. ~~`nano_ros_generate_interfaces()` requires explicit file listing~~ (Fixed)

Both the native and Zephyr versions of `nano_ros_generate_interfaces()`
now support auto-discovery when no files are specified. The C codegen also
correctly handles intra-package nested type dependencies.

```cmake
# Auto-discover all types + generate builtin_interfaces dependency
nano_ros_generate_interfaces(builtin_interfaces SKIP_INSTALL)
nano_ros_generate_interfaces(std_msgs DEPENDENCIES builtin_interfaces SKIP_INSTALL)

# Explicit listing still works for fine-grained control
nano_ros_generate_interfaces(std_msgs "msg/Int32.msg" SKIP_INSTALL)
```

Cross-package dependencies (e.g., `std_msgs` → `builtin_interfaces`) must be
declared with `DEPENDENCIES` and generated separately. Intra-package dependencies
(e.g., `ByteMultiArray` → `MultiArrayLayout` within `std_msgs`) are resolved
automatically.

## 4. Non-configurable compile-time constants

### Now configurable via env vars

The following constants were hardcoded but are now configurable via environment variables
(set in `.env` or exported before building):

| Env var                          | Default     | Constant                     | Crate          |
|----------------------------------|-------------|------------------------------|----------------|
| `NROS_SERVICE_TIMEOUT_MS`        | 10,000 ms   | `SERVICE_DEFAULT_TIMEOUT_MS` | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_BUFFER_SIZE` | 4,096 bytes | `PARAM_SERVICE_BUFFER_SIZE`  | nros-node      |
| `NROS_KEYEXPR_STRING_SIZE`       | 256         | `KEYEXPR_STRING_SIZE`        | nros-rmw-zenoh |

### Removed (dead code)

| Constant             | Reason                                                  |
|----------------------|---------------------------------------------------------|
| `DEFAULT_MAX_TIMERS` | Was never enforced; timer count is bounded by `MAX_CBS` |

### Internal constants (intentionally not user-configurable)

These have safe defaults and are unlikely to need tuning. Changing them risks
protocol incompatibility or buffer overflows with no user benefit:

| Constant                 | Value | Why internal                               |
|--------------------------|-------|--------------------------------------------|
| `MAX_PARAMS_PER_REQUEST` | 64    | Matches ROS 2 rclcpp default               |
| `LOCATOR_BUFFER_SIZE`    | 128   | Locator strings are always short           |
| `CONFIG_PROPERTY_SIZE`   | 256   | Session properties are simple key=value    |
| `MAX_SESSION_PROPERTIES` | 8     | Zenoh session rarely needs >8 properties   |
| `MANGLED_NAME_SIZE`      | 64    | Only the type suffix, not full topic name  |
| `QOS_STRING_SIZE`        | 32    | QoS strings are fixed format, always short |

## 5. ~~Hardcoded opaque type sizes in nros-c and nros-cpp~~ (Fixed)

Opaque storage sizes for RMW handles are now computed from
`core::mem::size_of` at compile time — they always match the actual Rust
type layout and auto-adjust when types change. No manual maintenance needed.

- **nros-c**: `opaque_sizes.rs` computes `SESSION_OPAQUE_U64S`,
  `PUBLISHER_OPAQUE_U64S`, `SERVICE_CLIENT_OPAQUE_U64S`, and
  `GUARD_HANDLE_OPAQUE_U64S` from `size_of::<RmwSession>()` etc.
- **nros-cpp**: `lib.rs` computes `CPP_PUBLISHER_OPAQUE_U64S` etc. from
  `size_of::<CppPublisher>()` etc.

When no RMW backend is enabled, fallback values are used (sufficient for
any backend).

### Remaining issue

The C++ header `config.hpp` duplicates values as `#define` macros. These
are not auto-generated — C++ users must update them to match the Rust
computed values, or the Rust-side assertion (in build.rs) catches the
mismatch at build time.
