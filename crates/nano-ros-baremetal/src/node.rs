//! BaremetalNode - High-level API for bare-metal ROS nodes

use core::ffi::c_void;
use core::marker::PhantomData;
use core::ptr;

use smoltcp::iface::{Interface, SocketSet};
use smoltcp::phy::Device;
use smoltcp::socket::tcp::{Socket as TcpSocket, SocketBuffer as TcpSocketBuffer};
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};

use crate::buffers;
use crate::config::NodeConfig;
use crate::error::{Error, Result};
use crate::publisher::Publisher;
use crate::subscriber::Subscriber;

// FFI declarations for zenoh-pico shim
extern "C" {
    fn zenoh_shim_init(locator: *const i8) -> i32;
    fn zenoh_shim_open() -> i32;
    fn zenoh_shim_close() -> i32;
    fn zenoh_shim_is_open() -> i32;
    fn zenoh_shim_declare_publisher(keyexpr: *const i8) -> i32;
    fn zenoh_shim_declare_subscriber(
        keyexpr: *const i8,
        callback: Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>,
        context: *mut c_void,
    ) -> i32;
    fn zenoh_shim_spin_once(timeout_ms: u32) -> i32;
}

// FFI for smoltcp bridge
extern "C" {
    fn smoltcp_seed_random(seed: u32);
    fn smoltcp_register_socket(handle: usize) -> i32;
    fn smoltcp_set_poll_callback(cb: Option<unsafe extern "C" fn()>);
}

/// Trait for Ethernet devices that can be used with BaremetalNode
pub trait EthernetDevice: Device {
    /// Get the MAC address
    fn mac_address(&self) -> [u8; 6];
}

// Global state for poll callback (required for zenoh-pico integration)
// These are raw pointers to user-owned data
static mut GLOBAL_IFACE: *mut Interface = ptr::null_mut();
static mut GLOBAL_SOCKETS: *mut SocketSet<'static> = ptr::null_mut();
static mut GLOBAL_DEVICE: *mut () = ptr::null_mut();
static mut GLOBAL_POLL_FN: Option<fn(*mut (), *mut Interface, *mut SocketSet<'static>)> = None;

/// Network poll callback called by zenoh-pico
#[no_mangle]
pub unsafe extern "C" fn smoltcp_network_poll() {
    if GLOBAL_IFACE.is_null() || GLOBAL_SOCKETS.is_null() || GLOBAL_DEVICE.is_null() {
        return;
    }

    if let Some(poll_fn) = GLOBAL_POLL_FN {
        poll_fn(GLOBAL_DEVICE, GLOBAL_IFACE, GLOBAL_SOCKETS);
    }
}

/// High-level bare-metal ROS node
///
/// This struct manages the complete lifecycle of a bare-metal ROS node:
/// - Ethernet driver
/// - smoltcp network interface
/// - TCP socket management
/// - zenoh-pico session
///
/// All smoltcp and zenoh-pico details are hidden from the user.
///
/// # Lifetime
///
/// The node holds references to the Ethernet driver, interface, and sockets.
/// These must outlive the node.
pub struct BaremetalNode<'a, D: EthernetDevice> {
    _marker: PhantomData<&'a mut D>,
}

