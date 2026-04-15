//! Platform initialization and `run()` entry point for ESP32-C3 QEMU.
//!
//! Uses `nros-smoltcp` for socket management and `openeth-smoltcp` for
//! Ethernet when the `ethernet` feature is enabled, or zenoh-pico's
//! built-in serial when the `serial` feature is enabled.

#[cfg(not(any(feature = "ethernet", feature = "serial")))]
compile_error!("Enable at least one transport: `ethernet` or `serial`");

use esp_hal::rng::Rng;

use nros_platform_esp32_qemu::random;

use crate::config::Config;

// NOTE: We intentionally do NOT import `type Result<T>` in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

// ---- Ethernet imports and static storage ----

#[cfg(feature = "ethernet")]
use core::mem::MaybeUninit;

#[cfg(feature = "ethernet")]
use openeth_smoltcp::OpenEth;
#[cfg(feature = "ethernet")]
use smoltcp::iface::{Interface, SocketSet};
#[cfg(feature = "ethernet")]
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
#[cfg(feature = "ethernet")]
use nros_platform_esp32_qemu::clock;
#[cfg(feature = "ethernet")]
use nros_smoltcp::SmoltcpBridge;

// Static storage for network objects (initialized by init_hardware, must
// outlive the function call so set_network_state pointers remain valid).
#[cfg(feature = "ethernet")]
static mut ETH_DEVICE: MaybeUninit<OpenEth> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
#[cfg(feature = "ethernet")]
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();

/// Helper to create a socket set with pre-allocated storage
#[cfg(feature = "ethernet")]
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { nros_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
}

// ---- Ethernet init ----

/// Initialize Ethernet transport via OpenETH + smoltcp.
#[cfg(feature = "ethernet")]
#[allow(static_mut_refs)]
fn init_ethernet(config: &Config) {
    // Initialize OpenETH driver
    esp_println::println!("Initializing OpenETH...");
    let openeth_config = openeth_smoltcp::Config {
        base_addr: openeth_smoltcp::ESP32C3_BASE,
        mac_addr: config.mac_addr,
    };
    let mut eth = unsafe { OpenEth::new(openeth_config) };
    eth.init();

    let mac = eth.mac_address();
    esp_println::println!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    // Move into static storage so pointers remain valid after this function returns
    unsafe { ETH_DEVICE.write(eth) };
    let eth = unsafe { ETH_DEVICE.assume_init_mut() };

    // Create smoltcp interface
    esp_println::println!("");
    esp_println::println!("Creating network interface...");

    let mac_addr = EthernetAddress::from_bytes(&mac);
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    let iface = Interface::new(iface_config, eth, smoltcp::time::Instant::from_millis(nros_platform_esp32_qemu::clock::clock_ms() as i64));
    unsafe { NET_IFACE.write(iface) };
    let sockets = unsafe { create_socket_set() };
    unsafe { NET_SOCKETS.write(sockets) };

    // Configure static IP (no DHCP in QEMU)
    let iface = unsafe { NET_IFACE.assume_init_mut() };
    let ip_addr = Ipv4Address::new(config.ip[0], config.ip[1], config.ip[2], config.ip[3]);
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::Ipv4(ip_addr), config.prefix))
            .ok();
    });

    let gw = Ipv4Address::new(
        config.gateway[0],
        config.gateway[1],
        config.gateway[2],
        config.gateway[3],
    );
    let _ = iface.routes_mut().add_default_ipv4_route(gw);

    esp_println::println!(
        "  IP: {}.{}.{}.{}/{}",
        config.ip[0], config.ip[1], config.ip[2], config.ip[3], config.prefix
    );
    esp_println::println!(
        "  Gateway: {}.{}.{}.{}",
        config.gateway[0], config.gateway[1], config.gateway[2], config.gateway[3]
    );

    // Initialize transport bridge
    SmoltcpBridge::init();

    // Create and register TCP + UDP sockets via transport crate
    let sockets = unsafe { NET_SOCKETS.assume_init_mut() };
    unsafe {
        nros_smoltcp::create_and_register_sockets(sockets);
        nros_smoltcp::create_and_register_udp_sockets(sockets);
    }

    // Store global state for poll callback
    let eth = unsafe { ETH_DEVICE.assume_init_mut() };
    unsafe {
        crate::network::set_network_state(
            iface as *mut Interface,
            sockets as *mut SocketSet<'static>,
            eth as *mut OpenEth as *mut (),
        );

        nros_smoltcp::set_poll_callback(crate::network::smoltcp_network_poll);
    }

    esp_println::println!("Ethernet ready.");
}

