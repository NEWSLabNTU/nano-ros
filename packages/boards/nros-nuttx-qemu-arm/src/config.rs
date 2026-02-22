//! Configuration for NuttX QEMU ARM virt nodes
//!
//! Provides sensible defaults for the QEMU virt machine with TAP networking.
//! Same IP scheme as bare-metal QEMU board crates (192.0.3.0/24 on br-qemu).

/// Network and node configuration for QEMU ARM virt.
///
/// # Default Configuration (Talker)
///
/// - IP: 192.0.3.10/24
/// - Gateway: 192.0.3.1
/// - Zenoh: tcp/192.0.3.1:7447
///
/// # Listener Configuration
///
/// - IP: 192.0.3.11/24
/// - Gateway: 192.0.3.1
/// - Zenoh: tcp/192.0.3.1:7447
#[derive(Clone, Debug)]
pub struct Config {
    /// IP address
    pub ip: [u8; 4],
    /// Network prefix length (default: 24)
    pub prefix: u8,
    /// Gateway IP
    pub gateway: [u8; 4],
    /// Zenoh locator
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (default: 0)
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            ip: [192, 0, 3, 10],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }
}

impl Config {
    /// Configuration preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            ip: [192, 0, 3, 11],
            ..Self::default()
        }
    }

    /// Alias for `Config::default()`.
    pub fn talker() -> Self {
        Self::default()
    }

    /// Configuration preset for a service/action server node.
    pub fn server() -> Self {
        Self {
            ip: [192, 0, 3, 12],
            ..Self::default()
        }
    }

    /// Configuration preset for a service/action client node.
    pub fn client() -> Self {
        Self {
            ip: [192, 0, 3, 13],
            ..Self::default()
        }
    }

    /// Builder: set IP address.
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Builder: set network prefix length.
    pub fn with_prefix(mut self, prefix: u8) -> Self {
        self.prefix = prefix;
        self
    }

    /// Builder: set gateway.
    pub fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Builder: set zenoh locator.
    pub fn with_zenoh_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: set ROS 2 domain ID.
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}
