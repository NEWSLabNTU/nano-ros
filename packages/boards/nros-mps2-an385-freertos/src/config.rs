//! Configuration for QEMU FreeRTOS nodes
//!
//! Same IP presets as the bare-metal board crate (`nros-mps2-an385`),
//! designed for the TAP bridge topology used by `just test-freertos`.

/// Network and node configuration for QEMU MPS2-AN385 + FreeRTOS.
///
/// # Default (Talker)
///
/// - IP: 192.0.3.10/24, Gateway: 192.0.3.1
/// - Zenoh: `tcp/192.0.3.1:7447`
#[derive(Clone)]
pub struct Config {
    /// MAC address (default: locally administered 02:00:00:00:00:00)
    pub mac: [u8; 6],
    /// IP address
    pub ip: [u8; 4],
    /// Network mask
    pub netmask: [u8; 4],
    /// Gateway IP
    pub gateway: [u8; 4],
    /// Zenoh locator string
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (default: 0)
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [192, 0, 3, 10],
            netmask: [255, 255, 255, 0],
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }
}

impl Config {
    /// Preset for a listener/subscriber node.
    pub fn listener() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [192, 0, 3, 11],
            netmask: [255, 255, 255, 0],
            gateway: [192, 0, 3, 1],
            zenoh_locator: "tcp/192.0.3.1:7447",
            domain_id: 0,
        }
    }

    /// Alias for `Config::default()`.
    pub fn talker() -> Self {
        Self::default()
    }

    /// Builder: set MAC address.
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self {
        self.mac = mac;
        self
    }

    /// Builder: set IP address.
    pub fn with_ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Builder: set network mask.
    pub fn with_netmask(mut self, netmask: [u8; 4]) -> Self {
        self.netmask = netmask;
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
