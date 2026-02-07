//! Simplified node API for ESP32-C3 QEMU bare-metal
//!
//! Handles OpenETH initialization, smoltcp interface, and zenoh-pico session
//! setup, exposing only ROS concepts to the user.

use core::ffi::{c_char, c_void};
use core::ptr;

use esp_hal::rng::Rng;
use openeth_smoltcp::OpenEth;
use smoltcp::iface::{Interface, SocketSet};
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
use crate::config::Config;
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

        let device = &mut *(GLOBAL_DEVICE as *mut OpenEth);
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        SmoltcpZenohBridge::poll(iface, device, sockets);
    }
}

/// Simplified node for ESP32-C3 QEMU applications
///
/// This hides all low-level OpenETH/smoltcp details.
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

/// Helper to create a socket set with pre-allocated storage
#[allow(static_mut_refs)]
unsafe fn create_socket_set() -> SocketSet<'static> {
    unsafe { SocketSet::new(&mut buffers::SOCKET_STORAGE[..]) }
}

/// Run a node with the given configuration
///
/// This is the main entry point for ESP32-C3 QEMU applications.
/// It handles all OpenETH, network, and zenoh initialization, then calls
/// your application code with a ready-to-use `Node`.
///
/// # Arguments
///
/// * `config` - Network configuration (IP, MAC, gateway, zenoh locator)
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
/// use nano_ros_bsp_esp32_qemu::prelude::*;
///
/// #[entry]
/// fn main() -> ! {
///     run_node(
///         Config::default(),
///         |node| {
///             let pub_ = node.create_publisher(b"demo/esp32\0")?;
///             for _ in 0..10 {
///                 node.spin_once(10);
///                 pub_.publish(b"Hello from QEMU ESP32!")?;
///             }
///             Ok(())
///         },
///     )
/// }
/// ```
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> core::result::Result<(), Error>,
{
    esp_println::println!("");
    esp_println::println!("========================================");
    esp_println::println!("  nano-ros ESP32-C3 QEMU BSP");
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

    // Step 7: Initialize zenoh-pico bridge
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
        GLOBAL_DEVICE = &mut eth as *mut OpenEth as *mut ();
        GLOBAL_IFACE = &mut iface as *mut Interface;
        GLOBAL_SOCKETS = &mut sockets as *mut SocketSet<'static>;

        smoltcp_set_poll_callback(Some(smoltcp_network_poll));
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

    // Step 9: Create node and run user application
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
