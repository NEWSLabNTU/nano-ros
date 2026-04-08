# Board Crate Implementation Guide

A board crate is the user-facing entry point for a specific hardware target.
It wraps the low-level platform crate (`zpico-platform-*`) with a convenient
API: a `Config` struct, a `run()` function, and hardware initialization logic
for networking (Ethernet, WiFi, serial).

## When You Need a Board Crate

You need a board crate when you have:
- A new MCU or development board
- A new RTOS + board combination (e.g., FreeRTOS on STM32)
- A new network transport on an existing board (e.g., adding WiFi to a
  board that only had Ethernet)

You do **not** need a board crate for POSIX (Linux/macOS) — the `platform-posix`
feature uses OS sockets directly with no board-specific setup.

## Crate Structure

```
packages/boards/nros-<board>/
├── Cargo.toml
├── .gitignore          # /target/
└── src/
    ├── lib.rs          # Re-exports, utility functions
    ├── config.rs       # Config struct with builders
    ├── node.rs         # Hardware init + run() entry point
    └── error.rs        # Board-specific error type (optional)
```

## Config Struct

The `Config` struct holds all hardware and network settings. Fields are
gated by Cargo features so only relevant settings exist for each transport.

```rust
pub struct Config {
    // Ethernet fields (gated by "ethernet" feature)
    #[cfg(feature = "ethernet")]
    pub mac: [u8; 6],
    #[cfg(feature = "ethernet")]
    pub ip: [u8; 4],
    #[cfg(feature = "ethernet")]
    pub prefix: u8,
    #[cfg(feature = "ethernet")]
    pub gateway: [u8; 4],

    // Serial fields (gated by "serial" feature)
    #[cfg(feature = "serial")]
    pub uart_base: usize,
    #[cfg(feature = "serial")]
    pub baudrate: u32,

    // Common (always present)
    pub zenoh_locator: &'static str,
    pub domain_id: u32,
}
```

### Factory Methods

Provide sensible defaults for common test topologies:

```rust
impl Config {
    /// Default talker config (IP .10, QEMU TAP bridge)
    pub fn default() -> Self { ... }

    /// Listener config (IP .11, QEMU TAP bridge)
    pub fn listener() -> Self { ... }

    /// Parse from TOML string (compile-time embedded)
    pub fn from_toml(toml: &'static str) -> Self { ... }
}
```

### Builder Methods

```rust
impl Config {
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self { self.mac = mac; self }
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self { self.ip = ip; self }
    pub fn with_gateway(mut self, gw: [u8; 4]) -> Self { self.gateway = gw; self }
    pub fn with_zenoh_locator(mut self, loc: &'static str) -> Self { ... }
    pub fn with_domain_id(mut self, id: u32) -> Self { ... }
    // ... one per field
}
```

### TOML Parsing

`from_toml()` parses a compile-time-embedded config string. Use a minimal
TOML parser (the existing board crates use a simple line-by-line parser —
no `toml` crate dependency needed in `no_std`).

```rust
pub fn from_toml(toml: &'static str) -> Self {
    let mut config = Self::default();
    // Parse [network], [zenoh], [serial], [scheduling] sections
    // See nros-mps2-an385/src/config.rs for reference implementation
    config
}
```

## Hardware Initialization (node.rs)

The `node.rs` module contains the hardware init sequence and the `run()`
entry point.

### Initialization Order

The init sequence must follow this order:

```
1. Clock           — init_hardware_timer()
2. Cycle counter   — CycleCounter::enable() (if available)
3. RNG seed        — seed PRNG with entropy
4. Transport init  — Ethernet OR Serial (based on features)
5. Application     — call user closure
```

### Ethernet Transport Setup

For boards with an Ethernet MAC/PHY:

