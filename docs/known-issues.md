# Known Issues

Documented bugs, hardcoded values, and improvement opportunities.
Items here are candidates for future roadmap phases.

## ~~1. Hardcoded network configuration in board crates and examples~~ (Fixed)

Resolved by Phase 72: all examples now use `Config::from_toml(include_str!("../config.toml"))`
with per-example configuration files. Users change `config.toml` and rebuild —
no source code edits needed.

Board crate `Config::default()` / `Config::listener()` presets remain for
backwards compatibility but are no longer used by examples.

## ~~2. Zenoh-pico free list allocator on bare-metal~~ (Fixed)

All four bare-metal platform crates now share a single free-list allocator
via the `zpico-alloc` crate (`packages/zpico/zpico-alloc/`). This replaced
the broken bump allocators on ESP32/ESP32-QEMU/STM32F4 (which had no-op
`z_free` and data-losing `z_realloc`) with the proven MPS2-AN385 first-fit
free-list with address-ordered coalescing.

Each platform's `memory.rs` is now a thin wrapper that instantiates
`FreeListHeap<N>` with its heap size (32-128 KB).

**Remaining considerations** (not bugs):
- Fixed heap size — can't grow at runtime (inherent to bare-metal)
- First-fit fragmentation risk over very long sessions (hours+)
- `zpico-alloc` has an optional `stats` feature for heap usage tracking

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

## 6. Two separate heap allocators on RTOS platforms

On RTOS platforms (FreeRTOS, ThreadX), there are **two independent heap
allocators** that cannot share memory or statistics:

| Allocator                      | Who calls it                                                              | Backed by                                                                      |
|--------------------------------|---------------------------------------------------------------------------|--------------------------------------------------------------------------------|
| zenoh-pico `z_malloc`/`z_free` | zenoh-pico C code (session state, buffers, hashmap buckets, vec growth)   | RTOS allocator (e.g. `pvPortMalloc`, `tx_byte_allocate`)                       |
| Rust `#[global_allocator]`     | nros Rust crates when `alloc` feature is enabled (`Box`, `Vec`, `String`) | RTOS allocator on FreeRTOS (via `FreeRtosAllocator`); not available on ThreadX |

**Current state by platform**:

| Platform   | z_malloc backend                                                   | Rust global_allocator                                         | nros alloc feature   |
|------------|--------------------------------------------------------------------|---------------------------------------------------------------|----------------------|
| Bare-metal | `zpico-alloc` (static free-list, 32–128 KB)                        | None                                                          | Disabled             |
| FreeRTOS   | `pvPortMalloc` (C, in zenoh-pico `system/freertos/system.c`)       | `FreeRtosAllocator` → `pvPortMalloc` (in `nros-c/src/lib.rs`) | Disabled in examples |
| ThreadX    | `tx_byte_allocate` (C, in `zpico-sys/c/platform/threadx/system.c`) | None available                                                | Disabled             |
| NuttX      | libc `malloc` (C, via POSIX `system/unix/system.c`)                | Standard Rust allocator (libc `malloc`)                       | Enabled (`std`)      |
| Zephyr     | `k_malloc` (C, in zenoh-pico `system/zephyr/system.c`)             | Zephyr allocator (when configured)                            | Varies               |

**Concerns**:

1. **FreeRTOS `z_realloc` returns NULL** — zenoh-pico's FreeRTOS `system.c`
   does not implement `z_realloc` (returns NULL). If zenoh-pico ever calls
   `z_realloc` on a FreeRTOS target, the allocation silently fails.
   Current usage does not hit this path, but it's fragile.

2. **ThreadX has no Rust global allocator** — if a future nros feature
   requires `alloc` on ThreadX, there's no allocator bridge. FreeRTOS has
   `FreeRtosAllocator` in `nros-c/src/lib.rs`; ThreadX does not.

3. **Heap budgeting is split** — on FreeRTOS, both zenoh-pico (via
   `pvPortMalloc`) and Rust (via `FreeRtosAllocator` → `pvPortMalloc`)
   draw from the same FreeRTOS heap, but there's no visibility into how
   much each consumer uses. On bare-metal, zenoh-pico uses its own
   `zpico-alloc` heap while nros Rust code uses no heap at all.

4. **Bare-metal could unify** — the `zpico-alloc` free-list heap could
   also serve as a Rust `#[global_allocator]` (implement `GlobalAlloc`
   for `FreeListHeap`), giving bare-metal targets a single heap for
   both C and Rust allocations. This is what the DDS backend already
   does (Phase 70).

**Possible improvements**:

- Implement `z_realloc` for FreeRTOS (alloc + memcpy + free, same as
  the ThreadX implementation already does).
- Add a `ThreadXAllocator` implementing `GlobalAlloc` via
  `tx_byte_allocate`/`tx_byte_release` for future `alloc` support.
- Implement `GlobalAlloc` on `FreeListHeap` so bare-metal platforms
  can optionally use `zpico-alloc` as the Rust global allocator too,
  creating a single unified heap.
- Add heap usage tracking (`stats` feature on `zpico-alloc`) to RTOS
  platforms as well, so developers can monitor total heap pressure.
