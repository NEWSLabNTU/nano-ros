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
| ThreadX    | `tx_byte_allocate` (Rust, in `nros-platform-threadx`)              | None available                                                | Disabled             |
| NuttX      | libc `malloc` (C, via POSIX `system/unix/system.c`)                | Standard Rust allocator (libc `malloc`)                       | Enabled (`std`)      |
| Zephyr     | `k_malloc` (C, in zenoh-pico `system/zephyr/system.c`)             | Zephyr allocator (when configured)                            | Varies               |

**Concerns**:

1. ~~**FreeRTOS `z_realloc` returns NULL**~~ (Fixed) — implemented as
   alloc-copy-free in `system/freertos/system.c`, matching ThreadX.

2. ~~**ThreadX has no Rust global allocator**~~ (Fixed) — added
   `ThreadXAllocator` in both `nros-c/src/lib.rs` and `nros-cpp/src/lib.rs`,
   wrapping `z_malloc`/`z_free` (which delegate to `tx_byte_allocate`/
   `tx_byte_release`). Gated on `alloc + !std + platform-threadx`.

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

## 7. Unbounded message sequences waste memory or cannot hold large payloads

Generated message bindings use `heapless::Vec<T, N>` for unbounded sequences
(`uint8[] data`, `float32[] ranges`, etc.). The capacity `N` is hardcoded in
the codegen at **64 elements** (`NROS_DEFAULT_SEQUENCE_CAPACITY` in
`packages/codegen/packages/rosidl-codegen/src/types.rs`).

This creates a fundamental mismatch for messages with large variable-length
payloads:

| Message                   | Field              | Typical size          | Generated capacity |
|---------------------------|--------------------|-----------------------|--------------------|
| `sensor_msgs/Image`       | `uint8[] data`     | 921,600 (640×480 RGB) | 64 bytes           |
| `sensor_msgs/PointCloud2` | `uint8[] data`     | 10,000+               | 64 bytes           |
| `sensor_msgs/LaserScan`   | `float32[] ranges` | 360–1080              | 64 floats          |
| `nav_msgs/OccupancyGrid`  | `int8[] data`      | 10,000+               | 64 bytes           |

**Problem**: `heapless::Vec<u8, 65536>` would support 64 KB images, but the
backing `[MaybeUninit<u8>; 65536]` **always occupies 64 KB** on the stack
regardless of actual content. On MCUs with 64–256 KB total RAM, this is
unacceptable.

Bounded sequences (`uint8[<=100] data`) use the specified max and do not
suffer from the default-capacity problem.

**Impact**: Large sensor messages (Image, PointCloud2, LaserScan) are
effectively unusable on embedded targets with the current codegen.
Deserialization fails with `DeserError::CapacityExceeded` when the incoming
data exceeds 64 elements.

**Design direction — borrowed deserialization (zero-copy)**:

Instead of copying sequence data into the message struct, generate a
borrowed message type where unbounded sequences are `&'a [u8]` slices
pointing directly into the CDR receive buffer:

```rust
// Current: copies data into fixed inline buffer (64 bytes max)
struct Image {
    height: u32,
    width: u32,
    encoding: heapless::String<32>,
    data: heapless::Vec<u8, 64>,  // 64 bytes on stack, always
}

// Proposed: borrows data from transport buffer (16 bytes on stack)
struct Image<'a> {
    height: u32,
    width: u32,
    encoding: heapless::String<32>,
    data: &'a [u8],  // pointer + length, points into CDR buffer
}
```

The deserializer reads the CDR sequence length header, then returns a slice
into the receive buffer at the correct offset — no copy, no fixed capacity.
The message struct is small and fixed-size. The payload can be arbitrarily
large, bounded only by the transport buffer size (`NROS_SUBSCRIPTION_BUFFER_SIZE`).

