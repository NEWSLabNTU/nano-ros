# Platform Model

nano-ros configures the library for a specific target through
compile-time choices on three axes. `nros` itself carries one
generic `rmw-cffi` runtime registry; the consuming `Cargo.toml`
adds the chosen backend shim crate directly (one per RMW).

## The Three Axes

### RMW Backend

The RMW backend determines which middleware transport carries
messages. `nros` exposes a single `rmw-cffi` feature — the generic
C-vtable runtime registry. The consuming crate adds the concrete
backend crate as a direct dependency; its `#[ctor]` registers a
vtable with that registry before `main`.

| Backend crate | Transport | Description |
|---|---|---|
| `nros-rmw-zenoh` | zenoh-pico | Peer-to-peer via Zenoh protocol; default, ROS-2-interop. |
| `nros-rmw-xrce-cffi` | Micro-XRCE-DDS-Client | Agent-based via DDS-XRCE protocol. |
| `nros-rmw-cyclonedds` | Cyclone DDS | C++ shim; standalone CMake project. |

Unlike the platform axis, RMW is **not** mutually exclusive — Phase
104 lets one binary register multiple backends (bridge nodes). See
[Choosing an RMW Backend](../user-guide/rmw-backends.md).

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

The platform and ROS-edition axes are mutually exclusive — `nros`
enforces this at compile time with `compile_error!()`:

```rust,ignore
#[cfg(all(feature = "platform-posix", feature = "platform-zephyr"))]
compile_error!("Platform features are mutually exclusive — select at most one.");

#[cfg(all(feature = "ros-humble", feature = "ros-iron"))]
compile_error!("`ros-humble` and `ros-iron` are mutually exclusive.");
```

The RMW axis is not enforced this way — it is the consuming crate's
choice of which `nros-rmw-*` dependency to add (one, or several for
bridge nodes).

## Boards vs Platforms

Above the platform axis, **board crates** select a concrete chip + RTOS
combination on top of one of the platform features. A board owns the
linker layout, the vendor HAL wiring, the boot path, and any chip-specific
peripherals (Ethernet PHY, IVC mailboxes, RTC). It does **not** carry its
own platform axis — it pulls in one of the rows above.

| Board crate                  | Underlying platform | CPU         | Notes                                       |
|------------------------------|---------------------|-------------|---------------------------------------------|
| `nros-board-mps2-an385`      | `bare-metal`        | Cortex-M3   | QEMU MPS2-AN385 + LAN9118 + smoltcp         |
| `nros-board-stm32f4`         | `bare-metal`        | Cortex-M4   | STM32F4-Discovery                           |
| `nros-board-esp32-qemu`      | `bare-metal`        | RISC-V (ESP32-C3) | QEMU ESP32 + OpenETH + smoltcp        |
| `nros-board-orin-spe`        | `freertos`          | Cortex-R5F  | NVIDIA Jetson AGX Orin SPE + FreeRTOS-FSP   |

`nros-board-orin-spe` is the canonical example of the board-over-RTOS
pattern: a Cortex-R5F SPE running NVIDIA's FreeRTOS V10.4.3 FSP, with
critical-section provided by the canonical
`nros_platform_critical_section_{acquire,release}` C symbols (Cortex-R
CPSR I-bit body inside the port) — not by a per-CPU Rust feature flag.
See `docs/roadmap/phase-100-orin-spe-infra.md` for the wiring.

## Example Configurations

Each config lists `nros` (with `rmw-cffi` + one `platform-*`) plus
the chosen backend crate. POSIX additionally needs
`nros-platform-cffi` with `posix-c-port` for a pure-cargo build.

**QEMU bare-metal Cortex-M3 with Zenoh:**
```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi", "platform-bare-metal", "ros-humble",
] }
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-bare-metal"] }
```

**Zephyr with XRCE-DDS and safety features:**
```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi", "platform-zephyr", "ros-humble", "alloc", "safety-e2e", "ffi-sync",
] }
nros-rmw-xrce-cffi = { path = "…/nros-rmw-xrce-cffi", features = ["platform-zephyr"] }
```

**Desktop Linux for development and testing:**
```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi", "platform-posix", "ros-humble", "std", "param-services",
] }
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-posix", "link-tcp", "ros-humble"] }
nros-platform-cffi = { path = "…/nros-platform-cffi", features = ["posix-c-port"] }
```

**FreeRTOS with lwIP networking:**
```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi", "platform-freertos", "ros-humble",
] }
nros-rmw-zenoh = { path = "…/nros-rmw-zenoh", features = ["platform-freertos"] }
```

**NuttX with XRCE-DDS over serial:**
```toml
[dependencies]
nros = { path = "…/nros", default-features = false, features = [
    "rmw-cffi", "platform-nuttx", "ros-humble",
] }
nros-rmw-xrce-cffi = { path = "…/nros-rmw-xrce-cffi", features = ["platform-nuttx", "xrce-serial"] }
```

## How the pieces link decoupled `nros` from concrete backends — `nros` carries
no RMW crate dependency. Instead:

- **RMW.** The consuming crate's `nros-rmw-*` dep ships a `#[ctor]`
  that calls `nros_rmw_<name>_register()` at lib load (POSIX
  `.init_array`) or the caller invokes it from `main` (bare-metal).
  The backend installs a `nros_rmw_vtable_t` into `nros-rmw-cffi`'s
  named registry. `Executor::open` resolves the registered backend;
  the concrete session type is always `CffiSession` (vtable-backed).
- **Platform.** `nros`'s `platform-*` feature resolves to
  `nros-platform-cffi`. The canonical `nros_platform_*` C symbols
  ship from `packages/core/nros-platform-<plat>/src/*.c`, linked at
  the consumer's build site — `posix-c-port` for pure-cargo POSIX
  builds, the standalone `lib<…>.a` for CMake builds, or the kernel
  build system for NuttX / Zephyr.
- **Alias TUs.** C translation units forward each transport C library's
  `z_*` / `uxr_*` platform calls to the canonical `nros_platform_*` symbols:
  `zpico-sys/c/zpico/platform_aliases.c` (default-on `platform-aliases`
  feature) for zenoh-pico, and `nros-rmw-xrce/src/platform_aliases.c` (always
  compiled into `nros-rmw-xrce-cffi`) for XRCE-DDS. (These replaced the
  former `zpico-platform-shim` / `xrce-platform-shim` crates, deleted in
  Phase 129.)

The default feature set is `std` only. No RMW backend or platform is
selected by default — the consuming crate chooses explicitly.

For implementation details on how to add a new platform, see the [Porting Guide](../porting/overview.md) and [Platform API Reference](../reference/platform-api.md).
