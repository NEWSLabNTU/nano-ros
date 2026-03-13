//! Configuration for ESP32-C3 QEMU nodes
//!
//! # Transport Features
//!
//! - `ethernet` (default) — OpenETH + smoltcp TCP/IP stack
//! - `serial` — zenoh-pico built-in serial (no additional deps)
//!
//! At least one transport must be enabled.

/// Node and network configuration for QEMU
///
/// # Default Configuration (Ethernet)
///
/// - MAC: 02:00:00:00:00:01
/// - IP: 192.0.3.10/24
/// - Gateway: 192.0.3.1
/// - Zenoh locator: tcp/192.0.3.1:7448
///
/// # Default Configuration (Serial)
///
/// - Zenoh locator: serial/UART_0#baudrate=115200
/// - Baud rate: 115200
///
/// # Example
///
/// ```ignore
/// let config = Config::default()
///     .with_zenoh_locator("tcp/10.0.0.1:7448");
/// ```
#[derive(Clone)]
pub struct Config {
    // -- Ethernet-specific fields --
    /// MAC address (6 bytes)
    #[cfg(feature = "ethernet")]
    pub mac_addr: [u8; 6],
    /// IPv4 address
    #[cfg(feature = "ethernet")]
    pub ip: [u8; 4],
    /// Network prefix length (e.g., 24 for /24)
    #[cfg(feature = "ethernet")]
    pub prefix: u8,
    /// Gateway IPv4 address
    #[cfg(feature = "ethernet")]
    pub gateway: [u8; 4],

    // -- Serial-specific fields --
    /// Baud rate (default: 115200). Ignored by QEMU (infinite speed),
    /// but required for the zenoh locator string.
    #[cfg(feature = "serial")]
    pub baudrate: u32,

    // -- Common fields --
    /// Zenoh router locator (Rust string, null termination handled internally)
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (used in keyexpr: `<domain_id>/<topic>/<type>/...`)
    pub domain_id: u32,
}

#[cfg(feature = "ethernet")]
impl Default for Config {
    fn default() -> Self {
        Self {
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [192, 0, 3, 10],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.0.3.1:7448",
            domain_id: 0,
        }
    }
}

#[cfg(all(feature = "serial", not(feature = "ethernet")))]
impl Default for Config {
    fn default() -> Self {
        Self::serial_default()
    }
}

impl Config {
    /// Configuration preset for serial transport with default settings.
    ///
    /// Uses UART0 at 115200 baud with a serial zenoh locator.
    #[cfg(feature = "serial")]
    pub fn serial_default() -> Self {
        Self {
            #[cfg(feature = "ethernet")]
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            #[cfg(feature = "ethernet")]
            ip: [192, 0, 3, 10],
            #[cfg(feature = "ethernet")]
            prefix: 24,
            #[cfg(feature = "ethernet")]
            gateway: [192, 0, 3, 1],
            baudrate: 115200,
            zenoh_locator: "serial/UART_0#baudrate=115200",
            domain_id: 0,
        }
    }

    /// Configuration for a listener instance (different IP/MAC from default talker)
    #[cfg(feature = "ethernet")]
    pub fn listener() -> Self {
        Self {
            mac_addr: [0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
            ip: [192, 0, 3, 11],
            prefix: 24,
            gateway: [192, 0, 3, 1],
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.0.3.1:7448",
            domain_id: 0,
        }
    }

    /// Builder: set zenoh router locator
    pub fn with_zenoh_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: set MAC address
    #[cfg(feature = "ethernet")]
    pub fn with_mac(mut self, mac: [u8; 6]) -> Self {
        self.mac_addr = mac;
        self
    }

    /// Builder: set IP address
    #[cfg(feature = "ethernet")]
    pub fn with_ip(mut self, ip: [u8; 4], prefix: u8) -> Self {
        self.ip = ip;
        self.prefix = prefix;
        self
    }

    /// Builder: set gateway
    #[cfg(feature = "ethernet")]
    pub fn with_gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Builder: set ROS 2 domain ID
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Builder: set baud rate
    #[cfg(feature = "serial")]
    pub fn with_baudrate(mut self, baudrate: u32) -> Self {
        self.baudrate = baudrate;
        self
    }
}
