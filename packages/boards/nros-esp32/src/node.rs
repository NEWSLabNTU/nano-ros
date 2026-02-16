//! Simplified node API for ESP32 WiFi bare-metal
//!
//! Handles WiFi connection, DHCP, smoltcp interface, and zenoh-pico session
//! setup, exposing only ROS concepts to the user.
//!
//! Uses `nros-rmw-zenoh` for transport and `zpico-smoltcp` for socket management.

use esp_hal::rng::Rng;
use esp_hal::time::Instant;
use esp_radio::wifi::{self, ClientConfig, ModeConfig, WifiDevice};
use nros_core::RosMessage;
use nros_rmw::{QosSettings, Rmw, RmwConfig, Session, SessionMode, TopicInfo};
use nros_rmw_zenoh::ZenohRmw;
use nros_rmw_zenoh::shim::ShimSession;
use zpico_smoltcp::SmoltcpBridge;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::socket::dhcpv4;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use zpico_platform_esp32::clock;
use crate::config::{IpMode, NodeConfig};
use crate::error::Error;
use crate::publisher::Publisher;
use zpico_platform_esp32::random;
use crate::subscriber::Subscription;

// NOTE: We intentionally do NOT define a `type Result<T>` alias in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

/// Simplified node for ESP32 WiFi applications
///
/// This hides all low-level WiFi/smoltcp details.
/// Users interact only with ROS concepts (publishers, subscriptions).
pub struct Node {
    session: ShimSession,
    domain_id: u32,
}

impl Node {
    /// Create a typed publisher for a ROS 2 topic
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic: &str,
    ) -> core::result::Result<Publisher<M>, Error> {
        let topic_info = TopicInfo {
            name: topic,
            type_name: M::TYPE_NAME,
            type_hash: M::TYPE_HASH,
            domain_id: self.domain_id,
        };
        let publisher = self.session.create_publisher(&topic_info, QosSettings::default())?;
        Ok(Publisher::new(publisher))
    }

    /// Create a typed subscription for a ROS 2 topic (pull-based)
    ///
    /// Returns a `Subscription` that you poll with `try_recv()` in your main loop.
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic: &str,
    ) -> core::result::Result<Subscription<M>, Error> {
        let topic_info = TopicInfo {
            name: topic,
            type_name: M::TYPE_NAME,
            type_hash: M::TYPE_HASH,
            domain_id: self.domain_id,
        };
        let subscriber = self.session.create_subscriber(&topic_info, QosSettings::default())?;
        Ok(Subscription::new(subscriber))
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
        let _ = self.session.spin_once(timeout_ms);
    }

    /// Shutdown the node gracefully
    pub fn shutdown(self) {
        // ShimSession closes on drop
        drop(self.session);
        unsafe {
            zpico_platform_esp32::network::clear_network_state();
        }
    }
}

/// Helper to create a socket set with pre-allocated storage
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
}

/// Run a node with the given WiFi and node configuration
///
/// This is the main entry point for ESP32 WiFi applications.
/// It handles all WiFi, network, and zenoh initialization, then calls
/// your application code with a ready-to-use `Node`.
///
/// # Returns
///
/// Never returns (`-> !`). Loops forever after the application function completes
/// or on error.
pub fn run_node<F>(config: NodeConfig, f: F) -> !
where
    F: FnOnce(&mut Node) -> core::result::Result<(), Error>,
{
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

    // Step 10: Initialize transport bridge
    SmoltcpBridge::init();

    // Create and register TCP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(&mut sockets);
    }

    // Store global state for poll callback (via zpico-platform-esp32)
    unsafe {
        zpico_platform_esp32::network::set_network_state(
            &mut iface as *mut Interface,
            &mut sockets as *mut SocketSet<'static>,
            &mut wifi_dev as *mut WifiDevice as *mut (),
        );

        zpico_smoltcp::set_poll_callback(zpico_platform_esp32::network::smoltcp_network_poll);
    }

    // Step 11: Open zenoh session via RMW layer
    esp_println::println!("");
    esp_println::println!("Connecting to zenoh router...");

    let rmw_config = RmwConfig {
        locator: config.zenoh_locator,
        mode: SessionMode::Client,
        domain_id: config.domain_id,
        node_name: "node",
        namespace: "",
    };

    let session = match ZenohRmw::open(&rmw_config) {
        Ok(s) => s,
        Err(e) => {
            esp_println::println!("ERROR: zenoh session open failed ({:?})", e);
            loop {
                core::hint::spin_loop();
            }
        }
    };

    esp_println::println!("Connected!");
    esp_println::println!("");

    // Step 12: Create node and run user application
    let mut node = Node {
        session,
        domain_id: config.domain_id,
    };

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
