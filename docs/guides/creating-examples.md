# Creating Examples

This guide explains how to create new nros examples for each supported platform. All examples should use **generated message bindings** from `cargo nano-ros generate` — never hand-write message types.

## Message Generation (All Platforms)

Every example that uses ROS message types needs:

1. **`package.xml`** declaring message dependencies
2. **`generated/`** directory produced by `cargo nano-ros generate`
3. **`[patch.crates-io]`** entries in `.cargo/config.toml` pointing to the generated crates

### Step 1: Create `package.xml`

```xml
<?xml version="1.0"?>
<package format="3">
  <name>my_example</name>
  <version>0.1.0</version>
  <description>My example description</description>
  <maintainer email="dev@example.com">Developer</maintainer>
  <license>MIT OR Apache-2.0</license>
  <depend>std_msgs</depend>
  <!-- Add more interface packages as needed: -->
  <!-- <depend>geometry_msgs</depend> -->
  <!-- <depend>sensor_msgs</depend> -->
  <export>
    <build_type>ament_cargo</build_type>
  </export>
</package>
```

### Step 2: Generate bindings

For **BSP and QEMU examples** (local development with path dependencies):

```bash
source /opt/ros/humble/setup.sh
cargo nano-ros generate --config --nano-ros-path ../../../crates
```

For **native and Zephyr examples** (where `.cargo/config.toml` is maintained manually):

```bash
source /opt/ros/humble/setup.sh
cargo nano-ros generate
```

The `--config` flag uses `ConfigPatcher` to add `[patch.crates-io]` entries idempotently, preserving existing `[build]` and `[target.*]` sections. The `--nano-ros-path` flag also patches core and serdes crates to use local paths.

This produces `generated/builtin_interfaces/` and `generated/std_msgs/` (plus any other transitive dependencies).

### Step 3: Add the message crate to `Cargo.toml`

```toml
[dependencies]
std_msgs = { version = "*", default-features = false }
```

### Step 4: Use the generated types

```rust
use std_msgs::msg::Int32;
```

### Step 5: Register in `just generate-bindings`

Add the example to the `generate-bindings` recipe in the justfile so bindings can be regenerated in bulk:

```just
cd examples/<platform>/<name> && cargo nano-ros generate --config --nano-ros-path ../../../crates
```

---

## Native Examples (`examples/native/`)

Native examples run on desktop Linux with the full `std` library.

### Directory structure

```
examples/native/my-example/
├── Cargo.toml
├── package.xml
├── src/
│   └── main.rs
├── generated/          # cargo nano-ros generate output
│   ├── builtin_interfaces/
│   └── std_msgs/
└── .cargo/
    └── config.toml     # [patch.crates-io] entries
```

### `Cargo.toml`

```toml
[package]
name = "native-my-example"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false

[[bin]]
name = "my-example"
path = "src/main.rs"

[features]
default = []
zenoh = ["nros/zenoh"]

[dependencies]
nros = { path = "../../../packages/core/nros", default-features = false, features = ["std"] }
std_msgs = { version = "*", default-features = false }
log = "0.4"
env_logger = "0.11"
```

### `src/main.rs`

```rust
use log::info;
use nros::prelude::*;
use std_msgs::msg::Int32;

fn main() {
    env_logger::init();

    let context = Context::from_env().expect("Failed to create context");
    let mut executor = context.create_basic_executor();
    let mut node = executor
        .create_node("my_node".namespace("/demo"))
        .expect("Failed to create node");

    let publisher = node
        .create_publisher::<Int32>(PublisherOptions::new("/chatter"))
        .expect("Failed to create publisher");

    let mut count: i32 = 0;
    loop {
        publisher.publish(&Int32 { data: count }).ok();
        info!("Published: {}", count);
        count = count.wrapping_add(1);
        let _ = executor.spin_once(1000);
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
```

### Key points