// ---- Serial init ----

/// Initialize serial transport.
///
/// ESP32-C3 QEMU uses zenoh-pico's built-in serial support — no additional
/// driver crates are needed. The zenoh locator string (e.g.,
/// `serial/UART_0#baudrate=115200`) tells zenoh-pico which UART to use.
#[cfg(feature = "serial")]
fn init_serial(config: &Config) {
    esp_println::println!("Initializing serial transport...");
    esp_println::println!("  Baud: {}", config.baudrate);
    esp_println::println!("  Locator: {}", config.zenoh_locator);
    esp_println::println!("Serial ready.");
}

// ---- Main init + run ----

/// Initialize all ESP32-C3 QEMU hardware and the transport stack.
///
/// Sets up ESP32 peripherals, heap allocator, RNG, and the selected
/// transport (Ethernet and/or serial depending on enabled features).
/// After calling this, you can create an `Executor` and start using nano-ros.
///
/// This is automatically called by [`run()`]. Call it directly only when
/// using an alternative execution model (e.g., RTIC) that needs hardware
/// initialized before returning control to the framework.
///
/// # Panics
///
/// Panics if hardware initialization fails. Must be called exactly once
/// before any nros operations.
pub fn init_hardware(config: &Config) {
    esp_println::println!("");
    esp_println::println!("========================================");
    esp_println::println!("  nros ESP32-C3 QEMU Platform");
    esp_println::println!("========================================");
    esp_println::println!("");

    // Step 1: Initialize ESP32 peripherals
    esp_println::println!("Initializing ESP32-C3...");
    let _peripherals = esp_hal::init(esp_hal::Config::default());

    // Step 2: Set up heap allocator (smaller than WiFi BSP - no WiFi overhead)
    esp_alloc::heap_allocator!(size: 64 * 1024);

    // Step 3: Initialize hardware RNG (for zenoh-pico session ID)
    let rng = Rng::new();
    let rng_seed = rng.random();
    random::seed(rng_seed);

    // Step 4: Initialize selected transport(s)
    #[cfg(feature = "ethernet")]
    init_ethernet(config);

    #[cfg(feature = "serial")]
    init_serial(config);

    esp_println::println!("");
}

/// Run an application with the given configuration.
///
/// This is the main entry point for ESP32-C3 QEMU applications.
/// It handles all hardware and network initialization, then calls
/// your application code with a reference to the config.
///
/// Inside the closure, use `Executor::open()` to create an executor
/// with full API access (publishers, subscriptions, services, actions,
/// timers, callbacks).
///
/// # Example
///
/// ```ignore
/// use nros_esp32_qemu::{Config, run};
/// use nros::prelude::*;
///
/// run(Config::default(), |config| {
///     let exec_config = ExecutorConfig::new(config.zenoh_locator)
///         .domain_id(config.domain_id);
///     let mut executor = Executor::open(&exec_config)?;
///     let mut node = executor.create_node("my_node")?;
///     // Full Executor API: publishers, subscriptions, services, actions...
///     Ok(())
/// })
/// ```
///
/// # Returns
///
/// Never returns (`-> !`). Loops forever after the application function completes
/// or on error.
pub fn run<F, E: core::fmt::Debug>(config: Config, f: F) -> !
where
    F: FnOnce(&Config) -> core::result::Result<(), E>,
{
    init_hardware(&config);

    // Run user application
    match f(&config) {
        Ok(()) => {
            esp_println::println!("");
            esp_println::println!("Application completed successfully.");
            esp_println::println!("");
            esp_println::println!("========================================");
            esp_println::println!("  Done");
            esp_println::println!("========================================");
        }
        Err(e) => {
            esp_println::println!("");
            esp_println::println!("Application error: {:?}", e);
        }
    }

    // Loop forever (ESP32 has no exit)
    #[allow(clippy::empty_loop)]
    loop {
        core::hint::spin_loop();
    }
}
