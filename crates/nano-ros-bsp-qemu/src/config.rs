//! Configuration for QEMU bare-metal nodes
//!
//! # IP Configuration Modes
//!
//! The BSP supports several IP configuration modes:
//!
//! 1. **Static IP** (default) - Use `Config::default()` or `Config::listener()`
//! 2. **Link-local auto-config** - Use `Config::link_local()` for zero-config networking
//! 3. **Custom** - Use builder methods to configure any settings
//!
//! ## Link-Local Auto-Configuration
//!
//! Link-local addresses (169.254.x.y) are automatically generated from the MAC
//! address, following RFC 3927. This allows nodes to communicate without manual
//! IP configuration, useful for development and testing.
//!
//! ```ignore
//! // Auto-generate link-local IP from MAC
//! let config = Config::link_local();
//!
//! // Or with a specific MAC address
//! let config = Config::link_local_with_mac([0x02, 0x00, 0x00, 0x12, 0x34, 0x56]);
//! ```

/// Network and node configuration
///
/// Provides sensible defaults for QEMU MPS2-AN385 development.
///
/// # Default Configuration (Talker)
///
/// - **TAP mode** (default): Connects directly to host TAP interface
///   - IP: 192.0.2.10/24
///   - Gateway: 192.0.2.1
///   - Zenoh: tcp/192.0.2.1:7447
///
/// - **Docker mode** (`docker` feature): Container with NAT networking
///   - IP: 192.168.100.10/24
///   - Gateway: 192.168.100.1
///   - Zenoh: tcp/172.20.0.2:7447
///
/// # Listener Configuration
///
/// Use `Config::listener()` for a second node on the same network:
/// - TAP mode: IP 192.0.2.11
/// - Docker mode: IP 192.168.100.11
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
            }
        }

        #[cfg(not(feature = "docker"))]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
                ip: [192, 0, 2, 10],
                prefix: 24,
                gateway: [192, 0, 2, 1],
                zenoh_locator: b"tcp/192.0.2.1:7447\0",
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
        }
    }

    /// Configuration preset for a listener/subscriber node
    ///
    /// Uses a different IP and MAC address than the default to avoid
    /// conflicts when running multiple nodes on the same network.
    ///
    /// - TAP mode: IP 192.0.2.11, MAC 02:00:00:00:00:01
    /// - Docker mode: IP 192.168.100.11, MAC 02:00:00:00:00:01
    pub fn listener() -> Self {
        #[cfg(feature = "docker")]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                ip: [192, 168, 100, 11],
                prefix: 24,
                gateway: [192, 168, 100, 1],
                zenoh_locator: b"tcp/172.20.0.2:7447\0",
            }
        }

        #[cfg(not(feature = "docker"))]
        {
            Self {
                mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
                ip: [192, 0, 2, 11],
                prefix: 24,
                gateway: [192, 0, 2, 1],
                zenoh_locator: b"tcp/192.0.2.1:7447\0",
            }
        }
    }

    /// Alias for `Config::default()` - configuration for a talker/publisher node
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

    // =========================================================================
    // Link-Local Auto-Configuration
    // =========================================================================

    /// Create a link-local configuration with auto-generated IP
    ///
    /// Generates a link-local IP address (169.254.x.y) from a default MAC address.
    /// Useful for zero-config development and testing scenarios.
    ///
    /// # Network Setup
    ///
    /// - IP: 169.254.x.y (derived from MAC)
    /// - Prefix: /16 (link-local subnet)
    /// - Gateway: None (link-local is local-only)
    /// - Zenoh: Uses default locator for the mode (TAP or Docker)
    ///
    /// # Note
    ///
    /// Link-local addresses only work within the local network segment.
    /// The zenoh router must be reachable on the same segment.
    pub fn link_local() -> Self {
        Self::link_local_with_mac([0x02, 0x00, 0x00, 0x00, 0x00, 0x00])
    }

    /// Create a link-local configuration for a second node (listener)
    ///
    /// Uses a different MAC address to generate a unique link-local IP.
    pub fn link_local_listener() -> Self {
        Self::link_local_with_mac([0x02, 0x00, 0x00, 0x00, 0x00, 0x01])
    }

    /// Create a link-local configuration with a specific MAC address
    ///
    /// The IP address is generated from the MAC address following RFC 3927:
    /// - First octet: 169
    /// - Second octet: 254
    /// - Third octet: derived from MAC[4] (avoiding 0 and 255)
    /// - Fourth octet: derived from MAC[5] (avoiding 0 and 255)
    pub fn link_local_with_mac(mac: [u8; 6]) -> Self {
        let ip = Self::mac_to_link_local(&mac);

        // Link-local has no gateway (local segment only)
        // Use 0.0.0.0 as a placeholder (smoltcp will not route to it)
        let gateway = [0, 0, 0, 0];

        // Use default zenoh locator for the mode
        #[cfg(feature = "docker")]
        let zenoh_locator: &'static [u8] = b"tcp/172.20.0.2:7447\0";

        #[cfg(not(feature = "docker"))]
        let zenoh_locator: &'static [u8] = b"tcp/192.0.2.1:7447\0";

        Self {
            mac,
            ip,
            prefix: 16, // Link-local uses /16
            gateway,
            zenoh_locator,
        }
    }

    /// Generate a link-local IP address from a MAC address
    ///
    /// Following RFC 3927, link-local addresses are in 169.254.0.0/16.
    /// The last two octets are derived from the MAC address, avoiding
    /// the reserved ranges 169.254.0.x and 169.254.255.x.
    fn mac_to_link_local(mac: &[u8; 6]) -> [u8; 4] {
        // Use MAC bytes 4 and 5 for the last two IP octets
        // Ensure we avoid 0 and 255 (reserved ranges)
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