- Use `nros` with `features = ["std"]` and gate real transport behind `zenoh` feature
- Entry point is a standard `fn main()`
- Use `Context::from_env()` to read `ROS_DOMAIN_ID`, `ZENOH_LOCATOR`, `ZENOH_MODE`
- Logging via `log` + `env_logger` (`RUST_LOG=info cargo run`)

---

## BSP Examples (`examples/qemu/`, `examples/stm32f4/`)

BSP examples run on bare-metal embedded targets via a Board Support Package that wraps all hardware and network setup.

### Directory structure

```
examples/qemu/my-example/
├── Cargo.toml
├── package.xml
├── src/
│   └── main.rs
├── generated/
│   ├── builtin_interfaces/
│   └── std_msgs/
└── .cargo/
    └── config.toml     # target + runner + [patch.crates-io]
```

### `Cargo.toml` (QEMU)

```toml
[package]
name = "qemu-my-example"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
publish = false

[[bin]]
name = "qemu-my-example"
test = false
bench = false

[features]
default = []
docker = ["nano-ros-platform-qemu/docker"]

[dependencies]
nano-ros-platform-qemu = { path = "../../../packages/platform/nano-ros-platform-qemu" }
std_msgs = { version = "*", default-features = false }
panic-semihosting = { version = "0.6", features = ["exit"] }
```

### `.cargo/config.toml` (QEMU)

Generated by `cargo nano-ros generate --config --nano-ros-path ../../../crates`. The result looks like:

```toml
[build]
target = "thumbv7m-none-eabi"

[target.thumbv7m-none-eabi]
runner = "qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -semihosting-config enable=on,target=native -kernel"
rustflags = ["-C", "link-arg=-Tlink.x"]

[patch.crates-io]
nros-core = { path = "../../../packages/core/nros-core" }
nros-serdes = { path = "../../../packages/core/nros-serdes" }
builtin_interfaces = { path = "generated/builtin_interfaces" }
std_msgs = { path = "generated/std_msgs" }
```

Create the `[build]` and `[target.*]` sections **first**, then run `cargo nano-ros generate --config` to add the patch entries. The ConfigPatcher preserves existing sections.

### `src/main.rs` (QEMU talker)

```rust
#![no_std]
#![no_main]

use nano_ros_platform_qemu::prelude::*;
use nano_ros_platform_qemu::println;
use panic_semihosting as _;
use std_msgs::msg::Int32;

#[entry]
fn main() -> ! {
    run_node(Config::default(), |node| {
        let publisher = node.create_publisher::<Int32>("/chatter")?;

        for i in 0..10i32 {
            for _ in 0..100 {
                node.spin_once(10);
            }
            publisher.publish(&Int32 { data: i })?;
            println!("Published: {}", i);
        }

        Ok(())
    })
}
```

### `src/main.rs` (QEMU listener)

```rust
#![no_std]
#![no_main]

use core::sync::atomic::{AtomicU32, Ordering};
use nano_ros_platform_qemu::prelude::*;
use nano_ros_platform_qemu::println;
use panic_semihosting as _;
use std_msgs::msg::Int32;

static MSG_COUNT: AtomicU32 = AtomicU32::new(0);

fn on_message(msg: &Int32) {
    println!("Received: {}", msg.data);
    MSG_COUNT.fetch_add(1, Ordering::SeqCst);
}

#[entry]
fn main() -> ! {
    run_node(Config::listener(), |node| {
        let _sub = node.create_subscription::<Int32>("/chatter", on_message)?;

        loop {
            node.spin_once(10);
            if MSG_COUNT.load(Ordering::SeqCst) >= 10 {
                break;
            }
        }
        Ok(())
    })
}
```

### Key points

- `#![no_std]` + `#![no_main]` — bare-metal, no standard library
- Entry point: `#[entry] fn main() -> !` (from `cortex-m-rt`)
- Platform crate handles hardware init, networking, and zenoh transport
- `Config::default()` for talkers (IP `192.0.2.10`), `Config::listener()` for listeners (IP `192.0.2.11`)
- Output via `println!` macro (semihosting)
- `test = false` and `bench = false` in `[[bin]]` (no test harness for `no_std`)

