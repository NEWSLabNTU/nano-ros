//! Configuration for QEMU bare-metal nodes

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
}
