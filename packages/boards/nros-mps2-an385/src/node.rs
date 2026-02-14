//! Simplified node API for QEMU bare-metal
//!
//! Uses `nros-rmw-zenoh` for transport and `zpico-smoltcp` for socket management.

use cortex_m_semihosting::hprintln;
use lan9118_smoltcp::{Config as EthConfig, Lan9118, MPS2_AN385_BASE};
use nros_core::RosMessage;
use nros_rmw::{QosSettings, Rmw, RmwConfig, Session, SessionMode, TopicInfo};
use nros_rmw_zenoh::ZenohRmw;
use nros_rmw_zenoh::shim::ShimSession;
use smoltcp::iface::{Interface, SocketSet};
use smoltcp::phy::Device;
use smoltcp::wire::{EthernetAddress, IpAddress, IpCidr, Ipv4Address};
use zpico_smoltcp::SmoltcpBridge;

use zpico_platform_mps2_an385::{clock, network, random};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::exit_failure;
use crate::publisher::Publisher;
use crate::subscriber::Subscription;

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

// ============================================================================
// Public API
// ============================================================================

/// Simplified node for QEMU bare-metal applications
pub struct Node {
    session: ShimSession,
    domain_id: u32,
}

impl Node {
    /// Create a typed publisher for a ROS 2 topic
    pub fn create_publisher<M: RosMessage>(&mut self, topic: &str) -> Result<Publisher<M>> {
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
    ) -> Result<Subscription<M>> {
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
    pub fn spin_once(&mut self, timeout_ms: u32) {
        let _ = self.session.spin_once(timeout_ms);
    }

    /// Shutdown the node gracefully
    pub fn shutdown(self) {
        // ShimSession closes on drop
        drop(self.session);
        unsafe {
            network::clear_network_state();
        }
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
unsafe fn create_socket_set() -> SocketSet<'static> {
    let storage = unsafe { zpico_smoltcp::get_socket_storage() };
    SocketSet::new(&mut storage[..])
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

/// Initialize network stack (IP, gateway, smoltcp bridge, sockets)
fn init_network<D: EthernetDevice + 'static>(
    eth: &mut D,
    iface: &mut Interface,
    sockets: &mut SocketSet<'static>,
    config: &Config,
) -> Result<()> {
    // Configure IP address
    let ip_addr = Ipv4Address::new(config.ip[0], config.ip[1], config.ip[2], config.ip[3]);
    iface.update_ip_addrs(|addrs| {
        addrs
            .push(IpCidr::new(IpAddress::Ipv4(ip_addr), config.prefix))
            .ok();
    });

    // Set default gateway (skip if 0.0.0.0, which indicates link-local mode)
    if config.gateway != [0, 0, 0, 0] {
        let gw = Ipv4Address::new(
            config.gateway[0],
            config.gateway[1],
            config.gateway[2],
            config.gateway[3],
        );
        iface
            .routes_mut()
            .add_default_ipv4_route(gw)
            .map_err(|_| Error::Route)?;
    }

    // Initialize the transport crate's bridge
    SmoltcpBridge::init();

    // Seed RNG with IP to avoid zenoh ID collisions
    let ip_seed = u32::from_be_bytes(config.ip);
    random::seed(ip_seed);

    // Create and register TCP sockets via transport crate
    unsafe {
        zpico_smoltcp::create_and_register_sockets(sockets);
    }

    // Store global state for poll callback
    unsafe {
        network::set_network_state(
            iface as *mut Interface,
            sockets as *mut SocketSet<'static>,
            eth as *mut D as *mut (),
        );

        zpico_smoltcp::set_poll_callback(network::smoltcp_network_poll);
    }

    Ok(())
}

/// Run a node with the given configuration
///
/// This is the main entry point for QEMU bare-metal applications.
/// It handles all hardware and network initialization, then calls
/// your application code with a ready-to-use `Node`.
///
/// # Returns
///
/// Never returns (`-> !`). Calls `exit_success()` on Ok, `exit_failure()` on Err.
pub fn run_node<F>(config: Config, f: F) -> !
where
    F: FnOnce(&mut Node) -> Result<()>,
{
    // Enable DWT cycle counter for timing measurements
    zpico_platform_mps2_an385::timing::CycleCounter::enable();

    hprintln!("");
    hprintln!("========================================");
    hprintln!("  nros QEMU Platform");
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

    // Initialize network stack
    if let Err(e) = init_network(&mut eth, &mut iface, &mut sockets, &config) {
        hprintln!("Error initializing network: {:?}", e);
        exit_failure();
    }

    // Open zenoh session via RMW layer
    hprintln!("");
    hprintln!("Connecting to zenoh router...");

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
            hprintln!("Error opening session: {:?}", e);
            exit_failure();
        }
    };

    hprintln!("Connected!");
    hprintln!("");

    // Create wrapper node
    let mut node = Node {
        session,
        domain_id: config.domain_id,
    };

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