This works for any sequence field, not just the last one — the CDR
deserializer knows each field's offset. The lifetime `'a` ties the message
to the receive buffer scope (valid for the duration of the subscription
callback).

**Implementation approach**:

1. Add a `borrowed` codegen mode alongside the current `owned` mode.
   `owned` generates `heapless::Vec<T, N>` (current behavior, for small
   messages). `borrowed` generates `&'a [T]` for unbounded sequences.
2. The subscription callback receives `Image<'_>` with data borrowing
   the CDR buffer. The message is valid only inside the callback.
3. For non-byte sequences (`float32[] ranges`), alignment must be
   verified — CDR guarantees alignment, but the slice cast from
   `&[u8]` to `&[f32]` needs validation on strictly-aligned platforms.
4. Transport buffer size becomes the effective message size limit,
   configurable per-subscription via `NROS_SUBSCRIPTION_BUFFER_SIZE`.

**Workarounds available today**:

- Define bounded message types for the application's actual payload
  size (e.g., `uint8[<=4096] data` in a custom `.msg` file).
- Use raw CDR APIs (`try_recv_raw`) to access the receive buffer
  directly, bypassing the generated message types entirely.

## 8. Two-copy receive path and static buffer pre-allocation at scale

Every subscription message traverses two copies before reaching user code:

```
Network → SUBSCRIBER_BUFFERS[i].data → SubEntry.buffer (arena) → deserialize → callback
              (zenoh-pico direct write)     (memcpy in try_recv_raw)    (CDR field-by-field)
```

**Copy chain**:

| Copy | From                         | To                           | Location       | Method                               |
|------|------------------------------|------------------------------|----------------|--------------------------------------|
| —    | Network                      | `SUBSCRIBER_BUFFERS[i].data` | Static         | zenoh-pico direct write (no copy)    |
| #1   | `SUBSCRIBER_BUFFERS[i].data` | `SubEntry.buffer`            | Executor arena | `memcpy` in `try_recv_raw()`         |
| #2   | `SubEntry.buffer`            | Message struct               | Stack          | CDR deserialization (field-by-field) |

**Static memory pre-allocation** (default config):

| Buffer                 | Per-unit | Count                         | Default total |
|------------------------|----------|-------------------------------|---------------|
| `SUBSCRIBER_BUFFERS`   | ~1064 B  | `ZPICO_MAX_SUBSCRIBERS` (128) | **133 KB**    |
| Executor arena entries | ~2304 B  | `NROS_EXECUTOR_MAX_CBS` (4)   | **~10 KB**    |

The dominant cost is `SUBSCRIBER_BUFFERS`: 128 slots × buffer size, all
pre-allocated as a static array regardless of how many subscribers exist.

**Scaling problem**: If the buffer size is increased for large messages
(e.g., `ZPICO_SUBSCRIBER_BUFFER_SIZE=65536` for 64 KB compressed images),
the static array becomes 128 × 64 KB = **8 MB** — impossible on any MCU.
Reducing `ZPICO_MAX_SUBSCRIBERS` helps (e.g., 4 slots × 64 KB = 256 KB),
but then the system supports very few concurrent subscribers.

**CPU cost**: The two memcpy operations are negligible for small messages
(1 KB at 100 Hz = 200 KB/s). For large messages (64 KB at 30 Hz = 3.8 MB/s),
the copies are still feasible on Cortex-M4 @ 168 MHz but become a
meaningful fraction of available bandwidth.

**Design direction — single-copy alloc-free receive**:

The goal is to eliminate copy #1 (arena copy) so the user callback
deserializes directly from `SUBSCRIBER_BUFFERS`, reducing to one write
(network → static buffer) plus zero-copy deserialization:

```
Network → SUBSCRIBER_BUFFERS[i].data → borrowed deserialize → callback(&msg)
              (zenoh-pico direct write)    (slices into buffer, no copy)
```

This requires:

