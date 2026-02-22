//! Platform initialization and `run()` entry point for ESP32-C3 QEMU.
//!
//! Uses `zpico-smoltcp` for socket management and `openeth-smoltcp` for Ethernet.

use esp_hal::rng::Rng;
use openeth_smoltcp::OpenEth;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
use zpico_smoltcp::SmoltcpBridge;

use zpico_platform_esp32_qemu::{clock, random};

use crate::config::Config;

// NOTE: We intentionally do NOT import `type Result<T>` in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

/// Helper to create a socket set with pre-allocated storage
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
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
///     let mut executor = Executor::<_, 0, 0>::open(&exec_config)?;
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

    // Step 4: Initialize OpenETH driver (replaces WiFi stack)
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

    // Step 5: Create smoltcp interface
    esp_println::println!("");
    esp_println::println!("Creating network interface...");

    let mac_addr = EthernetAddress::from_bytes(&mac);
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    let mut iface = Interface::new(iface_config, &mut eth, clock::now());
    let mut sockets = unsafe { create_socket_set() };

    // Step 6: Configure static IP (no DHCP in QEMU)
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

    // Step 7: Initialize transport bridge
    SmoltcpBridge::init();

    // Create and register TCP + UDP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(&mut sockets);
        zpico_smoltcp::create_and_register_udp_sockets(&mut sockets);
    }

    // Store global state for poll callback
    unsafe {
        zpico_platform_esp32_qemu::network::set_network_state(
            &mut iface as *mut Interface,
            &mut sockets as *mut SocketSet<'static>,
            &mut eth as *mut OpenEth as *mut (),
        );

        zpico_smoltcp::set_poll_callback(zpico_platform_esp32_qemu::network::smoltcp_network_poll);
    }

    esp_println::println!("Network ready.");
    esp_println::println!("");

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
