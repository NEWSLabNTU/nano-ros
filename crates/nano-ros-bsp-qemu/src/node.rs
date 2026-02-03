//! Simplified node API for QEMU bare-metal

use core::ffi::{c_char, c_void};
use core::marker::PhantomData;
use core::ptr;

use cortex_m_semihosting::hprintln;
use lan9118_smoltcp::{Config as EthConfig, Lan9118, MPS2_AN385_BASE};
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::phy::Device;
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
use crate::error::{Error, Result};
use crate::exit_failure;
use crate::publisher::Publisher;
use crate::subscriber::Subscriber;

/// Trait for Ethernet devices that can be used with Node
pub trait EthernetDevice: Device {
    /// Get the MAC address
    fn mac_address(&self) -> [u8; 6];
}

// Implement EthernetDevice for Lan9118
impl EthernetDevice for Lan9118 {
    fn mac_address(&self) -> [u8; 6] {
        Lan9118::mac_address(self)
    }
}

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

        let eth = &mut *(GLOBAL_DEVICE as *mut Lan9118);
        let iface = &mut *GLOBAL_IFACE;
        let sockets = &mut *GLOBAL_SOCKETS;

        SmoltcpZenohBridge::poll(iface, eth, sockets);
        clock::advance_clock_ms(1);
    }
}

/// Internal node state (manages bare-metal resources)
struct InnerNode<'a, D: EthernetDevice> {
    _marker: PhantomData<&'a mut D>,
}

impl<'a, D: EthernetDevice + 'static> InnerNode<'a, D> {
    /// Create a new inner node
    fn new(
        eth: &'a mut D,
        iface: &'a mut Interface,
        sockets: &'a mut SocketSet<'static>,
        ip: [u8; 4],
        gateway: [u8; 4],
        prefix: u8,
        zenoh_locator: &[u8],
    ) -> Result<Self> {
        // Configure IP address
        let ip_addr = Ipv4Address::new(ip[0], ip[1], ip[2], ip[3]);
        iface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(IpAddress::Ipv4(ip_addr), prefix))
                .ok();
        });

        // Set default gateway
        let gw = Ipv4Address::new(gateway[0], gateway[1], gateway[2], gateway[3]);
        iface
            .routes_mut()
            .add_default_ipv4_route(gw)
            .map_err(|_| Error::Route)?;

        // Initialize the zenoh-pico bridge
        SmoltcpZenohBridge::init();

        // Seed RNG with IP to avoid zenoh ID collisions
        let ip_seed = u32::from_be_bytes(ip);
        smoltcp_seed_random(ip_seed);

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
            GLOBAL_DEVICE = eth as *mut D as *mut ();
            GLOBAL_IFACE = iface as *mut Interface;
            GLOBAL_SOCKETS = sockets as *mut SocketSet<'static>;

            smoltcp_set_poll_callback(Some(smoltcp_network_poll));
        }

        // Initialize zenoh session
        let ret = unsafe { zenoh_shim_init(zenoh_locator.as_ptr() as *const c_char) };
        if ret < 0 {
            return Err(Error::ZenohInit);
        }

        let ret = unsafe { zenoh_shim_open() };
        if ret < 0 {
            return Err(Error::ZenohOpen);
        }

        // Verify session is open
        if unsafe { zenoh_shim_is_open() } == 0 {
            return Err(Error::ZenohNotOpen);
        }

        Ok(Self {
            _marker: PhantomData,
        })
    }

    /// Create a publisher
    fn create_publisher(&mut self, topic: &[u8]) -> Result<Publisher> {
        let handle = unsafe { zenoh_shim_declare_publisher(topic.as_ptr() as *const c_char) };
        if handle < 0 {
            return Err(Error::PublisherDeclare);
        }
        Ok(unsafe { Publisher::from_handle(handle) })
    }

    /// Create a subscriber
    unsafe fn create_subscriber_raw(
        &mut self,
        topic: &[u8],
        callback: Option<ShimCallback>,
        context: *mut c_void,
    ) -> Result<Subscriber> {
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

    /// Spin once
    fn spin_once(&mut self, timeout_ms: u32) {
        unsafe {
            zenoh_shim_spin_once(timeout_ms);
        }
    }
}

impl<'a, D: EthernetDevice> Drop for InnerNode<'a, D> {
    fn drop(&mut self) {
        unsafe {
            zenoh_shim_close();

            GLOBAL_IFACE = ptr::null_mut();
            GLOBAL_SOCKETS = ptr::null_mut();
            GLOBAL_DEVICE = ptr::null_mut();
        }
    }
}

// ============================================================================
// Public API
// ============================================================================

/// Simplified node for QEMU bare-metal applications
///
/// This hides all low-level smoltcp details.
/// Users interact only with ROS concepts (publishers, subscribers).
pub struct Node<'a> {
    inner: InnerNode<'a, Lan9118>,
}