```rust
#[cfg(feature = "ethernet")]
fn init_ethernet(config: &Config) {
    // 1. Create the Ethernet driver
    let eth = MyEthDriver::new(ETH_BASE_ADDR);
    eth.init();

    // 2. Create smoltcp Interface
    let mut iface = Interface::new(smoltcp_config, &mut eth, smoltcp_now());
    iface.update_ip_addrs(|addrs| {
        addrs.push(IpCidr::new(config.ip.into(), config.prefix));
    });
    iface.routes_mut().add_default_ipv4_route(config.gateway.into());

    // 3. Create SocketSet with pre-allocated storage
    let mut sockets = SocketSet::new(&mut SOCKET_STORAGE[..]);

    // 4. Create TCP + UDP sockets via zpico-smoltcp
    zpico_smoltcp::SmoltcpBridge::create_sockets(&mut sockets);

    // 5. Seed ephemeral port (avoid TAP collisions between runs)
    iface.inner_mut().seed_ephemeral_port(entropy_source());

    // 6. Register network poll callback
    unsafe {
        zpico_platform::network::set_network_state(
            &mut iface as *mut _,
            &mut sockets as *mut _,
            &mut eth as *mut _ as *mut (),
        );
    }
}
```

**Static storage:** Ethernet peripherals and smoltcp state must live in
static storage (they are referenced by FFI callbacks). Use
`static mut` with `MaybeUninit`:

```rust
static mut ETH_DEVICE: MaybeUninit<MyEthDriver> = MaybeUninit::uninit();
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();
```

### Adding a New Ethernet Driver

If your board has an Ethernet peripheral not yet supported:

1. **Create a driver crate** in `packages/drivers/` (e.g., `my-eth-smoltcp`)
2. Implement smoltcp's `Device` trait:

```rust
impl Device for MyEthDriver {
    type RxToken<'a> = MyRxToken<'a>;
    type TxToken<'a> = MyTxToken<'a>;

    fn receive(&mut self, _: Instant) -> Option<(Self::RxToken<'_>, Self::TxToken<'_>)> {
        // Check if RX FIFO has data, return tokens
    }

    fn transmit(&mut self, _: Instant) -> Option<Self::TxToken<'_>> {
        // Check if TX is available, return token
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let mut caps = DeviceCapabilities::default();
        caps.max_transmission_unit = 1514;
        caps.medium = Medium::Ethernet;
        caps
    }
}
```

3. Existing driver examples:
   - `lan9118-smoltcp` — LAN9118 (QEMU MPS2-AN385, STM32F4)
   - `openeth-smoltcp` — Open Ethernet (QEMU ESP32)
   - `virtio-net-netx` — VirtIO-Net (ThreadX QEMU RISC-V)

### Serial Transport Setup

For UART-based communication:

```rust
#[cfg(feature = "serial")]
fn init_serial(config: &Config) {
    // 1. Initialize UART peripheral
    let uart = MyUart::new(config.uart_base, config.baudrate);
    uart.init();

    // 2. Register with zpico-serial (bare-metal only)
    unsafe {
        zpico_serial::register_uart(
            &uart as *const _ as *const (),
            my_uart_read,   // fn(*const (), *mut u8, usize) -> isize
            my_uart_write,  // fn(*const (), *const u8, usize) -> isize
        );
    }
}
```

On platforms where zenoh-pico has built-in serial support (ESP32, Zephyr),
you don't need `zpico-serial` — configure the UART and let zenoh-pico
use it directly.

**Serial locator format:** `serial/<device>#baudrate=<rate>`

```toml
[zenoh]
locator = "serial//dev/ttyUSB0#baudrate=115200"
```

### WiFi Transport Setup (ESP32)

For WiFi-enabled boards:

```rust
fn init_wifi(config: &Config) {
    // 1. Initialize WiFi peripheral (HAL-specific)
    let wifi = esp_wifi::init(config.ssid, config.password);

    // 2. Wait for DHCP or configure static IP
    wifi.connect()?;

    // 3. zenoh-pico uses the OS/HAL socket layer directly
    //    (no smoltcp needed — WiFi stack provides sockets)
}
```

### Adding Network Support via lwIP (RTOS)

For FreeRTOS with lwIP:

1. Build lwIP as a static library (CMake or build.rs)
2. Create FreeRTOS tasks for:
   - **RX poll task** — drains the Ethernet RX FIFO into lwIP
   - **tcpip thread** — lwIP's internal processing thread
3. zenoh-pico uses lwIP sockets via its FreeRTOS platform layer

The board crate creates these tasks during `init_hardware()`:

