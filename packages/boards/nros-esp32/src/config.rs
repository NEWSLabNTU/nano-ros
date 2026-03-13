//! Configuration for ESP32 nodes
//!
//! # Transport Features
//!
//! - `wifi` (default) — WiFi via esp-radio + smoltcp
//! - `serial` — UART via zenoh-pico built-in ESP-IDF serial
//!
//! # WiFi Configuration
//!
//! Requires WiFi credentials (SSID and password) to connect.
//! These can be provided at compile time via environment variables
//! or hardcoded in source.
//!
//! # IP Configuration Modes (WiFi only)
//!
//! Two IP modes are supported:
//!
//! 1. **DHCP** (default) - Automatically acquire IP from router
//! 2. **Static IP** - Use a manually configured IP address

/// WiFi credentials
#[cfg(feature = "wifi")]
#[derive(Clone)]
pub struct WifiConfig {
    /// WiFi network name (SSID)
    pub ssid: &'static str,
    /// WiFi password
    pub password: &'static str,
}

#[cfg(feature = "wifi")]
impl WifiConfig {
    /// Create a new WiFi configuration
    pub fn new(ssid: &'static str, password: &'static str) -> Self {
        Self { ssid, password }
    }
}

/// IP address assignment mode
#[cfg(feature = "wifi")]
#[derive(Clone)]
pub enum IpMode {
    /// Acquire IP via DHCP (default)
    Dhcp,
    /// Use a static IP configuration
    Static {
        /// IPv4 address
        ip: [u8; 4],
        /// Network prefix length (e.g., 24 for /24)
        prefix: u8,
        /// Gateway IPv4 address
        gateway: [u8; 4],
    },
}

/// Node and network configuration
///
/// Combines transport settings with zenoh connection parameters.
///
/// # WiFi Example
///
/// ```ignore
/// let config = NodeConfig::new(WifiConfig::new("MyNetwork", "password123"))
///     .with_zenoh_locator("tcp/10.0.0.1:7447")
///     .with_static_ip([10, 0, 0, 100], 24, [10, 0, 0, 1]);
/// ```
///
/// # Serial Example
///
/// ```ignore
/// let config = NodeConfig::serial_default()
///     .with_baudrate(921600);
/// ```
#[derive(Clone)]
pub struct NodeConfig {
    // -- WiFi-specific fields --
    /// WiFi credentials
    #[cfg(feature = "wifi")]
    pub wifi: WifiConfig,
    /// IP address mode
    #[cfg(feature = "wifi")]
    pub ip_mode: IpMode,

    // -- Serial-specific fields --
    /// Baud rate (default: 115200)
    #[cfg(feature = "serial")]
    pub baudrate: u32,

    // -- Common fields --
    /// Zenoh router locator (Rust string, null termination handled internally)
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (used in keyexpr formatting)
    pub domain_id: u32,
}

impl NodeConfig {
    /// Create a new node configuration with WiFi, DHCP, and default zenoh locator
    #[cfg(feature = "wifi")]
    pub fn new(wifi: WifiConfig) -> Self {
        Self {
            wifi,
            ip_mode: IpMode::Dhcp,
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.168.1.1:7447",
            domain_id: 0,
        }
    }

    /// Configuration preset for serial transport with default settings.
    ///
    /// Uses 115200 baud with a serial zenoh locator.
    #[cfg(feature = "serial")]
    pub fn serial_default() -> Self {
        Self {
            #[cfg(feature = "wifi")]
            wifi: WifiConfig::new("", ""),
            #[cfg(feature = "wifi")]
            ip_mode: IpMode::Dhcp,
            baudrate: 115200,
            zenoh_locator: "serial/UART_0#baudrate=115200",
            domain_id: 0,
        }
    }

    /// Builder: set zenoh router locator
    pub fn with_zenoh_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: use static IP instead of DHCP
    #[cfg(feature = "wifi")]
    pub fn with_static_ip(mut self, ip: [u8; 4], prefix: u8, gateway: [u8; 4]) -> Self {
        self.ip_mode = IpMode::Static {
            ip,
            prefix,
            gateway,
        };
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

#[cfg(feature = "wifi")]
impl Default for NodeConfig {
    fn default() -> Self {
        Self::new(WifiConfig::new("", ""))
    }
}

#[cfg(all(feature = "serial", not(feature = "wifi")))]
impl Default for NodeConfig {
    fn default() -> Self {
        Self::serial_default()
    }
}