1. **Skip the arena buffer**: The executor dispatches directly from
   `SUBSCRIBER_BUFFERS` instead of copying into `SubEntry.buffer`.
   The subscriber buffer is locked (already has an atomic lock flag)
   during callback execution to prevent zenoh-pico from overwriting it.

2. **Borrowed deserialization** (issue 7): The message struct borrows
   `&'a [u8]` slices from the subscriber buffer for variable-length
   fields, avoiding the CDR copy into `heapless::Vec`.

3. **Reduce subscriber slot count**: Instead of 128 pre-allocated
   slots, size `ZPICO_MAX_SUBSCRIBERS` to the actual number of
   subscribers (e.g., 4–8). This is already configurable but defaults
   to 128.

Combined with issue 7's borrowed deserialization, this gives a
zero-copy path from network to user callback for the payload data,
with only fixed-size header fields deserialized onto the stack.

**Existing zero-copy path** (`unstable-zenoh-api`): Skips
`SUBSCRIBER_BUFFERS` entirely — the callback receives `&[u8]` pointing
into zenoh-pico's internal buffer. However, it requires `alloc`
(boxed callback closure) and bypasses the executor's LET semantics,
making it unsuitable for alloc-free bare-metal use.

**Workarounds available today**:

- Set `ZPICO_MAX_SUBSCRIBERS` to the actual subscriber count (e.g., 4)
  to reduce static memory waste.
- Increase `ZPICO_SUBSCRIBER_BUFFER_SIZE` only when large messages are
  needed, accepting the memory tradeoff.
- Use the raw CDR API (`try_recv_raw`) with a caller-provided buffer
  to bypass the static buffer system entirely.

## ~~9. Test groups are fully serialized due to shared resources~~

**Status: Fixed** (Phase 74 — Test Infrastructure: Parallel Isolation)

QEMU-based E2E tests now run in parallel across platforms using:
- **Slirp networking** (74.1) — each QEMU instance has its own isolated NAT stack; no TAP devices, bridges, or `sudo` required
- **Per-platform zenohd ports** (74.2) — each platform uses a fixed port (baremetal=7450, freertos=7451, nuttx=7452, threadx-riscv=7453, esp32=7454, threadx-linux=7455, zephyr=7456)
- **Per-platform nextest groups** (74.5) — `qemu-baremetal`, `qemu-freertos`, `qemu-nuttx`, etc. run concurrently; tests within each group are still serial

C/C++ library contention was resolved by Phase 75 (relocatable CMake install). Phase 140 superseded that: examples now build the staticlibs in-tree per-example via Corrosion (no shared prefix), and each example's CMake build dir is cleaned per invocation.

**Remaining serialization** (by design):
- `c_api` / `cpp_api` — C/C++ native tests share static library build outputs
- `xrce` — single XRCE Agent UDP port
- `large_msg` — high CPU/memory stress tests
- `ros2-interop` — ROS 2 discovery contention

## 10. CMake install prefix is never cleaned between builds

**Status: Resolved by Phase 140 (install-local rip-off).**

Pre-Phase-140, `just install-local` ran `cmake --install` for each
RMW backend into a shared `build/install/` prefix. CMake install is
additive — it wrote new files but never removed files left over from
previous builds, so stale `libnros_cpp_ffi_zenoh.a` /
`libnros_cpp_ffi_xrce.a` archives accumulated. Phase 140 deleted
`install-local` entirely; per-example builds produce their own
Corrosion target tree and never reuse a shared prefix, so the
stale-artefact failure mode is gone.

## 11. C/C++ examples do not use package.xml as single source of truth for message deps

Most C/C++ examples manually call `nros_generate_interfaces()` in CMakeLists.txt
with hardcoded package names and DEPENDENCIES. The intended pattern is for `package.xml`
to be the single source of truth, with `nros_find_interfaces()` resolving
deps via AMENT index.

**Current state**: FreeRTOS C++ and NuttX C++ examples use `package.xml` +
`nros_find_interfaces()`. All other CMake examples (native C/C++, XRCE,
FreeRTOS C) still use manual `nros_generate_interfaces()` calls.

