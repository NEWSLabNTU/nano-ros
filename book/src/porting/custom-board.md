# Custom Board Package

A board crate is the top-level, batteries-included package that application code depends on. It provides three things:

1. **`Config` struct** -- network settings (IP, MAC, gateway), transport selection (Ethernet, serial, or WiFi via Cargo features), and a zenoh locator string.
2. **`run()` entry point** -- initializes hardware, starts the network stack, and calls a user-provided closure with `&Config`.
3. **Hardware initialization** -- PHY/MAC driver setup, interrupt configuration, clock init, and force-linking of platform shim crates.

Users never interact with platform crates or driver crates directly. They depend on a single board crate, call `run()`, and get a working environment for creating an `Executor`.

```rust,ignore
use nros_my_board::{Config, run};
use nros::prelude::*;

run(Config::default(), |config| {
    let exec_config = ExecutorConfig::new(config.zenoh_locator)
        .domain_id(config.domain_id);
    let mut executor = Executor::open(&exec_config)?;
    let mut node = executor.create_node("my_node")?;
    // publishers, subscriptions, services, actions, timers...
    Ok(())
})
```

## Board = platform + hardware

A board crate combines two layers:

- **Platform crate** (`nros-platform-<os>`) -- generic RTOS or bare-metal primitives: clock, sleep, threading, memory, random number generation. Implements the traits defined in `nros-platform`. See [Custom Platform](custom-platform.md) for how to write one.
- **Hardware drivers** -- board-specific peripherals: Ethernet controller, WiFi radio, serial UART. Driver crates in `packages/drivers/` (e.g., `lan9118-smoltcp`, `openeth-smoltcp`) implement the smoltcp `Device` trait for a specific Ethernet MAC/PHY.

The board crate glues these together: it depends on the platform crate for OS primitives, on driver crates for peripheral access, and on transport crates (`nros-smoltcp`, `zpico-serial`) for bridging the network stack to zenoh-pico. The result is a single dependency that gives application code everything it needs.

```text
Application
    |
    v
Board crate (nros-my-board)
    |
    +-- Platform crate (nros-platform-freertos)
    +-- Driver crate (lan9118-smoltcp)
    +-- Transport bridge (nros-smoltcp or zpico-serial)
    +-- Shim crate (zpico-platform-shim)
```

## Crate structure

A minimal board crate looks like this:

```text
nros-my-board/
  Cargo.toml
  .gitignore        # /target/
  src/
    lib.rs          # extern crate force-links, pub use Config, pub fn run()
    config.rs       # Config struct with network fields
    node.rs         # Hardware init sequence + run() implementation
```

### Cargo.toml

```toml
[package]
name = "nros-my-board"
version = "0.1.0"
edition = "2024"

[lib]
name = "nros_my_board"

[dependencies]
# Platform primitives (pick one)
nros-platform = { path = "../../core/nros-platform", features = ["platform-freertos"] }
nros-platform-freertos = { path = "../../core/nros-platform-freertos" }

# Force-link shim crate (provides zenoh-pico C symbols)
zpico-platform-shim = { path = "../../zpico/zpico-platform-shim", features = ["active"] }
zpico-sys = { path = "../../zpico/zpico-sys", default-features = false }

# Ethernet transport (optional, gated by feature)
nros-smoltcp = { path = "../../drivers/nros-smoltcp", optional = true }
lan9118-smoltcp = { path = "../../drivers/lan9118-smoltcp", optional = true }
smoltcp = { version = "0.12", default-features = false, optional = true, features = [
    "medium-ethernet", "proto-ipv4", "socket-tcp", "socket-udp",
] }

# Serial transport (optional, gated by feature)
zpico-serial = { path = "../../zpico/zpico-serial", optional = true }
my-uart-driver = { path = "../../drivers/my-uart", optional = true }

# Board-specific dependencies
cortex-m = "0.7"
cortex-m-rt = "0.7"
cortex-m-semihosting = "0.5"
panic-semihosting = "0.6"

[features]
default = ["ethernet"]
ethernet = ["dep:nros-smoltcp", "dep:lan9118-smoltcp", "dep:smoltcp"]
serial = ["dep:zpico-serial", "dep:my-uart-driver"]
```

### lib.rs

The lib module force-links shim crates and re-exports the public API.

```rust,ignore
#![no_std]

// Force-link: ensures the platform shim's C symbols (zpico FFI) are
// included in the final binary even though Rust code never calls them
// directly. Without this, the linker drops them and zenoh-pico fails.
extern crate zpico_platform_shim;

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};

// Re-export entry macro so examples can use #[entry]
pub use cortex_m_rt::entry;

// Convenience println! that routes to semihosting (QEMU) or UART
pub use cortex_m_semihosting;
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => { $crate::cortex_m_semihosting::hprintln!($($arg)*) };
}

pub fn exit_success() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_SUCCESS);
    loop {}
}

pub fn exit_failure() -> ! {
    cortex_m_semihosting::debug::exit(cortex_m_semihosting::debug::EXIT_FAILURE);
    loop {}
}
```

