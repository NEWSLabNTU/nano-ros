//! Simplified node API for ESP32-C3 QEMU bare-metal
//!
//! Uses `nros-rmw-zenoh` for transport and `zpico-smoltcp` for socket management.

use esp_hal::rng::Rng;
use nros_core::RosMessage;
use nros_rmw::{QosSettings, Rmw, RmwConfig, Session, SessionMode, TopicInfo};
use nros_rmw_zenoh::ZenohRmw;
use nros_rmw_zenoh::shim::ShimSession;
use zpico_smoltcp::SmoltcpBridge;
use openeth_smoltcp::OpenEth;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use zpico_platform_esp32_qemu::clock;
use crate::config::Config;
use crate::error::Error;
use crate::publisher::Publisher;
use zpico_platform_esp32_qemu::random;
use crate::subscriber::Subscription;

// NOTE: We intentionally do NOT import `type Result<T>` in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

/// Simplified node for ESP32-C3 QEMU applications
///
/// This hides all low-level OpenETH/smoltcp details.
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
            type_hash: "TypeHashNotSupported",
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
            type_hash: "TypeHashNotSupported",
            domain_id: self.domain_id,
        };
        let subscriber = self.session.create_subscriber(&topic_info, QosSettings::default())?;
        Ok(Subscription::new(subscriber))
    }

    /// Process network events and dispatch callbacks
    ///
    /// Call this periodically to handle:
    /// - Ethernet traffic
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
            zpico_platform_esp32_qemu::network::clear_network_state();
        }
    }
}

/// Helper to create a socket set with pre-allocated storage
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
}

/// Run a node with the given configuration
///
/// This is the main entry point for ESP32-C3 QEMU applications.
/// It handles all OpenETH, network, and zenoh initialization, then calls
/// your application code with a ready-to-use `Node`.
///
/// # Returns
///
/// Never returns (`-> !`). Loops forever after the application function completes
/// or on error.
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> core::result::Result<(), Error>,
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

    // Create and register TCP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(&mut sockets);
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

    // Step 8: Open zenoh session via RMW layer
    // Retry z_open -- on consecutive QEMU runs the TAP/bridge may need
    // time to resettle, causing the first TCP connect attempt to fail.
    esp_println::println!("");
    esp_println::println!("Connecting to zenoh router...");

    let rmw_config = RmwConfig {
        locator: config.zenoh_locator,
        mode: SessionMode::Client,
        domain_id: config.domain_id,
        node_name: "node",
        namespace: "",
    };

    const MAX_OPEN_RETRIES: u32 = 5;
    let mut session_opt: Option<ShimSession> = None;

    for attempt in 1..=MAX_OPEN_RETRIES {
        match ZenohRmw::open(&rmw_config) {
            Ok(s) => {
                session_opt = Some(s);
                break;
            }
            Err(e) => {
                esp_println::println!(
                    "  zenoh open attempt {}/{} failed ({:?}), retrying...",
                    attempt,
                    MAX_OPEN_RETRIES,
                    e
                );
                // Poll network stack and delay ~1s before retrying
                for _ in 0..100 {
                    unsafe {
                        zpico_platform_esp32_qemu::network::smoltcp_network_poll();
                    }
                    for _ in 0..250_000 {
                        core::hint::spin_loop();
                    }
                }
            }
        }
    }

    let session = match session_opt {
        Some(s) => s,
        None => {
            esp_println::println!(
                "ERROR: zenoh open failed after {} attempts",
                MAX_OPEN_RETRIES
            );
            loop {
                core::hint::spin_loop();
            }
        }
    };

    esp_println::println!("Connected!");
    esp_println::println!("");

    // Step 9: Create node and run user application
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