**To migrate**: Add `package.xml` to each example declaring `<depend>` on message
packages, replace manual codegen calls with `nros_find_interfaces()`.

## ~~12. Stale standalone lockfiles trip the codegen ABI guard (218.J debt)~~ (Fixed)

**Status: Fixed** — the `abi_guard` now resolves the **monorepo root**
`Cargo.lock` for any consumer inside the nano-ros tree (detected via the
`packages/core/nros-core/Cargo.toml` marker) instead of a standalone
crate's nearest — possibly stale — lock. In-tree examples link the
in-tree `nros-core` via `[patch.crates-io]`, so the root lock (`0.5.0`) is
the authoritative ABI; external consumers keep the nearest-lock rule.
`just generate-bindings` and the broad-build preflight now pass without
`NROS_SKIP_VERSION_CHECK`. (`packages/cli/nros-cli-core/src/abi_guard.rs`,
`find_monorepo_root` + `monorepo_root_lock`.)

The ~56 standalone lockfiles are still pinned at `0.1.0`, but that no
longer blocks codegen or broad builds; regenerating them is now optional
cosmetic cleanup (the `stm32f4-porting` missing-`[workspace]` snag and the
`tests/simple-workspace` patch config remain if that cleanup is pursued).

---

Original report:

Surfaced by the Phase 226.F broad-build validation. `nros generate-rust`
aborted via the `nros-cli-core` `abi_guard` with
`ABI version mismatch: CLI nros-core 0.5.0 vs workspace nros-core 0.1.0`,
which fails the `generate-bindings` preflight of `build-all-jobserver.sh` /
`just build-test-fixtures` — so no fixture stamp is written and
`just test-all` mass-fails on `_require-fixtures`.

**Root cause**: the Phase 218.J `0.1.0 → 0.5.0` bundle-version bump never
propagated to the standalone example/testing lockfiles — ~56 `Cargo.lock`
+ 7 `Cargo.toml` still pin nano-ros crates at `0.1.0`. The guard reads the
lock in the dir `generate-rust` runs in; a stale own-lock trips it.

**This is a FALSE POSITIVE for actual builds**: the real `nros-core` source
is `0.5.0` everywhere (root `Cargo.lock` + `cargo tree`); standalone locks
are not used for the actual workspace compilation.

**Workaround**: `NROS_SKIP_VERSION_CHECK=1` (the documented `abi_guard`
opt-out) for broad-build / generate-bindings runs.

**Why a clean regen is non-trivial**: standalone locks reference nano-ros as
*patched registry* deps, so a repo-wide `ws sync` + `cargo update -p` sweep
leaves most locks incomplete (61/71 in one run) and produces 5500+ lines of
registry-dep churn. `packages/reference/stm32f4-porting/{polling,rtic}` also
lack an empty `[workspace]` table (they fold into the root workspace), and
`tests/simple-workspace` needs its colcon/standalone patch config
re-established first. Also note: any broad build / `just generate-bindings`
regenerates these stale committed locks + `generated/*.rs`, dirtying ~60+
tracked files per run.

**To fix**: pick a canonical strategy — regenerate + commit all standalone
locks at `0.5.0`, OR drop committed locks for copy-out examples that don't
need a pinned lock, OR retarget the `abi_guard` to read the root lock for
in-tree dirs. Add the missing `[workspace]` tables. (Phase 226.F context.)

## ~~13. stm32f4 `talker-embassy` fixture does not link~~ (Fixed — excluded from build)

**Status: Fixed (build no longer breaks)** — added a manifest `skip_build`
flag (honored in `fixtures-manifest.py::matches_filters`, so it is excluded
from both the build list and the stale probe; surfaced in
`fixture-inventory.py` as a `skip-build` note) and marked `talker-embassy`.
This restores the pre-226 deliberate omission: the stm32f4 fixture build no
longer attempts the broken example. The example itself is still incomplete
(does not link standalone) — fix its board libc/platform glue + memory
layout to drop the `skip_build` flag later. (`examples/fixtures.toml`
talker-embassy row.)

