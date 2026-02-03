//! Simplified node API for QEMU bare-metal

use core::ffi::c_void;

use cortex_m_semihosting::hprintln;
use lan9118_smoltcp::Lan9118;

use nano_ros_baremetal::platform::qemu_mps2;
use nano_ros_baremetal::{BaremetalNode, NodeConfig, create_interface, create_socket_set};

use crate::config::Config;
use crate::{Publisher, Result, ShimCallback, Subscriber, exit_failure};

/// Simplified node for QEMU bare-metal applications
///
/// This wraps `BaremetalNode` and hides the low-level smoltcp details.
/// Users interact only with ROS concepts (publishers, subscribers).
pub struct Node<'a> {
    inner: BaremetalNode<'a, Lan9118>,
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
    ///
    /// # Example
    ///
    /// ```ignore
    /// unsafe extern "C" fn on_message(data: *const u8, len: usize, ctx: *mut c_void) {
    ///     // Handle message
    /// }
    ///
    /// let sub = unsafe {
    ///     node.create_subscriber(b"chatter\0", Some(on_message), core::ptr::null_mut())
    /// }?;
    /// ```
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
        self.inner.shutdown();
    }
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

    // Create bare-metal node
    hprintln!("");
    hprintln!("Connecting to zenoh router...");
    let node_config = NodeConfig {
        ip: config.ip,
        gateway: config.gateway,
        prefix: config.prefix,
        zenoh_locator: config.zenoh_locator,
    };

    let inner = match BaremetalNode::new(&mut eth, &mut iface, &mut sockets, node_config) {
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

/// Create LAN9118 Ethernet driver for QEMU MPS2-AN385
fn create_ethernet(mac: [u8; 6]) -> Result<Lan9118> {
    qemu_mps2::create_ethernet(mac)
}
