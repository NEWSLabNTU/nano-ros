//! Configuration for QEMU bare-metal nodes
//!
//! # IP Configuration Modes
//!
//! The platform crate supports several IP configuration modes:
//!
//! 1. **Static IP** (default) - Use `Config::default()` or `Config::listener()`
//! 2. **Link-local auto-config** - Use `Config::link_local()` for zero-config networking
//! 3. **Custom** - Use builder methods to configure any settings

/// Network and node configuration
///
/// Provides sensible defaults for QEMU MPS2-AN385 development.
///
/// # Default Configuration (Talker)
///
/// - **TAP mode** (default): Connects directly to host TAP interface
///   - IP: 192.0.3.10/24
///   - Gateway: 192.0.3.1
///   - Zenoh: tcp/192.0.3.1:7447
///
/// - **Docker mode** (`docker` feature): Container with NAT networking
///   - IP: 192.168.100.10/24
///   - Gateway: 192.168.100.1
///   - Zenoh: tcp/172.20.0.2:7447
#[derive(Clone)]
pub struct Config {
    /// MAC address (default: locally administered 02:00:00:00:00:00)
    pub mac: [u8; 6],
    /// IP address
    pub ip: [u8; 4],
    /// Network prefix (default: 24)
    pub prefix: u8,
    /// Gateway IP
    pub gateway: [u8; 4],
    /// Zenoh locator (null-terminated)
    pub zenoh_locator: &'static [u8],
    /// ROS 2 domain ID (default: 0)
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        #[cfg(feature = "docker")]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
                ip: [192, 168, 100, 10],
                prefix: 24,
                gateway: [192, 168, 100, 1],
                zenoh_locator: b"tcp/172.20.0.2:7447\0",
                domain_id: 0,
            }
        }

        #[cfg(not(feature = "docker"))]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
                ip: [192, 0, 3, 10],
                prefix: 24,
                gateway: [192, 0, 3, 1],
                zenoh_locator: b"tcp/192.0.3.1:7447\0",
                domain_id: 0,
            }
        }
    }
}

impl Config {
    /// Create a new config with custom settings
    pub fn new(mac: [u8; 6], ip: [u8; 4], gateway: [u8; 4], zenoh_locator: &'static [u8]) -> Self {
        Self {
            mac,
            ip,
            prefix: 24,
            gateway,
            zenoh_locator,
            domain_id: 0,
        }
    }

    /// Configuration preset for a listener/subscriber node
    pub fn listener() -> Self {
        #[cfg(feature = "docker")]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                ip: [192, 168, 100, 11],
                prefix: 24,
                gateway: [192, 168, 100, 1],
                zenoh_locator: b"tcp/172.20.0.2:7447\0",
                domain_id: 0,
            }
        }

        #[cfg(not(feature = "docker"))]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                ip: [192, 0, 3, 11],
                prefix: 24,
                gateway: [192, 0, 3, 1],
                zenoh_locator: b"tcp/192.0.3.1:7447\0",
                domain_id: 0,
            }
        }
    }

    /// Alias for `Config::default()`
    pub fn talker() -> Self {
        Self::default()
    }

    /// Builder: set MAC address
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self {
        self.mac = mac;
        self
    }

    /// Builder: set IP address
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Builder: set network prefix length
    pub fn with_prefix(mut self, prefix: u8) -> Self {
        self.prefix = prefix;
        self
    }

    /// Builder: set gateway
    pub fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Builder: set zenoh locator
    pub fn with_zenoh_locator(mut self, locator: &'static [u8]) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: set ROS 2 domain ID
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Create a link-local configuration with auto-generated IP
    pub fn link_local() -> Self {
        Self::link_local_with_mac([0x02, 0x00, 0x00, 0x00, 0x00, 0x00])
    }

    /// Create a link-local configuration for a second node (listener)
    pub fn link_local_listener() -> Self {
        Self::link_local_with_mac([0x02, 0x00, 0x00, 0x00, 0x00, 0x01])
    }

    /// Create a link-local configuration with a specific MAC address
    pub fn link_local_with_mac(mac: [u8; 6]) -> Self {
        let ip = Self::mac_to_link_local(&mac);
        let gateway = [0, 0, 0, 0];

        #[cfg(feature = "docker")]
        let zenoh_locator: &'static [u8] = b"tcp/172.20.0.2:7447\0";

        #[cfg(not(feature = "docker"))]
        let zenoh_locator: &'static [u8] = b"tcp/192.0.3.1:7447\0";

        Self {
            mac,
            ip,
            prefix: 16,
            gateway,
            zenoh_locator,
            domain_id: 0,
        }
    }

    /// Generate a link-local IP address from a MAC address (RFC 3927)
    fn mac_to_link_local(mac: &[u8; 6]) -> [u8; 4] {
        let third = match mac[4] {
            0 => 1,
            255 => 254,
            x => x,
        };
        let fourth = match mac[5] {
            0 => 1,
            255 => 254,
            x => x,
        };

        [169, 254, third, fourth]
    }

    /// Check if this configuration uses a link-local IP address
    pub fn is_link_local(&self) -> bool {
        self.ip[0] == 169 && self.ip[1] == 254
    }
}
