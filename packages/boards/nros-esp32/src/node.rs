//! Hardware initialization and entry point for ESP32 WiFi bare-metal
//!
//! Handles WiFi connection, DHCP, and smoltcp interface setup, then calls
//! user code with the configuration. Users create their own `nros` executor
//! and node inside the callback.
//!
//! Uses `zpico-smoltcp` for socket management.

use core::mem::MaybeUninit;

use esp_hal::rng::Rng;
use esp_hal::time::Instant;
use esp_radio::wifi::{self, ClientConfig, ModeConfig, WifiDevice};
use zpico_smoltcp::SmoltcpBridge;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::socket::dhcpv4;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use zpico_platform_esp32::clock;
use crate::config::{IpMode, NodeConfig};
use zpico_platform_esp32::random;

// NOTE: We intentionally do NOT define a `type Result<T>` alias in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

// Static storage for network objects (initialized by init_hardware, must
// outlive the function call so set_network_state pointers remain valid).
//
// WifiDevice<'d> uses PhantomData<&'d ()> — same layout for all lifetimes.
// These statics are never dropped on no_std bare-metal (no program exit).
static mut WIFI_DEV: MaybeUninit<WifiDevice<'static>> = MaybeUninit::uninit();
static mut NET_IFACE: MaybeUninit<Interface> = MaybeUninit::uninit();
static mut NET_SOCKETS: MaybeUninit<SocketSet<'static>> = MaybeUninit::uninit();

/// Helper to create a socket set with pre-allocated storage
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
}