### config.rs

The `Config` struct holds all network and transport settings. Fields are
gated by Cargo features so only relevant fields exist for a given build.

```rust,ignore
#[derive(Clone)]
pub struct Config {
    #[cfg(feature = "ethernet")]
    pub mac: [u8; 6],
    #[cfg(feature = "ethernet")]
    pub ip: [u8; 4],
    #[cfg(feature = "ethernet")]
    pub prefix: u8,
    #[cfg(feature = "ethernet")]
    pub gateway: [u8; 4],

    #[cfg(feature = "serial")]
    pub uart_base: usize,
    #[cfg(feature = "serial")]
    pub baudrate: u32,

    pub zenoh_locator: &'static str,
    pub domain_id: u32,
}

#[cfg(feature = "ethernet")]
impl Default for Config {
    fn default() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [192, 0, 3, 10],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            #[cfg(feature = "serial")]
            uart_base: 0x4000_4000,
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }
}
```

Provide builder methods (`with_ip`, `with_mac`, `with_gateway`, etc.) and factory presets (`listener()`, `talker()`) for common test topologies. See `nros-board-mps2-an385/src/config.rs` for a complete example including `from_toml()` parsing.

### node.rs

The `run()` function is the entry point. For bare-metal targets it calls the user closure directly. For RTOS targets it creates a task, starts the scheduler, and runs the closure inside that task.

**Bare-metal pattern:**

```rust,ignore
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> Result<(), E>,
{
    init_hardware(&config);
    match f(&config) {
        Ok(()) => exit_success(),
        Err(e) => {
            hprintln!("Error: {:?}", e);
            exit_failure()
        }
    }
}
```

**RTOS pattern (FreeRTOS example):**

```rust,ignore
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> Result<(), E>,
{
    init_hardware(&config);
    // Create FreeRTOS task with the user closure
    create_app_task(f, config);
    // Start the scheduler -- never returns
    start_scheduler()
}
```

The `init_hardware()` function must follow this order:

1. **Clock** -- initialize the hardware timer (must happen before any clock reads)
2. **Cycle counter** -- enable DWT or equivalent for timing measurements
3. **RNG seed** -- seed PRNG with entropy (IP-based hash, semihosting time, etc.)
4. **Transport** -- Ethernet driver + smoltcp, or UART + zpico-serial
5. **Application** -- call the user closure

Ethernet peripherals and smoltcp state must live in `static mut` storage (they are referenced by FFI poll callbacks):

```rust,ignore
static mut ETH_DEVICE: MaybeUninit<Lan9118> = MaybeUninit::uninit();
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();
```

## Transport features

Board crates use Cargo features to select the communication transport:

- **`ethernet`** (typically the default) -- TCP/UDP via smoltcp or lwIP. Requires an Ethernet driver crate and `nros-smoltcp`.
- **`serial`** -- UART link via `zpico-serial`. Requires a UART driver crate.
- **`wifi`** -- WiFi via the platform's native stack (ESP32). The zenoh-pico layer uses OS sockets directly.

At least one transport must be enabled. Enforce this at compile time:

```rust,ignore
#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: `ethernet` or `serial`");
```

`Config` fields are `#[cfg(feature = "...")]`-gated per transport, so the struct only contains fields relevant to the enabled transport. Both `ethernet` and `serial` can be enabled simultaneously -- the zenoh locator string determines which transport is used at runtime (`tcp/...` for Ethernet, `serial/...` for UART).

## Reference implementations

Start from the crate closest to your target:

| Board crate | Transport | Platform | Location |
|---|---|---|---|
| `nros-board-mps2-an385` | Ethernet + Serial | Bare-metal | `packages/boards/nros-board-mps2-an385/` |
| `nros-board-mps2-an385-freertos` | Ethernet (lwIP) | FreeRTOS | `packages/boards/nros-board-mps2-an385-freertos/` |
| `nros-board-esp32` | WiFi | Bare-metal (ESP-IDF) | `packages/boards/nros-board-esp32/` |
| `nros-board-stm32f4` | Ethernet + Serial | Bare-metal | `packages/boards/nros-board-stm32f4/` |
| `nros-board-nuttx-qemu-arm` | BSD sockets | NuttX | `packages/boards/nros-board-nuttx-qemu-arm/` |
| `nros-board-threadx-qemu-riscv64` | NetX Duo | ThreadX | `packages/boards/nros-board-threadx-qemu-riscv64/` |

The bare-metal `nros-board-mps2-an385` is the simplest starting point. The FreeRTOS variant shows how to add RTOS task creation and lwIP networking.

## See also

- [Custom Platform](custom-platform.md) -- writing the platform crate that sits underneath a board crate
- [Porting Overview](overview.md) -- the three customization axes (RMW, platform, board)
- [Board Crate Implementation](../internals/board-crate.md) -- detailed internal reference with driver implementation, lwIP/NetX Duo setup, and a full checklist
