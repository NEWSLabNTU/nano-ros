//! Simplified node API for ESP32-C3 QEMU bare-metal
//!
//! Uses `zpico-smoltcp` for socket management instead of
//! the legacy BSP bridge.

use core::ffi::{c_char, c_void};
use core::ptr;

use core::fmt::Write as _;

use esp_hal::rng::Rng;
use heapless::String;
use nros_core::RosMessage;
use zpico_smoltcp::SmoltcpBridge;
use openeth_smoltcp::OpenEth;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use zpico_sys::{
    zenoh_shim_close, zenoh_shim_declare_publisher, zenoh_shim_declare_subscriber,
    zenoh_shim_init, zenoh_shim_is_open, zenoh_shim_open, zenoh_shim_spin_once,
};

use crate::clock;
use crate::config::Config;
use crate::error::Error;
use crate::publisher::Publisher;
use crate::random;
use crate::subscriber::{Subscription, subscription_trampoline};

// NOTE: We intentionally do NOT import `type Result<T>` in this module.
// The `esp_println::println!` macro uses `?` internally which expands to
// `Result<(), core::fmt::Error>`. A `type Result<T>` alias here would shadow
// `core::result::Result` and cause "expected 1 generic argument but 2 supplied" errors.

// Global state for poll callback
static mut GLOBAL_IFACE: *mut Interface = ptr::null_mut();
static mut GLOBAL_SOCKETS: *mut SocketSet<'static> = ptr::null_mut();
static mut GLOBAL_DEVICE: *mut () = ptr::null_mut();

/// Network poll callback called by the transport crate's smoltcp_poll()
#[unsafe(no_mangle)]
pub unsafe extern "C" fn smoltcp_network_poll() {
    unsafe {
        if GLOBAL_IFACE.is_null() || GLOBAL_SOCKETS.is_null() || GLOBAL_DEVICE.is_null() {
            return;
        }

        let device = &mut *(GLOBAL_DEVICE as *mut OpenEth);
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        SmoltcpBridge::poll(iface, device, sockets);
    }
}

/// Simplified node for ESP32-C3 QEMU applications
///
/// This hides all low-level OpenETH/smoltcp details.
/// Users interact only with ROS concepts (publishers, subscriptions).
pub struct Node {
    domain_id: u32,
}

impl Node {
    /// Create a typed publisher for a ROS 2 topic
    ///
    /// Constructs the ROS 2 keyexpr from topic name and `M::TYPE_NAME`:
    /// `<domain_id>/<topic>/<type_name>/TypeHashNotSupported`
    pub fn create_publisher<M: RosMessage>(
        &mut self,
        topic: &str,
    ) -> core::result::Result<Publisher<M>, Error> {
        let mut key = format_ros2_keyexpr(self.domain_id, topic, M::TYPE_NAME);
        key.push('\0').map_err(|_| Error::TopicTooLong)?;
        let handle = unsafe { zenoh_shim_declare_publisher(key.as_bytes().as_ptr() as *const c_char) };
        if handle < 0 {
            return Err(Error::PublisherDeclare);
        }
        Ok(unsafe { Publisher::from_handle(handle) })
    }

    /// Create a typed subscription for a ROS 2 topic
    ///
    /// Messages are deserialized from CDR and delivered to the callback.
    ///
    /// # Limitations
    ///
    /// The callback is a function pointer (`fn(&M)`), not a closure.
    /// Use `static` variables for external state — the standard bare-metal pattern.
    pub fn create_subscription<M: RosMessage>(
        &mut self,
        topic: &str,
        callback: fn(&M),
    ) -> core::result::Result<Subscription<M>, Error> {
        let mut key = format_ros2_keyexpr_wildcard(self.domain_id, topic, M::TYPE_NAME);
        key.push('\0').map_err(|_| Error::TopicTooLong)?;
        let ctx = callback as *mut c_void;
        let handle = unsafe {
            zenoh_shim_declare_subscriber(
                key.as_bytes().as_ptr() as *const c_char,
                subscription_trampoline::<M>,
                ctx,
            )
        };
        if handle < 0 {
            return Err(Error::SubscriberDeclare);
        }
        Ok(unsafe { Subscription::from_handle(handle) })
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

/// Format a ROS 2 data keyexpr: `<domain_id>/<topic>/<type_name>/TypeHashNotSupported`
fn format_ros2_keyexpr(domain_id: u32, topic: &str, type_name: &str) -> String<256> {
    let mut key = String::<256>::new();
    let topic_stripped = topic.trim_matches('/');
    let _ = write!(
        key,
        "{}/{}/{}/TypeHashNotSupported",
        domain_id, topic_stripped, type_name
    );
    key
}

/// Format a ROS 2 subscriber keyexpr with wildcard: `<domain_id>/<topic>/<type_name>/*`
fn format_ros2_keyexpr_wildcard(domain_id: u32, topic: &str, type_name: &str) -> String<256> {
    let mut key = String::<256>::new();
    let topic_stripped = topic.trim_matches('/');
    let _ = write!(key, "{}/{}/{}/*", domain_id, topic_stripped, type_name);
    key
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
        GLOBAL_DEVICE = &mut eth as *mut OpenEth as *mut ();
        GLOBAL_IFACE = &mut iface as *mut Interface;
        GLOBAL_SOCKETS = &mut sockets as *mut SocketSet<'static>;

        zpico_smoltcp::set_poll_callback(smoltcp_network_poll);
    }

    // Step 8: Initialize zenoh session
    esp_println::println!("");
    esp_println::println!("Connecting to zenoh router...");

    let ret = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const c_char) };
    if ret < 0 {
        esp_println::println!("ERROR: zenoh init failed ({})", ret);
        loop {
            core::hint::spin_loop();
        }
    }

    // Retry z_open — on consecutive QEMU runs the TAP/bridge may need
    // time to resettle, causing the first TCP connect attempt to fail.
    const MAX_OPEN_RETRIES: u32 = 5;
    let mut connected = false;
    for attempt in 1..=MAX_OPEN_RETRIES {
        let ret = unsafe { zenoh_shim_open() };
        if ret >= 0 && unsafe { zenoh_shim_is_open() } != 0 {
            connected = true;
            break;
        }
        esp_println::println!(
            "  zenoh open attempt {}/{} failed ({}), retrying...",
            attempt,
            MAX_OPEN_RETRIES,
            ret
        );
        // Poll network stack and delay ~1s before retrying
        for _ in 0..100 {
            unsafe { smoltcp_network_poll() };
            for _ in 0..250_000 {
                core::hint::spin_loop();
            }
        }
        // Re-init zenoh config for next attempt
        let _ = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const c_char) };
    }

    if !connected {
        esp_println::println!("ERROR: zenoh open failed after {} attempts", MAX_OPEN_RETRIES);
        loop {
            core::hint::spin_loop();
        }
    }

    esp_println::println!("Connected!");
    esp_println::println!("");

    // Step 9: Create node and run user application
    let mut node = Node {
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
