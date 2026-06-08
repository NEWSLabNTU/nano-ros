# Board Integration

How you consume nano-ros depends on what your project's build
system looks like, not on what RTOS you're targeting. This page maps
**user profile → recommended path**. Pick your row, follow the
linked guide.

The architecture behind the matrix lives in
[`docs/design/0012-board-bsp-integration-architecture.md`](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0012-board-bsp-integration-architecture.md)
(layered model, vendor-BSP scaling, plan).

## Consumption matrix

| Profile | Workflow | Recommended path |
|---|---|---|
| **Cargo-first Rust user (have RTOS sources)** | `cargo build --target <triple>` produces a single ELF. RTOS kernel built in-tree by Cargo. | [Generic board crate](#generic-board-crate) + env vars pointing at `FREERTOS_DIR` / `THREADX_DIR` / etc. |
| **Vendor-IDE user (STM32CubeIDE, MCUXpresso IDE, etc.)** | Vendor's existing FreeRTOS / ThreadX project; nano-ros as an `add_subdirectory()` library. | [`add_subdirectory(third_party/nano-ros)`](../getting-started/build-as-subdirectory.md) from the vendor's project. |
| **Zephyr user (any board)** | `west` + DTS overlays. Vendor HALs come as Zephyr modules. | [Zephyr integration shell](../getting-started/integration-zephyr.md) — `projects:` entry in your `west.yml`. |
| **ESP-IDF user (any ESP32 chip)** | `idf.py build`. | [ESP-IDF integration shell](../getting-started/integration-esp-idf.md) — `idf.py add-dependency nano-ros`. |
| **NuttX user (any board)** | NuttX `apps/external/` + Kconfig. | [NuttX integration shell](../getting-started/integration-nuttx.md) — symlink under `apps/external/`. |
| **PX4 user** | PX4 build pipeline. | [PX4 integration shell](../getting-started/integration-px4.md) — `EXTERNAL_MODULES_LOCATION`. |
| **Niche RTOS / vendor fork** | Stock RTOS kernel + vendor driver SDK. Cargo-driven build. | Generic board crate + **vendor overlay crate** (~50 LOC). See the [Vendor Overlay cookbook](../porting/vendor-overlay.md). |

## Generic board crate

For Cargo-first users targeting one of the four supported kernel
families:

| Kernel | Crate | SDK env vars you set |
|---|---|---|
| FreeRTOS + lwIP | `nros-board-freertos` | `FREERTOS_DIR`, `FREERTOS_PORT`, `LWIP_DIR`, `FREERTOS_CONFIG_DIR` |
| ThreadX + NetX-Duo | `nros-board-threadx` | `THREADX_DIR`, `THREADX_CONFIG_DIR`, `NETX_DIR`, `NETX_CONFIG_DIR` |
| NuttX | `nros-board-nuttx` | `NUTTX_DIR` (kernel built by NuttX itself) |
| bare-metal Cortex-M + smoltcp | `nros-board-baremetal-cortex-m` | `BOARD_LINKER_SCRIPT_DIR` |

```toml
# user_app/Cargo.toml
[dependencies]
nros-board-freertos = "0.1"
nros = { version = "0.1", default-features = false, features = ["rmw-cffi", "platform-freertos", "ros-humble"] }
nros-rmw-zenoh = { version = "0.1", features = ["platform-freertos"] }
std_msgs = { version = "*", default-features = false }
```

```rust
// user_app/src/main.rs
use nros::prelude::*;
use nros_board_freertos::{Config, run};

fn main() -> ! {
    run(Config::from_toml(include_str!("../config.toml")), |config| {
        let mut executor = Executor::open(
            &ExecutorConfig::new(config.zenoh_locator).node_name("my_node"),
        )?;
        // publishers, subscriptions, services, actions, timers...
        Ok::<(), NodeError>(())
    })
}
```

```bash
export FREERTOS_DIR=$HOME/sdk/freertos/kernel
export FREERTOS_PORT=GCC/ARM_CM3
export LWIP_DIR=$HOME/sdk/freertos/lwip
cargo build --release --target thumbv7m-none-eabi
```

The generic crate's `build.rs` compiles the kernel + network stack
+ nano-ros platform glue into a single ELF. No vendor driver glue —
that's what overlay crates are for.

## Vendor overlay crate

When your board needs vendor-specific drivers (STM HAL,
NXP `fsl_*`, NVIDIA FSP, Renesas Synergy SSP, …) on top of one of
the generic crates, write a small (~50 LOC) overlay crate that:

1. Depends on the matching generic board crate.
2. Re-exports `Config` + `run`.
3. Implements `#[no_mangle]` board-init hooks
   (`nros_board_init_clocks`, `nros_board_init_eth`,
   `nros_board_init_extra_drivers`).
4. Pulls vendor HAL `.c` sources via its own `build.rs` cc-rs
   invocation.

The full cookbook + working precedents
(`nros-board-orin-spe`, `nros-board-mps2-an385-freertos`) live in
[Vendor Overlay Board Crate](../porting/vendor-overlay.md).

## Why so many paths

Per the
[architecture doc](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0012-board-bsp-integration-architecture.md),
each RTOS already has its own package manager (Zephyr's
`west` + DTS, ESP-IDF's component registry, NuttX's
`apps/external/`, PX4's `EXTERNAL_MODULES_LOCATION`). nano-ros
rides those rails instead
of trying to re-invent a single "embedded library" mechanism that
fits no vendor ecosystem cleanly.

The Cargo-first profile (with optional vendor overlay crates) covers
the gap where a user is **NOT** inside an existing RTOS-IDE project
and wants to drive their build from Cargo end-to-end. That's the
two-and-a-half rows at the top + bottom of the matrix.

## Not on the matrix

- **No common driver HAL.** STM `HAL_*`, NXP `fsl_*`, Espressif
  `esp_*`, Renesas `R_*` all stay vendor-owned. Overlay crates wrap
  them; nano-ros doesn't abstract over them.
- **No DTS-equivalent for non-Zephyr platforms.** Zephyr owns its
  board contract; everywhere else, board config is whatever your
  vendor IDE produces (CubeMX `.ioc`, NuttX `defconfig`, ESP-IDF
  `sdkconfig`).
- **No board crate per SKU.** Generic + overlay covers the long
  tail. If you have an exotic board with custom HAL, write a ~50 LOC
  overlay; nano-ros doesn't catalog every vendor SKU.

## Related reading

- [Vendor Overlay Board Crate](../porting/vendor-overlay.md) — the
  overlay cookbook.
- [Custom Board Package](../porting/custom-board.md) — older guide
  covering monolithic board crates (legacy pattern).
- [`add_subdirectory(third_party/nano-ros)`](../getting-started/build-as-subdirectory.md)
  — root CMake entry for vendor-IDE consumers.
- [Platform Model](./platform-model.md) — Boards vs Platforms;
  Layer 1's `<nros/platform_*.h>` contract overlays sit on top of.
- [RTOS Cooperation](./rtos-cooperation.md) — what the runtime
  expects from the OS underneath the platform layer.
