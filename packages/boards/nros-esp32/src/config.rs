//! Configuration for ESP32 WiFi nodes
//!
//! # WiFi Configuration
//!
//! Requires WiFi credentials (SSID and password) to connect.
//! These can be provided at compile time via environment variables
//! or hardcoded in source.
//!
//! # IP Configuration Modes
//!
//! Two IP modes are supported:
//!
//! 1. **DHCP** (default) - Automatically acquire IP from router
//! 2. **Static IP** - Use a manually configured IP address

/// WiFi credentials
#[derive(Clone)]
pub struct WifiConfig {
    /// WiFi network name (SSID)
    pub ssid: &'static str,
    /// WiFi password
    pub password: &'static str,
}

impl WifiConfig {
    /// Create a new WiFi configuration
    pub fn new(ssid: &'static str, password: &'static str) -> Self {
        Self { ssid, password }
    }
}

/// IP address assignment mode
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
/// Combines WiFi credentials with network settings.
///
/// # Default Configuration
///
/// - IP mode: DHCP
/// - Zenoh locator: tcp/192.168.1.1:7447 (typical home router gateway)
///
/// # Example
///
/// ```ignore
/// let config = NodeConfig::new(WifiConfig::new("MyNetwork", "password123"))
///     .with_zenoh_locator("tcp/10.0.0.1:7447")
///     .with_static_ip([10, 0, 0, 100], 24, [10, 0, 0, 1]);
/// ```
#[derive(Clone)]
pub struct NodeConfig {
    /// WiFi credentials
    pub wifi: WifiConfig,
    /// IP address mode
    pub ip_mode: IpMode,
    /// Zenoh router locator (Rust string, null termination handled internally)
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (used in keyexpr formatting)
    pub domain_id: u32,
}

impl NodeConfig {
    /// Create a new node configuration with DHCP and default zenoh locator
    pub fn new(wifi: WifiConfig) -> Self {
        Self {
            wifi,
            ip_mode: IpMode::Dhcp,
            zenoh_locator: "tcp/192.168.1.1:7447",
            domain_id: 0,
        }
    }

    /// Builder: set zenoh router locator
    pub fn with_zenoh_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Builder: use static IP instead of DHCP
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
}