### STM32F4 variant

Same pattern, but use `nano-ros-platform-stm32f4` and `defmt` logging:

```toml
[dependencies]
nano-ros-platform-stm32f4 = { path = "../../../packages/platform/nano-ros-platform-stm32f4" }
std_msgs = { version = "*", default-features = false }
panic-probe = { version = "0.3", features = ["print-defmt"] }
defmt-rtt = "0.4"
```

```rust
use nano_ros_platform_stm32f4::prelude::*;
use std_msgs::msg::Int32;

#[entry]
fn main() -> ! {
    run_node(Config::nucleo_f429zi(), |node| {
        let publisher = node.create_publisher::<Int32>("/chatter")?;
        // ...
    })
}
```

The STM32F4 target is `thumbv7em-none-eabihf` (Cortex-M4F with hardware float).

---

## Zephyr Examples (`examples/zephyr/`)

Zephyr examples build as a static library linked into the Zephyr application by CMake.

### Directory structure

```
examples/zephyr/my-example/
├── Cargo.toml
├── package.xml
├── CMakeLists.txt
├── prj.conf
├── src/
│   └── lib.rs          # Note: lib.rs, not main.rs
├── generated/
│   ├── builtin_interfaces/
│   └── std_msgs/
└── .cargo/
    └── config.toml     # patches for ALL nros crates
```

### `Cargo.toml`

The package name **must** be `rustapp` for `zephyr-lang-rust` integration:

```toml
[package]
name = "rustapp"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["staticlib"]

[dependencies]
zephyr = "0.1.0"
log = "0.4"
nros = { version = "*", default-features = false, features = ["shim-zephyr"] }
std_msgs = { version = "*", default-features = false }

[profile.release]
opt-level = "s"
lto = true
```

### `.cargo/config.toml`

Zephyr examples patch **all** nros crates (not just core/serdes):

```toml
[patch.crates-io]
nros = { path = "../../../packages/core/nros" }
nros-core = { path = "../../../packages/core/nros-core" }
nros-serdes = { path = "../../../packages/core/nros-serdes" }
nros-node = { path = "../../../packages/core/nros-node" }
nano-ros-transport = { path = "../../../packages/core/nano-ros-transport" }
nros-params = { path = "../../../packages/core/nros-params" }
nros-macros = { path = "../../../packages/core/nros-macros" }
nano-ros-transport-zenoh = { path = "../../../packages/transport/nano-ros-transport-zenoh" }
nano-ros-transport-zenoh-sys = { path = "../../../packages/transport/nano-ros-transport-zenoh-sys" }
builtin_interfaces = { path = "generated/builtin_interfaces" }
std_msgs = { path = "generated/std_msgs" }
```

### `CMakeLists.txt`

```cmake
find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(my_example)

# nros BSP for Zephyr (C glue)
set(BSP_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../../packages/bsp/nano-ros-bsp-zephyr)
target_sources(app PRIVATE ${BSP_DIR}/src/bsp_zephyr.c)
target_include_directories(app PRIVATE ${BSP_DIR}/include)

# zenoh-pico C shim
set(SHIM_DIR ${CMAKE_CURRENT_SOURCE_DIR}/../../../packages/transport/nano-ros-transport-zenoh-sys/c)
target_sources(app PRIVATE ${SHIM_DIR}/shim/zenoh_shim.c)
target_include_directories(app PRIVATE ${SHIM_DIR}/include)

# Disable Z_FEATURE_INTEREST to avoid write-filter mutex exhaustion
zephyr_compile_definitions(Z_FEATURE_INTEREST=0 Z_FEATURE_MATCHING=0)

# Build Rust staticlib and link
rust_cargo_application()
```

### `prj.conf`

Critical settings (copy from an existing Zephyr example and adjust):

