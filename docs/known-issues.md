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

| Env var | Default | Constant | Crate |
|---------|---------|----------|-------|
| `NROS_SERVICE_TIMEOUT_MS` | 10,000 ms | `SERVICE_DEFAULT_TIMEOUT_MS` | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_BUFFER_SIZE` | 4,096 bytes | `PARAM_SERVICE_BUFFER_SIZE` | nros-node |
| `NROS_KEYEXPR_STRING_SIZE` | 256 | `KEYEXPR_STRING_SIZE` | nros-rmw-zenoh |

### Removed (dead code)

| Constant | Reason |
|----------|--------|
| `DEFAULT_MAX_TIMERS` | Was never enforced; timer count is bounded by `MAX_CBS` |

### Internal constants (intentionally not user-configurable)

These have safe defaults and are unlikely to need tuning. Changing them risks
protocol incompatibility or buffer overflows with no user benefit:

| Constant                 | Value | Why internal |
|--------------------------|-------|-------------|
| `MAX_PARAMS_PER_REQUEST` | 64    | Matches ROS 2 rclcpp default |
| `LOCATOR_BUFFER_SIZE`    | 128   | Locator strings are always short |
| `CONFIG_PROPERTY_SIZE`   | 256   | Session properties are simple key=value |
| `MAX_SESSION_PROPERTIES` | 8     | Zenoh session rarely needs >8 properties |
| `MANGLED_NAME_SIZE`      | 64    | Only the type suffix, not full topic name |
| `QOS_STRING_SIZE`        | 32    | QoS strings are fixed format, always short |

## 5. Hardcoded opaque type sizes in nros-c and nros-cpp

The C and C++ APIs use inline opaque storage (`alignas(8) uint8_t storage_[N]`) to hold
Rust types without heap allocation. The storage sizes are hardcoded constants that must
be >= the actual Rust struct size. Compile-time `const _: () = assert!(...)` checks catch
undersized constants, but the values themselves are manually chosen upper bounds — they
must be bumped by hand whenever the underlying Rust types grow.

### nros-c — hardcoded in `packages/core/nros-c/src/constants.rs`

| Constant | Size (u64s) | Bytes | Rust type |
|----------|-------------|-------|-----------|
| `SESSION_OPAQUE_U64S` | 64 | 512 | `RmwSession` |
| `PUBLISHER_OPAQUE_U64S` | 48 | 384 | `RmwPublisher` |
| `SERVICE_CLIENT_OPAQUE_U64S` | 48 | 384 | `RmwServiceClient` |
| `GUARD_HANDLE_OPAQUE_U64S` | 4 | 32 | `GuardConditionHandle` |

Static assertions in `support.rs`, `publisher.rs`, `service.rs`, `guard_condition.rs`.

### nros-cpp — hardcoded in `packages/core/nros-cpp/src/lib.rs` (Rust) and `include/nros/config.hpp` (C++)

| Constant | Default (u64s) | DDS (u64s) | Rust type |
|----------|---------------|------------|-----------|
| `CPP_PUBLISHER_OPAQUE_U64S` | 96 | 256 | `CppPublisher` |
| `CPP_SUBSCRIPTION_OPAQUE_U64S` | 224 | 384 | `CppSubscription` |
| `CPP_SERVICE_SERVER_OPAQUE_U64S` | 224 | 768 | `CppServiceServer` |
| `CPP_SERVICE_CLIENT_OPAQUE_U64S` | 224 | 768 | `CppServiceClient` |
| `CPP_GUARD_HANDLE_OPAQUE_U64S` | 4 | 4 | `GuardConditionHandle` |

Static assertions in `publisher.rs`, `subscription.rs`, `service.rs`, `guard_condition.rs`.

The C++ header `config.hpp` duplicates the non-DDS values as `#define` macros
(e.g., `NROS_CPP_PUBLISHER_STORAGE_SIZE (96 * 8)`). These are **not feature-gated**
for DDS — a DDS build with unchanged `config.hpp` will silently use undersized
C++ storage, causing UB even though the Rust-side assertion catches it.

### What already works

The **executor** storage is computed at build time in both crates' `build.rs` from
nros-node's `DEP_NROS_NODE_*` link metadata and written to a generated header
(`nros_config_generated.h` / `nros_cpp_config_generated.h`). This is the right pattern.

### Problems

1. **Manual maintenance** — adding fields to Rust types requires finding and bumping
   the hardcoded constant, then rebuilding to see if the assertion passes.
2. **DDS size divergence** — DDS types are much larger; `config.hpp` doesn't
   account for this, so C++ users on DDS get a build failure or UB.
3. **Duplicated values** — Rust constants and C/C++ `#define`s must be kept in sync
   manually. The executor already solved this with generated headers.

### Possible fix

Extend the `build.rs` approach used for executor storage to all opaque types:

1. Compute `size_of::<RmwPublisher>()` etc. at build time (these are known after
   dependent crates compile — use `DEP_*` link metadata or measure directly).
2. Write the sizes to the generated config headers alongside executor storage.
3. Remove the hardcoded constants from `constants.rs`, `lib.rs`, and `config.hpp`.
4. The C/C++ headers then always match the actual Rust layout, regardless of
   feature flags (DDS, XRCE, etc.).
