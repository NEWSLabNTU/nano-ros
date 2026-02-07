//! Simplified node API for ESP32 bare-metal
//!
//! Handles WiFi connection, DHCP, smoltcp interface, and zenoh-pico session
//! setup, exposing only ROS concepts to the user.

use core::ffi::{c_char, c_void};
use core::ptr;

use esp_hal::rng::Rng;
use esp_hal::time::Instant;
use esp_radio::wifi::{self, ClientConfig, ModeConfig, WifiDevice};
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::socket::dhcpv4;
use smoltcp::socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use zenoh_pico_shim_sys::{
    ShimCallback, zenoh_shim_close, zenoh_shim_declare_publisher, zenoh_shim_declare_subscriber,
    zenoh_shim_init, zenoh_shim_is_open, zenoh_shim_open, zenoh_shim_spin_once,
};

use crate::bridge::SmoltcpZenohBridge;
use crate::bridge::{smoltcp_register_socket, smoltcp_seed_random, smoltcp_set_poll_callback};
use crate::buffers;
use crate::clock;
use crate::config::{IpMode, NodeConfig};
use crate::error::Error;
use crate::publisher::Publisher;
use crate::subscriber::Subscriber;

// NOTE: We intentionally do NOT define a `type Result<T>` alias in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

// Global state for poll callback
static mut GLOBAL_IFACE: *mut Interface = ptr::null_mut();
static mut GLOBAL_SOCKETS: *mut SocketSet<'static> = ptr::null_mut();
static mut GLOBAL_DEVICE: *mut () = ptr::null_mut();

/// Network poll callback called by zenoh-pico
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        if GLOBAL_IFACE.is_null() || GLOBAL_SOCKETS.is_null() || GLOBAL_DEVICE.is_null() {
            return;
        }

        let device = &mut *(GLOBAL_DEVICE as *mut WifiDevice);
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        SmoltcpZenohBridge::poll(iface, device, sockets);
        // No clock::advance_clock_ms() needed - ESP32 hardware timer is authoritative
    }
}

/// Simplified node for ESP32 applications
///
/// This hides all low-level WiFi/smoltcp details.
/// Users interact only with ROS concepts (publishers, subscribers).
pub struct Node {
    _private: (), // prevent external construction
}

impl Node {
    /// Create a publisher for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (null-terminated, e.g., `b"demo/topic\0"`)
    pub fn create_publisher(&mut self, topic: &[u8]) -> core::result::Result<Publisher, Error> {
        let handle = unsafe { zenoh_shim_declare_publisher(topic.as_ptr() as *const c_char) };
        if handle < 0 {
            return Err(Error::PublisherDeclare);
        }
        Ok(unsafe { Publisher::from_handle(handle) })
    }

    /// Create a subscriber for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (null-terminated)
    /// * `callback` - Function called when messages arrive
    /// * `context` - User data passed to callback
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the node's lifetime.
    pub unsafe fn create_subscriber(
        &mut self,
        topic: &[u8],
        callback: Option<ShimCallback>,
        context: *mut c_void,
    ) -> core::result::Result<Subscriber, Error> {
        let cb = match callback {
            Some(f) => f,
            None => return Err(Error::SubscriberDeclare),
        };

        let handle =
            unsafe { zenoh_shim_declare_subscriber(topic.as_ptr() as *const c_char, cb, context) };
        if handle < 0 {
            return Err(Error::SubscriberDeclare);
        }
        Ok(unsafe { Subscriber::from_handle(handle) })
    }

    /// Process network events and dispatch callbacks
    ///
    /// Call this periodically to handle:
    /// - WiFi traffic
    /// - TCP/IP processing
    /// - Zenoh protocol messages
    /// - Subscriber callbacks
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Max wait time (0 = non-blocking)
    pub fn spin_once(&mut self, timeout_ms: u32) {
        unsafe {
            zenoh_shim_spin_once(timeout_ms);
        }
    }

    /// Shutdown the node gracefully
    pub fn shutdown(self) {
        unsafe {
            zenoh_shim_close();
            GLOBAL_IFACE = ptr::null_mut();
            GLOBAL_SOCKETS = ptr::null_mut();
            GLOBAL_DEVICE = ptr::null_mut();
        }
    }
}

/// Helper to create a socket set with pre-allocated storage
#[allow(static_mut_refs)]
unsafe fn create_socket_set() -> SocketSet<'static> {
    unsafe { SocketSet::new(&mut buffers::SOCKET_STORAGE[..]) }
}