```rust
fn init_freertos_networking(config: &Config) {
    // 1. Initialize lwIP stack
    lwip_init();

    // 2. Create network interface
    netif_add(&mut netif, config.ip, config.netmask, config.gateway, ...);

    // 3. Start RX poll task
    xTaskCreate(rx_poll_task, "rx_poll", STACK_SIZE, ...);

    // 4. Start lwIP tcpip thread
    tcpip_init(None, core::ptr::null_mut());
}
```

### Adding Network Support via NetX Duo (ThreadX)

For ThreadX with NetX Duo:

```rust
fn init_threadx_networking(config: &Config) {
    // 1. Create IP instance
    nx_ip_create(&mut ip, "IP", config.ip, config.netmask, &pool, driver, ...);

    // 2. Enable TCP and UDP
    nx_tcp_enable(&ip);
    nx_udp_enable(&ip);

    // 3. Create ARP
    nx_arp_enable(&ip, &arp_area, ARP_AREA_SIZE);

    // 4. zenoh-pico uses NetX sockets via its ThreadX platform layer
}
```

## The `run()` Entry Point

The `run()` function is the standard entry point for examples:

```rust
pub fn run<F, E>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> Result<(), E>,
    E: core::fmt::Debug,
{
    // 1. Initialize hardware
    init_hardware(&config);

    // 2. Call application closure
    match f(&config) {
        Ok(()) => exit_success(),
        Err(e) => {
            println!("Error: {:?}", e);
            exit_failure()
        }
    }
}
```

For RTOS platforms, `run()` starts the scheduler after creating the
application task:

```rust
pub fn run<F, E>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> Result<(), E> + Send + 'static,
    E: core::fmt::Debug,
{
    init_hardware(&config);
    create_app_task(f, config);
    start_scheduler()  // Never returns
}
```

## Re-exports (lib.rs)

The board crate should re-export commonly needed items:

```rust
// Hardware entry point
pub use cortex_m_rt::entry;           // or esp_hal::entry, etc.

// Platform access (for advanced users)
pub use zpico_platform_<board>;

// Board API
pub use config::Config;
pub use node::{init_hardware, run};

// Cycle counter (for profiling)
pub use zpico_platform_<board>::timing::CycleCounter;

// Convenience macros
#[macro_export]
macro_rules! println {
    ($($arg:tt)*) => { cortex_m_semihosting::hprintln!($($arg)*) };
}
```

## Cargo Features

```toml
[features]
default = ["ethernet"]
ethernet = ["zpico-smoltcp", "my-eth-driver", "smoltcp"]
serial = ["zpico-serial", "my-uart-driver"]
docker = []      # Alternative IP config for Docker networking
link-tls = [...]  # Enable TLS (increases heap for mbedTLS)
```

At least one transport must be enabled. Enforce with:

```rust
#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: ethernet or serial");
```

## Checklist for a New Board Crate

- [ ] `Config` struct with feature-gated fields for each transport
- [ ] `Config::default()`, `Config::listener()`, `Config::from_toml()`
- [ ] Builder methods for all config fields
- [ ] `init_hardware()` with correct init order (clock → counter → RNG → transport)
- [ ] `run()` entry point with error handling
- [ ] Static storage for peripherals (`static mut` + `MaybeUninit`)
- [ ] Re-exports in `lib.rs` (entry macro, platform, Config, run)
- [ ] `Cargo.toml` with `ethernet`/`serial` features
- [ ] `.gitignore` with `/target/`
- [ ] At least one example using the board crate
- [ ] Test infrastructure (`just test-<board>` recipe)

## Reference Implementations

| Board Crate | Transport | RTOS | Complexity |
|-------------|-----------|------|------------|
| `nros-mps2-an385` | Ethernet (LAN9118) + Serial | Bare-metal | Low — best starting point |
| `nros-mps2-an385-freertos` | Ethernet (LAN9118) | FreeRTOS | Medium — RTOS tasks |
| `nros-esp32` | WiFi | Bare-metal | Medium — WiFi stack |
| `nros-nuttx-qemu-arm` | BSD sockets | NuttX | Low — POSIX-like |
| `nros-threadx-qemu-riscv64` | NetX Duo (VirtIO) | ThreadX | High — custom RTOS |
| `nros-threadx-linux` | veth sockets | ThreadX (Linux sim) | Low — simulation |
