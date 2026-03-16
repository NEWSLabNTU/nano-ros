# Known Issues

Documented bugs, hardcoded values, and improvement opportunities.
Items here are candidates for future roadmap phases.

## ~~1. Hardcoded network configuration in board crates and examples~~ (Fixed)

Resolved by Phase 72: all examples now use `Config::from_toml(include_str!("../config.toml"))`
with per-example configuration files. Users change `config.toml` and rebuild —
no source code edits needed.

Board crate `Config::default()` / `Config::listener()` presets remain for
backwards compatibility but are no longer used by examples.

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

## ~~3. Non-configurable compile-time constants~~ (Fixed)

Three user-facing constants are now configurable via environment variables:

| Env var                          | Default     | Constant                     | Crate          |
|----------------------------------|-------------|------------------------------|----------------|
| `NROS_SERVICE_TIMEOUT_MS`        | 10,000 ms   | `SERVICE_DEFAULT_TIMEOUT_MS` | nros-rmw-zenoh |
| `NROS_PARAM_SERVICE_BUFFER_SIZE` | 4,096 bytes | `PARAM_SERVICE_BUFFER_SIZE`  | nros-node      |
| `NROS_KEYEXPR_STRING_SIZE`       | 256         | `KEYEXPR_STRING_SIZE`        | nros-rmw-zenoh |

`DEFAULT_MAX_TIMERS` was removed (dead code — timer count bounded by `MAX_CBS`).

Six internal constants remain intentionally non-configurable (safe defaults,
protocol-tied values).

## ~~4. `nano_ros_generate_interfaces()` requires explicit file listing~~ (Fixed)

Both the native and Zephyr CMake functions now support auto-discovery when
no files are specified. The C codegen also handles intra-package nested type
dependencies correctly (fully qualified type names, per-type `#include`
directives).

Cross-package dependencies must be declared with `DEPENDENCIES` and generated
separately.

## ~~5. Hardcoded opaque type sizes in nros-c and nros-cpp~~ (Fixed)

Opaque storage sizes for RMW handles are now computed from
`core::mem::size_of` at compile time — they always match the actual Rust
type layout and auto-adjust when types change. No manual maintenance needed.

- **nros-c**: `opaque_sizes.rs` computes sizes from `size_of::<RmwSession>()` etc.
- **nros-cpp**: `lib.rs` computes sizes from `size_of::<CppPublisher>()` etc.