/// Initialize all ESP32 hardware, WiFi, and the network stack.
///
/// Sets up ESP32 peripherals, heap allocator, WiFi connection (with
/// optional DHCP), smoltcp interface, and the zenoh-pico transport bridge.
/// After calling this, you can create an `Executor` and start using nano-ros.
///
/// This is automatically called by [`run()`]. Call it directly only when
/// using an alternative execution model that needs hardware initialized
/// before returning control to the framework.
///
/// # Panics
///
/// Panics if hardware initialization fails (WiFi, DHCP timeout).
/// Must be called exactly once before any nros operations.
#[allow(static_mut_refs)]
pub fn init_hardware(config: &NodeConfig) {
    esp_println::println!("");
    esp_println::println!("========================================");
    esp_println::println!("  nros ESP32-C3 WiFi Platform");
    esp_println::println!("========================================");
    esp_println::println!("");

    // Step 1: Initialize ESP32 peripherals
    esp_println::println!("Initializing ESP32-C3...");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Step 2: Set up heap allocator for WiFi stack
    // WiFi requires heap allocation (esp-radio uses alloc internally)
    esp_alloc::heap_allocator!(size: 100 * 1024);

    // Step 3: Initialize hardware RNG (for zenoh-pico session ID)
    let rng = Rng::new();
    let rng_seed = rng.random();
    random::seed(rng_seed);

    // Step 4: Start the esp-rtos scheduler (required for WiFi background processing)
    esp_println::println!("Starting WiFi scheduler...");
    let timg0 = esp_hal::timer::timg::TimerGroup::new(peripherals.TIMG0);
    let sw_ints =
        esp_hal::interrupt::software::SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);
    esp_rtos::start(timg0.timer0, sw_ints.software_interrupt0);

    // Step 5: Initialize WiFi radio
    esp_println::println!("Initializing WiFi...");
    let radio_controller = esp_radio::init().unwrap_or_else(|_| {
        esp_println::println!("ERROR: WiFi radio initialization failed");
        loop {
            core::hint::spin_loop();
        }
    });

    // Step 6: Create WiFi controller and device interfaces
    let (mut wifi_controller, interfaces) =
        wifi::new(&radio_controller, peripherals.WIFI, wifi::Config::default()).unwrap_or_else(
            |_| {
                esp_println::println!("ERROR: WiFi creation failed");
                loop {
                    core::hint::spin_loop();
                }
            },
        );

    // Step 7: Configure and connect WiFi
    esp_println::println!("Connecting to WiFi: {}", config.wifi.ssid);
    let client_config = ClientConfig::default()
        .with_ssid(alloc::string::String::from(config.wifi.ssid))
        .with_password(alloc::string::String::from(config.wifi.password));
    wifi_controller
        .set_config(&ModeConfig::Client(client_config))
        .unwrap_or_else(|_| {
            esp_println::println!("ERROR: WiFi config failed");
            loop {
                core::hint::spin_loop();
            }
        });
    wifi_controller.start().unwrap_or_else(|_| {
        esp_println::println!("ERROR: WiFi start failed");
        loop {
            core::hint::spin_loop();
        }
    });
    wifi_controller.connect().unwrap_or_else(|_| {
        esp_println::println!("ERROR: WiFi connect failed");
        loop {
            core::hint::spin_loop();
        }
    });

    // Wait for WiFi association
    esp_println::println!("Waiting for WiFi association...");
    while !wifi_controller.is_connected().unwrap_or(false) {
        core::hint::spin_loop();
    }
    esp_println::println!("WiFi connected!");

    // Step 8: Create smoltcp interface from WiFi device
    esp_println::println!("");
    esp_println::println!("Creating network interface...");

    // Get MAC address from WiFi STA device
    let wifi_dev = interfaces.sta;
    // Safety: WifiDevice<'d> uses PhantomData<&'d ()> — same layout for all 'd.
    // Stored in static that is never dropped (no_std bare-metal, no exit).
    unsafe { WIFI_DEV.write(core::mem::transmute(wifi_dev)) };
    let wifi_dev = unsafe { WIFI_DEV.assume_init_mut() };

    let mac = wifi_dev.mac_address();
    let mac_addr = EthernetAddress::from_bytes(&mac);
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    let iface = Interface::new(iface_config, wifi_dev, clock::now());
    unsafe { NET_IFACE.write(iface) };
    let sockets = unsafe { create_socket_set() };
    unsafe { NET_SOCKETS.write(sockets) };

    esp_println::println!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    // Step 9: Configure IP (DHCP or static)
    let iface = unsafe { NET_IFACE.assume_init_mut() };
    let sockets = unsafe { NET_SOCKETS.assume_init_mut() };

    match &config.ip_mode {
        IpMode::Dhcp => {
            esp_println::println!("  IP mode: DHCP");

            // Add DHCP socket
            let dhcp_socket = dhcpv4::Socket::new();
            let dhcp_handle = sockets.add(dhcp_socket);

            // Poll until IP acquired
            esp_println::println!("  Waiting for DHCP...");
            let timeout_ms = 30_000u64; // 30 second timeout
            let start = Instant::now();
            let wifi_dev = unsafe { WIFI_DEV.assume_init_mut() };

            loop {
                iface.poll(clock::now(), wifi_dev, sockets);

                // Check if we got an IP
                if iface
                    .ipv4_addr()
                    .is_some_and(|ip| !ip.is_unspecified())
                {
                    break;
                }

                // Check for DHCP events and apply config
                let event = sockets.get_mut::<dhcpv4::Socket>(dhcp_handle).poll();
                if let Some(dhcpv4::Event::Configured(dhcp_config)) = event {
                    iface.update_ip_addrs(|addrs| {
                        addrs
                            .push(IpCidr::new(
                                IpAddress::Ipv4(dhcp_config.address.address()),
                                dhcp_config.address.prefix_len(),
                            ))
                            .ok();
                    });
                    if let Some(router) = dhcp_config.router {
                        let _ = iface.routes_mut().add_default_ipv4_route(router);
                    }
                    break;
                }

                // Check timeout
                let elapsed_ms = Instant::now()
                    .duration_since_epoch()
                    .as_millis()
                    - start.duration_since_epoch().as_millis();
                if elapsed_ms > timeout_ms {
                    esp_println::println!("ERROR: DHCP timeout");
                    loop {
                        core::hint::spin_loop();
                    }
                }
            }

            if let Some(ip) = iface.ipv4_addr() {
                esp_println::println!("  IP: {}", ip);
            }
        }
        IpMode::Static {
            ip,
            prefix,
            gateway,
        } => {
            esp_println::println!(
                "  IP: {}.{}.{}.{}/{}",
                ip[0], ip[1], ip[2], ip[3], prefix
            );

            let ip_addr = Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]);
            iface.update_ip_addrs(|addrs| {
                addrs
                    .push(IpCidr::new(IpAddress::Ipv4(ip_addr), *prefix))
                    .ok();
            });

            if *gateway != [0, 0, 0, 0] {
                let gw = Ipv4Address::new(gateway[0], gateway[1], gateway[2], gateway[3]);
                let _ = iface.routes_mut().add_default_ipv4_route(gw);
                esp_println::println!(
                    "  Gateway: {}.{}.{}.{}",
                    gateway[0], gateway[1], gateway[2], gateway[3]
                );
            }
        }
    }

    // Step 10: Initialize transport bridge
    SmoltcpBridge::init();

    // Create and register TCP + UDP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(sockets);
        zpico_smoltcp::create_and_register_udp_sockets(sockets);
    }

    // Store global state for poll callback (via zpico-platform-esp32)
    let wifi_dev = unsafe { WIFI_DEV.assume_init_mut() };
    unsafe {
        zpico_platform_esp32::network::set_network_state(
            iface as *mut Interface,
            sockets as *mut SocketSet<'static>,
            wifi_dev as *mut WifiDevice as *mut (),
        );

        zpico_smoltcp::set_poll_callback(zpico_platform_esp32::network::smoltcp_network_poll);
    }

    esp_println::println!("");
    esp_println::println!("Hardware initialization complete.");
    esp_println::println!("");

    // Prevent wifi_controller and radio_controller from being dropped
    // (Drop would deinit the WiFi radio). On no_std bare-metal, these
    // leak intentionally — the program never exits.
    // wifi_controller borrows radio_controller, so forget it first.
    core::mem::forget(wifi_controller);
    core::mem::forget(radio_controller);
}

/// Run an ESP32 WiFi application
///
/// This is the main entry point for ESP32 WiFi applications.
/// It handles all WiFi, network, and smoltcp initialization, then calls
/// your application code with the configuration. Users create their own
/// `nros` executor and node inside the callback.
///
/// # Returns
///
/// Never returns (`-> !`). Loops forever after the application function completes
/// or on error.
pub fn run<F, E: core::fmt::Debug>(config: NodeConfig, f: F) -> !
where
    F: FnOnce(&NodeConfig) -> core::result::Result<(), E>,
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