---

Original report:

Surfaced by Phase 226.F. `build-test-fixtures` failed at the stm32f4 leaf:
`stm32f4-rs-embassy-example` — undefined symbols (`__assert_func`,
`strncmp`, `nros_platform_alloc`, …) on a standalone `cargo build`, and
duplicate `platform_aliases` symbols (`z_random_fill`, `z_clock_now`, …)
in the shared fixture target dir.

`talker-embassy` is an incomplete example (missing board libc/platform glue
+ memory layout). The pre-226 hard-coded stm32f4 recipe list **deliberately
omitted** it; the Phase 226 manifest migration (`fixtures-build.sh stm32f4
rust`) builds every manifest row, so it now includes the broken example.
There is no manifest `skip_build` field (only `skip_probe`).

**To fix**: either fix the example's link (board glue + `memory.x`), or add a
manifest `skip_build`/exclude flag and mark it, restoring the pre-226
omission. (Separate from the pre-existing RTIC `_defmt_timestamp` link gap.)

## ~~14. `examples/templates/multi-node-workspace` missing generated dir in broad build~~ (Fixed)

**Status: Fixed** — the broad build's standalone-manifest prefetch
(`nros_cargo_fetch_standalone_manifests()` in `scripts/build/cargo.sh`,
called from `scripts/build-all-jobserver.sh`) ripgreps every
`examples/**/Cargo.toml` with a sibling `Cargo.lock` and runs `cargo
fetch`. The `multi-node-workspace` template path-deps gitignored
`generated/<msg-crate>` via its auto-managed `[patch.crates-io]` block
(only `nros ws sync` materialises it), so `cargo fetch` hard-failed on the
missing manifest. Templates are copy-out recipes — built by no fixture row
or broad-build recipe — so they should not be cache-warmed. Added
`-g '!examples/templates/**'` to the prefetch glob (mirrors the existing
`examples/zephyr/**` exclusion).

---

Original report:

Surfaced by Phase 226.F. `build-all-jobserver.sh` failed: `failed to load
source for dependency builtin_interfaces` —
`examples/templates/multi-node-workspace/generated/builtin_interfaces/Cargo.toml`
no such file. The template path-deps generated message crates under
`generated/` (gitignored) but the broad build runs no codegen / `nros ws
sync` pass for `examples/templates/*` before resolving it.

**To fix**: add a codegen/`ws sync` preflight for `examples/templates/*` in
the broad build, or exclude templates from the broad cargo resolve.

## ~~15. threadx-linux C++ `nros_cpp_ffi.h` regeneration race~~ (Fixed)

**Status: Fixed** — the cbindgen headers (`nros_cpp_ffi.h` / `nros-c`'s
`nros_generated.h`) are committed to the source tree and regenerated **in
place** by every parallel `build.rs`. On a cold build, N independent
Corrosion/Cargo trees (the threadx-linux C++ fixtures run unserialized,
unlike nuttx which forces `NROS_CARGO_FRONTENDS=1`) raced on the
write/compare/rename sequence with no cross-process mutual exclusion, so a
concurrent C++ compile `#include`-ing the header could observe an
inconsistent state → transient "multiple definition / conflicting
declaration of `nros_cpp_qos_t`". The pre-existing atomic tmp+rename
(`9aaa4e435`) only guards a *single* writer.

Fixed in `packages/core/nros-build-helpers/src/shared.rs`: header
regeneration now acquires a cross-process advisory `flock` keyed on the
absolute output path (temp-dir lockfile — no source-tree noise; non-unix
falls back to atomic-rename-only) around the atomic write, making the
"generate → atomically replace" critical section mutually exclusive across
all concurrent regenerators. Covered by
`header_lock_serializes_concurrent_holders` (8×200 concurrent acquisitions,
zero overlap). Also protects `nros-c`'s `nros_generated.h` (same helper).