impl<'a, D: EthernetDevice + 'static> BaremetalNode<'a, D> {
    /// Create a new bare-metal node
    ///
    /// This initializes the network stack, connects to the zenoh router,
    /// and makes the node ready for publishing/subscribing.
    ///
    /// # Arguments
    ///
    /// * `eth` - Mutable reference to initialized Ethernet device
    /// * `iface` - Mutable reference to smoltcp Interface (will be configured)
    /// * `sockets` - Mutable reference to SocketSet
    /// * `config` - Network and zenoh configuration
    ///
    /// # Safety
    ///
    /// The eth, iface, and sockets references must remain valid for the
    /// lifetime of the node. Typically this means they are stack-allocated
    /// in main() which never returns on bare-metal.
    ///
    /// # Errors
    ///
    /// Returns an error if network or zenoh initialization fails.
    pub fn new(
        eth: &'a mut D,
        iface: &'a mut Interface,
        sockets: &'a mut SocketSet<'static>,
        config: NodeConfig,
    ) -> Result<Self> {
        // Configure IP address
        let ip = Ipv4Address::new(config.ip[0], config.ip[1], config.ip[2], config.ip[3]);
        iface.update_ip_addrs(|addrs| {
            addrs
                .push(IpCidr::new(IpAddress::Ipv4(ip), config.prefix))
                .ok();
        });

        // Set default gateway
        let gateway = Ipv4Address::new(
            config.gateway[0],
            config.gateway[1],
            config.gateway[2],
            config.gateway[3],
        );
        iface
            .routes_mut()
            .add_default_ipv4_route(gateway)
            .map_err(|_| Error::Route)?;

        // Initialize the zenoh-pico bridge
        #[cfg(feature = "qemu-mps2")]
        qemu_rs_common::SmoltcpZenohBridge::init();

        // Seed RNG with IP to avoid zenoh ID collisions
        let ip_seed = u32::from_be_bytes(config.ip);
        unsafe { smoltcp_seed_random(ip_seed) };

        // Create and register TCP sockets
        for i in 0..2 {
            let (rx_buf, tx_buf) = unsafe { buffers::get_tcp_buffers(i) };
            let tcp = TcpSocket::new(
                TcpSocketBuffer::new(rx_buf),
                TcpSocketBuffer::new(tx_buf),
            );
            let handle = sockets.add(tcp);

            unsafe {
                smoltcp_register_socket(core::mem::transmute::<
                    smoltcp::iface::SocketHandle,
                    usize,
                >(handle));
            }
        }

        // Store global state for poll callback
        unsafe {
            GLOBAL_DEVICE = eth as *mut D as *mut ();
            GLOBAL_IFACE = iface as *mut Interface;
            GLOBAL_SOCKETS = sockets as *mut SocketSet<'static>;

            // Set platform-specific poll function
            #[cfg(feature = "qemu-mps2")]
            {
                GLOBAL_POLL_FN = Some(poll_qemu_mps2::<D>);
            }

            smoltcp_set_poll_callback(Some(smoltcp_network_poll));
        }

        // Initialize zenoh session
        let ret = unsafe { zenoh_shim_init(config.zenoh_locator.as_ptr() as *const i8) };
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

    /// Create a publisher for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (null-terminated, e.g., b"demo/topic\0")
    ///
    /// # Errors
    ///
    /// Returns `Error::PublisherDeclare` if publisher creation fails.
    pub fn create_publisher(&mut self, topic: &[u8]) -> Result<Publisher> {
        let handle = unsafe { zenoh_shim_declare_publisher(topic.as_ptr() as *const i8) };
        if handle < 0 {
            return Err(Error::PublisherDeclare);
        }
        Ok(unsafe { Publisher::from_handle(handle) })
    }

    /// Create a subscriber for the given topic
    ///
    /// # Arguments
    ///
    /// * `topic` - Topic name (null-terminated, e.g., b"demo/topic\0")
    /// * `callback` - Callback function invoked when messages arrive
    /// * `context` - Context pointer passed to callback
    ///
    /// # Safety
    ///
    /// The callback and context must remain valid for the lifetime of the subscriber.
    ///
    /// # Errors
    ///
    /// Returns `Error::SubscriberDeclare` if subscriber creation fails.
    pub unsafe fn create_subscriber_raw(
        &mut self,
        topic: &[u8],
        callback: Option<unsafe extern "C" fn(*const u8, usize, *mut c_void)>,
        context: *mut c_void,
    ) -> Result<Subscriber> {
        let handle = zenoh_shim_declare_subscriber(topic.as_ptr() as *const i8, callback, context);
        if handle < 0 {
            return Err(Error::SubscriberDeclare);
        }
        Ok(Subscriber::from_handle(handle))
    }

    /// Process network events and zenoh callbacks
    ///
    /// Must be called periodically to handle network traffic and
    /// dispatch subscriber callbacks.
    ///
    /// # Arguments
    ///
    /// * `timeout_ms` - Maximum time to wait for events (0 = non-blocking)
    pub fn spin_once(&mut self, timeout_ms: u32) {
        unsafe {
            zenoh_shim_spin_once(timeout_ms);
        }
    }

    /// Shutdown the node and release resources
    pub fn shutdown(self) {
        // Drop runs cleanup
    }
}

impl<'a, D: EthernetDevice> Drop for BaremetalNode<'a, D> {
    fn drop(&mut self) {
        unsafe {
            zenoh_shim_close();

            // Clear global pointers
            GLOBAL_IFACE = ptr::null_mut();
            GLOBAL_SOCKETS = ptr::null_mut();
            GLOBAL_DEVICE = ptr::null_mut();
            GLOBAL_POLL_FN = None;
        }
    }
}

/// Helper to create an smoltcp interface from an Ethernet device
///
/// # Arguments
///
/// * `eth` - Mutable reference to Ethernet device
///
/// # Returns
///
/// Configured smoltcp Interface
pub fn create_interface<D: EthernetDevice>(eth: &mut D) -> Interface {
    let mac = eth.mac_address();
    let mac_addr = EthernetAddress::from_bytes(&mac);

    #[cfg(feature = "qemu-mps2")]
    let now = qemu_rs_common::clock::now();

    #[cfg(not(feature = "qemu-mps2"))]
    let now = smoltcp::time::Instant::from_millis(0);

    let iface_config = smoltcp::iface::Config::new(mac_addr.into());
    Interface::new(iface_config, eth, now)
}

/// Helper to create a socket set with pre-allocated storage
///
/// # Safety
///
/// This function should only be called once, as it uses static buffers.
#[allow(static_mut_refs)]
pub unsafe fn create_socket_set() -> SocketSet<'static> {
    SocketSet::new(&mut buffers::SOCKET_STORAGE[..])
}

// Platform-specific poll functions
#[cfg(feature = "qemu-mps2")]
fn poll_qemu_mps2<D: EthernetDevice>(
    device: *mut (),
    iface: *mut Interface,
    sockets: *mut SocketSet<'static>,
) {
    use qemu_rs_common::{clock, SmoltcpZenohBridge};

    unsafe {
        let eth = &mut *(device as *mut D);
        let iface = &mut *iface;
        let sockets = &mut *sockets;

        SmoltcpZenohBridge::poll(iface, eth, sockets);
        clock::advance_clock_ms(1);
    }
}