impl<'a> Node<'a> {
    /// Create a publisher for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (null-terminated, e.g., `b"demo/topic\0"`)
    ///
    /// # Example
    ///
    /// ```ignore
    /// let publisher = node.create_publisher(b"chatter\0")?;
    /// publisher.publish(b"Hello!")?;
    /// ```
    pub fn create_publisher(&mut self, topic: &[u8]) -> Result<Publisher> {
        self.inner.create_publisher(topic)
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
    ) -> Result<Subscriber> {
        unsafe { self.inner.create_subscriber_raw(topic, callback, context) }
    }

    /// Process network events and dispatch callbacks
    ///
    /// Call this periodically to handle:
    /// - Network traffic (TCP/IP)
    /// - Zenoh protocol messages
    /// - Subscriber callbacks
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Max wait time (0 = non-blocking)
    pub fn spin_once(&mut self, timeout_ms: u32) {
        self.inner.spin_once(timeout_ms);
    }

    /// Shutdown the node gracefully
    pub fn shutdown(self) {
        drop(self.inner);
    }
}

/// Helper to create an smoltcp interface from an Ethernet device
fn create_interface<D: EthernetDevice>(eth: &mut D) -> Interface {
    let mac = eth.mac_address();
    let mac_addr = EthernetAddress::from_bytes(&mac);
    let now = clock::now();
    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    Interface::new(iface_config, eth, now)
}

/// Helper to create a socket set with pre-allocated storage
#[allow(static_mut_refs)]
unsafe fn create_socket_set() -> SocketSet<'static> {
    unsafe { SocketSet::new(&mut buffers::SOCKET_STORAGE[..]) }
}

/// Create LAN9118 Ethernet driver for QEMU MPS2-AN385
fn create_ethernet(mac: [u8; 6]) -> Result<Lan9118> {
    let config = EthConfig {
        base_addr: MPS2_AN385_BASE,
        mac_addr: mac,
    };

    let mut eth = unsafe { Lan9118::new(config).map_err(|_| Error::EthernetInit)? };
    eth.init().map_err(|_| Error::EthernetInit)?;

    Ok(eth)
}

/// Run a node with the given configuration
///
/// This is the main entry point for QEMU bare-metal applications.
/// It handles all hardware and network initialization, then calls
/// your application code with a ready-to-use `Node`.
///
/// # Arguments
///
/// * `config` - Network and node configuration (use `Config::default()`)
/// * `f` - Application function that receives the initialized node
///
/// # Returns
///
/// Never returns (`-> !`). Calls `exit_success()` on Ok, `exit_failure()` on Err.
///
/// # Example
///
/// ```ignore
/// #![no_std]
/// #![no_main]
///
/// use nano_ros_bsp_qemu::prelude::*;
///
/// #[entry]
/// fn main() -> ! {
///     run_node(Config::default(), |node| {
///         let pub_ = node.create_publisher(b"demo/topic\0")?;
///
///         for _ in 0..10 {
///             node.spin_once(10);
///             pub_.publish(b"Hello!")?;
///         }
///
///         Ok(())
///     })
/// }
/// ```
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> Result<()>,
{
    hprintln!("");
    hprintln!("========================================");
    hprintln!("  nano-ros QEMU BSP");
    hprintln!("========================================");
    hprintln!("");

    // Initialize Ethernet driver
    hprintln!("Initializing LAN9118 Ethernet...");
    let mut eth = match create_ethernet(config.mac) {
        Ok(e) => e,
        Err(e) => {
            hprintln!("Error creating Ethernet: {:?}", e);
            exit_failure();
        }
    };

    hprintln!(
        "  MAC: {:02x}:{:02x}:{:02x}:{:02x}:{:02x}:{:02x}",
        config.mac[0],
        config.mac[1],
        config.mac[2],
        config.mac[3],
        config.mac[4],
        config.mac[5]
    );

    // Create smoltcp interface and socket set
    hprintln!("");
    hprintln!("Creating network interface...");
    let mut iface = create_interface(&mut eth);
    let mut sockets = unsafe { create_socket_set() };

    hprintln!(
        "  IP: {}.{}.{}.{}",
        config.ip[0],
        config.ip[1],
        config.ip[2],
        config.ip[3]
    );

    // Create inner node
    hprintln!("");
    hprintln!("Connecting to zenoh router...");

    let inner = match InnerNode::new(
        &mut eth,
        &mut iface,
        &mut sockets,
        config.ip,
        config.gateway,
        config.prefix,
        config.zenoh_locator,
    ) {
        Ok(n) => n,
        Err(e) => {
            hprintln!("Error creating node: {:?}", e);
            exit_failure();
        }
    };

    hprintln!("Connected!");
    hprintln!("");

    // Create wrapper node
    let mut node = Node { inner };

    // Run user application
    match f(&mut node) {
        Ok(()) => {
            hprintln!("");
            hprintln!("Application completed successfully.");
            node.shutdown();
            hprintln!("");
            hprintln!("========================================");
            hprintln!("  Done");
            hprintln!("========================================");
            crate::exit_success();
        }
        Err(e) => {
            hprintln!("");
            hprintln!("Application error: {:?}", e);
            node.shutdown();
            exit_failure();
        }
    }
}