---

Original report:

Surfaced by Phase 226.E/226.F. Intermittent `nros_cpp_ffi.h` "multiple
definition / conflicting declaration of `nros_cpp_qos_t`" during a cold
threadx-linux C++ fixture build. The on-disk header is clean (1 definition);
the duplication is transient when a cold workspace Corrosion target
regenerates the header mid-build.

## ~~16. threadx-riscv64 `build-fixture-extras` exits 127 on the maintainer host~~ (Fixed)

**Status: Fixed** — the rc=127 came from the ThreadX-RV64 **Cyclone**
fixtures. They self-provision CycloneDDS from source with `BUILD_IDLC=OFF`
(`cmake/platform/nano-ros-threadx.cmake`), so they need a host `idlc`
(Cyclone DDS IDL compiler) the recipe never provisions — it must come from
PATH (a ROS 2 / CycloneDDS install) or the project's own
`build/cyclonedds/bin/idlc` (`just cyclonedds setup`). A clean
`source ./activate.sh` shell has none on PATH. A fresh build dir errors
loudly at configure (`NrosRmwCycloneddsTypeSupport.cmake`, rc 1); an
incremental build dir with a stale cached idlc path invokes the missing
binary as a build-time custom command → opaque **rc 127**.

Fix: `just/threadx-riscv64.just::build-fixture-extras` now guards the
Cyclone block — folds `build/cyclonedds/bin/idlc` onto PATH when present,
and if no `idlc` is resolvable skips the Cyclone fixtures with an
actionable hint (`just cyclonedds setup` / put idlc on PATH /
`NROS_THREADX_RV64_CYCLONEDDS_FIXTURES=0`) instead of dying with 127.
`just threadx_riscv64 doctor` reports `idlc` readiness (advisory).
Behavior is unchanged when `idlc` is already on PATH.

## 17. Zephyr native_sim ↔ zenoh E2E does not connect on some hosts (NSOS offload)

Surfaced by Phase 225.P (Zephyr workspace Entry). On the maintainer host,
every zephyr-zenoh native_sim E2E fails: the zephyr `zephyr.exe` reports
`Transport(ConnectionFailed)` reaching the host `zenohd`, and the listener
times out with zero messages. This affects the new
`test_zephyr_workspace_entry_native_sim_e2e` **and** the pre-existing
single-node reference `test_zephyr_to_native_e2e` identically.

**Root cause (environmental, not a nano-ros defect)**: the native_sim NSOS
(`CONFIG_NET_SOCKETS_OFFLOAD` + `CONFIG_NET_NATIVE_OFFLOADED_SOCKETS`,
both confirmed `=y` in the built `.config`) host-socket offload is
non-functional in this environment. Evidence: `zenohd` v1.7.2 listens on
`tcp/127.0.0.1:7456` and the host shell connects fine, but when the
native_sim binary runs, (a) `zenohd` logs **no** incoming TCP, (b)
`ss -tnp` shows **no** connection to 7456 during the connect window, and
(c) `strace -f -e connect` on the binary shows **no** `connect()` syscall
to 7456 at all. So NSOS fails the connect *inside* the offload layer
before issuing any host syscall — a Zephyr/native_simulator NSOS-layer
problem (kernel / libc / host-trampoline), independent of nano-ros.

**Impact**: no zephyr-zenoh E2E can pass in this environment. The Phase
225.P workspace Entry itself is correct — it builds via `west build`,
boots, brings up the network, registers its launch node set, and attempts
the session exactly like the proven reference; only the host's NSOS
connectivity blocks delivery.