```ini
# Rust support
CONFIG_RUST=y
CONFIG_RUST_ALLOC=y

# Networking
CONFIG_NETWORKING=y
CONFIG_NET_IPV4=y
CONFIG_NET_TCP=y
CONFIG_NET_UDP=y

# POSIX API (required by zenoh-pico)
CONFIG_POSIX_API=y
CONFIG_MAX_PTHREAD_MUTEX_COUNT=32   # Default 5 is too low for zenoh-pico
CONFIG_MAX_PTHREAD_COND_COUNT=16

# zenoh-pico
CONFIG_ZENOH_PICO=y
CONFIG_ZENOH_PICO_PUBLICATION=y
CONFIG_ZENOH_PICO_SUBSCRIPTION=y    # For listeners
CONFIG_ZENOH_PICO_LINK_TCP=y

# Stack and heap
CONFIG_MAIN_STACK_SIZE=16384
CONFIG_HEAP_MEM_POOL_SIZE=65536

# Static IP
CONFIG_NET_CONFIG_SETTINGS=y
CONFIG_NET_CONFIG_MY_IPV4_ADDR="192.0.2.1"
CONFIG_NET_CONFIG_MY_IPV4_NETMASK="255.255.255.0"
CONFIG_NET_CONFIG_MY_IPV4_GW="192.0.2.2"
```

### `src/lib.rs`

```rust
#![no_std]

use log::{error, info};
use nros::{ShimExecutor, ShimNodeError};
use std_msgs::msg::Int32;

#[unsafe(no_mangle)]
extern "C" fn rust_main() {
    unsafe { zephyr::set_logger().ok(); }

    info!("nros Zephyr Example");

    if let Err(e) = run() {
        error!("Error: {:?}", e);
    }
}

fn run() -> Result<(), ShimNodeError> {
    let mut executor = ShimExecutor::new(b"tcp/192.0.2.2:7447\0")?;
    let mut node = executor.create_node("my_node")?;
    let publisher = node.create_publisher::<Int32>("/chatter")?;

    let mut counter: i32 = 0;
    loop {
        publisher.publish(&Int32 { data: counter })?;
        info!("Published: {}", counter);
        counter = counter.wrapping_add(1);
        let _ = executor.spin_once(1000);
    }
}
```

### Key points

- **Package name must be `rustapp`** — the `zephyr-lang-rust` build system expects this
- Source file is `src/lib.rs` (staticlib), not `src/main.rs`
- Entry point: `#[unsafe(no_mangle)] extern "C" fn rust_main()`
- Use `nros` with `features = ["shim-zephyr"]`
- Zenoh locator is a null-terminated byte string: `b"tcp/192.0.2.2:7447\0"`
- `CONFIG_MAX_PTHREAD_MUTEX_COUNT=32` is critical — zenoh-pico needs ~8+ POSIX mutexes
- Build via `west build`, not `cargo build`

---

## Platform Comparison

| | Native | Platform (QEMU/STM32F4) | Zephyr |
|---|---|---|---|
| **Entry point** | `fn main()` | `#[entry] fn main() -> !` | `extern "C" fn rust_main()` |
| **`std` support** | Yes | No | No |
| **Source file** | `src/main.rs` | `src/main.rs` | `src/lib.rs` |
| **Crate type** | Binary | Binary | Staticlib |
| **Package name** | Any | Any | Must be `rustapp` |
| **Main crate** | `nros` | `nano-ros-platform-qemu`/`stm32f4` | `nros` (shim-zephyr) |
| **Transport** | Feature-gated `zenoh` | Built into platform crate | Built into BSP (C) |
| **Logging** | `env_logger` | Semihosting / `defmt` | `zephyr::set_logger()` |
| **Build system** | `cargo build` | `cargo build` | `west build` (CMake) |
| **Generate flags** | `cargo nano-ros generate` | `--config --nano-ros-path` | `cargo nano-ros generate` |

## See Also

- [examples/README.md](../../examples/README.md) — Running existing examples
- [message-generation.md](message-generation.md) — Message generation details
- [zephyr-setup.md](zephyr-setup.md) — Zephyr workspace setup
