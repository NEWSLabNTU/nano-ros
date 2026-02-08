//! Configuration for ESP32-C3 QEMU nodes
//!
//! Unlike the WiFi BSP, the QEMU BSP uses static IP (no DHCP needed)
//! and OpenETH instead of WiFi.

/// Node and network configuration for QEMU
///
/// # Default Configuration
///
/// - MAC: 02:00:00:00:00:01
/// - IP: 192.0.3.10/24
/// - Gateway: 192.0.3.1
/// - Zenoh locator: tcp/192.0.3.1:7448
///
/// # Example
///
/// ```ignore
/// let config = Config::default()
///     .with_zenoh_locator(b"tcp/10.0.0.1:7448\0");
/// ```
#[derive(Clone)]
pub struct Config {
    /// MAC address (6 bytes)
    pub mac_addr: [u8; 6],
    /// IPv4 address
    pub ip: [u8; 4],
    /// Network prefix length (e.g., 24 for /24)
    pub prefix: u8,
    /// Gateway IPv4 address
    pub gateway: [u8; 4],
    /// Zenoh router locator (null-terminated)
    pub zenoh_locator: &'static [u8],
    /// ROS 2 domain ID (used in keyexpr: `<domain_id>/<topic>/<type>/...`)
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [192, 0, 3, 10],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            zenoh_locator: b"tcp/192.0.3.1:7448\0",
            domain_id: 0,
        }
    }
}

impl Config {
    /// Configuration for a listener instance (different IP/MAC from default talker)
    pub fn listener() -> Self {
        Self {
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
            ip: [192, 0, 3, 11],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            zenoh_locator: b"tcp/192.0.3.1:7448\0",
            domain_id: 0,
        }
    }

    /// Builder: set zenoh router locator (must be null-terminated)
    pub fn with_zenoh_locator(mut self, locator: &'static [u8]) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: set MAC address
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self {
        self.mac_addr = mac;
        self
    }

    /// Builder: set IP address
    pub fn with_ip(mut self, ip: [u8; 4], prefix: u8) -> Self {
        self.ip = ip;
        self.prefix = prefix;
        self
    }

    /// Builder: set gateway
    pub fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Builder: set ROS 2 domain ID
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}