**To fix / workaround**: run the zephyr E2E in a capable environment (CI,
where the reference test passes), or repair the native_sim NSOS
host-socket offload on this host. Build-only verification (`just zephyr
build-fixtures` with `NROS_ZEPHYR_FIXTURE_FILTER=workspace-entry`) is
green and is the local gate until NSOS connectivity is restored.

## 18. NuttX workspace Entry as a standalone cargo-lane fixture (link path identified)

Surfaced by Phase 225.O (NuttX workspace Entry attempt). A
`qemu_nuttx_entry` built through the workspace cargo lane (`cargo build -p
qemu_nuttx_entry --target armv7a-nuttx-eabihf`, build-std) **compiles but
fails to link**: build-std `std` references NuttX libc/pthread symbols
(`pthread_cond_*`, `pthread_key_*`, `clock_gettime`, `getcwd`, `__errno`,
`strerror_r`, `exit`, …) that the standalone `arm-none-eabi-gcc` link does
not resolve.

**`std` is NOT the cause** (verified 2026-06-09). `armv7a-nuttx-eabihf` is
a std-capable Rust target — the NuttX std port maps std's unix-pal calls
onto NuttX libc — and **every one of those symbols is defined (`T`) in the
prebuilt `third-party/nuttx/nuttx/nuttx-export-0.0.0/libs/libc.a`**
(`arm-none-eabi-nm` confirms `pthread_cond_broadcast`, `clock_gettime`,
`getcwd`, `__errno`, `strerror_r`, `exit`, `pthread_cond_wait`, …). The
standalone link failed only because it pulled arm-none-eabi **newlib**, not
the NuttX export libs. So going no_std would not be required — std resolves
fine once the export libs are linked.

**Real unblock — the freertos pattern.** The NuttX export
(`nuttx-export-0.0.0/`) is a complete app-link kit: `scripts/dramboot.ld`
(linker script), `arch/.../crt0.o` (startup), and prebuilt kernel libs in
`libs/` (`libsched libdrivers libboards libc libmm libarch libxx libapps
libnet libcrypto libfs libbinfmt libopenamp libboard`, + toolchain
`libgcc`/`libm`). Mirroring `nros-board-mps2-an385-freertos/build.rs`, a
build.rs on the NuttX board/entry can `cargo:rustc-link-search` the export
`libs/` and `cargo:rustc-link-lib` the kernel libs + stage `dramboot.ld`,
linking a standalone bootable NuttX image via cargo — the same contract as
native/freertos/threadx/esp32. The kernel libs are prebuilt, so this is a
**link-orchestration** task, not a compile-the-kernel task.

**Remaining sub-problem — the init entrypoint.** The export was built
`NUTTX_BUILD="flat"` with `CONFIG_INIT_ENTRYPOINT="nsh_main"`: the kernel's
init task calls `nsh_main`, not the `nros::main!`-emitted `main`. So the
Entry must either export a `nsh_main` symbol that routes to the nros
runtime (linked before `libapps` so it wins), or the export must be
rebuilt with `CONFIG_INIT_ENTRYPOINT` pointing at the nros entry (the
kernel-reconfig path). This entry-point wiring — not std, not the link
itself — is the real work left.

**Fixed in passing**: `nros-board-nuttx` had `#![cfg_attr(not(feature =
"reference-qemu-arm"), no_std)]` while its `run_entry`/`run_generic`
`std::`-using bodies are gated on `cfg(any(feature, target_os = "nuttx"))`
— so a NuttX-target build without that feature compiled the crate as
no_std with active `std::` bodies (24 errors). Corrected to `not(any(feature
= "reference-qemu-arm", target_os = "nuttx"))`; the crate now compiles for
NuttX targets.

**To fix**: (1) build.rs linking the prebuilt `nuttx-export` libs +
`dramboot.ld` + `crt0` (mechanical, freertos-pattern); (2) wire the
`nsh_main` init entrypoint to the nros runtime (or rebuild the export with
a nros entrypoint). The `qemu_nuttx_entry` crate/row was not landed pending
(2).