/// Run a node with the given WiFi and node configuration
///
/// This is the main entry point for ESP32 applications.
/// It handles all WiFi, network, and zenoh initialization, then calls
/// your application code with a ready-to-use `Node`.
///
/// # Arguments
///
/// * `config` - WiFi + network configuration
/// * `f` - Application function that receives the initialized node
///
/// # Returns
///
/// Never returns (`-> !`). Loops forever after the application function completes
/// or on error.
///
/// # Example
///
/// ```ignore
/// #![no_std]
/// #![no_main]
///
/// use nano_ros_bsp_esp32::prelude::*;
///
/// #[entry]
/// fn main() -> ! {
///     run_node(
///         NodeConfig::new(WifiConfig::new("MySSID", "MyPassword")),
///         |node| {
///             let pub_ = node.create_publisher(b"demo/esp32\0")?;
///             for _ in 0..10 {
///                 node.spin_once(10);
///                 pub_.publish(b"Hello from ESP32!")?;
///             }
///             Ok(())
///         },
///     )
/// }
/// ```
pub fn run_node<F>(config: NodeConfig, f: F) -> !
where
    F: FnOnce(&mut Node) -> core::result::Result<(), Error>,
{
    esp_println::println!("");
    esp_println::println!("========================================");
    esp_println::println!("  nano-ros ESP32 BSP");
    esp_println::println!("========================================");
    esp_println::println!("");

    // Step 1: Initialize ESP32 peripherals
    esp_println::println!("Initializing ESP32...");
    let peripherals = esp_hal::init(esp_hal::Config::default());

    // Step 2: Set up heap allocator for WiFi stack
    // WiFi requires heap allocation (esp-radio uses alloc internally)
    esp_alloc::heap_allocator!(size: 100 * 1024);

    // Step 3: Initialize hardware RNG (for zenoh-pico session ID)
    let rng = Rng::new();
    let rng_seed = rng.random();

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
    let mut wifi_dev = interfaces.sta;
    let mac = wifi_dev.mac_address();
    let mac_addr = EthernetAddress::from_bytes(&mac);
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    let mut iface = Interface::new(iface_config, &mut wifi_dev, clock::now());
    let mut sockets = unsafe { create_socket_set() };

    esp_println::println!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        mac[0], mac[1], mac[2], mac[3], mac[4], mac[5]
    );

    // Step 9: Configure IP (DHCP or static)
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

            loop {
                iface.poll(clock::now(), &mut wifi_dev, &mut sockets);

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

    // Step 10: Initialize zenoh-pico bridge
    SmoltcpZenohBridge::init();

    // Seed RNG with hardware random number
    smoltcp_seed_random(rng_seed);

    // Create and register TCP sockets
    for i in 0..2 {
        let (rx_buf, tx_buf) = unsafe { buffers::get_tcp_buffers(i) };
        let tcp = TcpSocket::new(TcpSocketBuffer::new(rx_buf), TcpSocketBuffer::new(tx_buf));
        let handle = sockets.add(tcp);

        smoltcp_register_socket(unsafe {
            core::mem::transmute::<smoltcp::iface::SocketHandle, usize>(handle)
        });
    }

    // Store global state for poll callback
    unsafe {
        GLOBAL_DEVICE = &mut wifi_dev as *mut WifiDevice as *mut ();
        GLOBAL_IFACE = &mut iface as *mut Interface;
        GLOBAL_SOCKETS = &mut sockets as *mut SocketSet<'static>;

        smoltcp_set_poll_callback(Some(smoltcp_network_poll));
    }

    // Step 11: Initialize zenoh session
    esp_println::println!("");
    esp_println::println!("Connecting to zenoh router...");

    let ret = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const c_char) };
    if ret < 0 {
        esp_println::println!("ERROR: zenoh init failed ({})", ret);
        loop {
            core::hint::spin_loop();
        }
    }

    let ret = unsafe { zenoh_shim_open() };
    if ret < 0 {
        esp_println::println!("ERROR: zenoh open failed ({})", ret);
        loop {
            core::hint::spin_loop();
        }
    }

    if unsafe { zenoh_shim_is_open() } == 0 {
        esp_println::println!("ERROR: zenoh session not open");
        loop {
            core::hint::spin_loop();
        }
    }

    esp_println::println!("Connected!");
    esp_println::println!("");

    // Step 12: Create node and run user application
    let mut node = Node { _private: () };

    match f(&mut node) {
        Ok(()) => {
            esp_println::println!("");
            esp_println::println!("Application completed successfully.");
            node.shutdown();
            esp_println::println!("");
            esp_println::println!("========================================");
            esp_println::println!("  Done");
            esp_println::println!("========================================");
        }
        Err(e) => {
            esp_println::println!("");
            esp_println::println!("Application error: {:?}", e);
            node.shutdown();
        }
    }

    // Loop forever (ESP32 has no exit)
    #[allow(clippy::empty_loop)]
    loop {
        core::hint::spin_loop();
    }
}
