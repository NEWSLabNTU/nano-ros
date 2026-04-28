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

impl NodeConfig {
    /// Parse configuration from a TOML string.
    ///
    /// Missing fields use board-specific defaults. This is designed to work
    /// with `include_str!("../config.toml")` for compile-time embedding.
    ///
    /// Note: WiFi credentials (SSID/password) should not be stored in
    /// config.toml (they may contain secrets). Use builder methods or
    /// environment variables for WiFi credentials.
    ///
    /// # Supported fields
    ///
    /// ```toml
    /// [wifi]
    /// ssid = "MyNetwork"
    /// password = "secret"
    ///
    /// [network]
    /// ip = "10.0.0.100"
    /// gateway = "10.0.0.1"
    /// prefix = 24
    ///
    /// [serial]
    /// baudrate = 115200
    ///
    /// [zenoh]
    /// locator = "tcp/10.0.0.1:7447"
    /// domain_id = 0
    /// ```
    pub fn from_toml(toml: &'static str) -> Self {
        let mut config = Self::default();
        let mut section = "";

        for line in toml.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    section = line[1..end].trim();
                }
                continue;
            }
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].trim();
                let value = line[eq_pos + 1..].trim();
                let value = if (value.starts_with('"') && value.ends_with('"'))
                    || (value.starts_with('\'') && value.ends_with('\''))
                {
                    &value[1..value.len() - 1]
                } else {
                    value
                };

                match (section, key) {
                    #[cfg(feature = "wifi")]
                    ("wifi", "ssid") => {
                        config.wifi.ssid = value;
                    }
                    #[cfg(feature = "wifi")]
                    ("wifi", "password") => {
                        config.wifi.password = value;
                    }
                    #[cfg(feature = "wifi")]
                    ("network", "ip") => {
                        if let Some(ip) = parse_ipv4(value) {
                            // Parse prefix from existing static config or default to 24
                            let (existing_prefix, existing_gateway) = match &config.ip_mode {
                                IpMode::Static {
                                    prefix, gateway, ..
                                } => (*prefix, *gateway),
                                IpMode::Dhcp => (24, [0, 0, 0, 0]),
                            };
                            config.ip_mode = IpMode::Static {
                                ip,
                                prefix: existing_prefix,
                                gateway: existing_gateway,
                            };
                        }
                    }
                    #[cfg(feature = "wifi")]
                    ("network", "gateway") => {
                        if let Some(gw) = parse_ipv4(value) {
                            match &config.ip_mode {
                                IpMode::Static { ip, prefix, .. } => {
                                    config.ip_mode = IpMode::Static {
                                        ip: *ip,
                                        prefix: *prefix,
                                        gateway: gw,
                                    };
                                }
                                IpMode::Dhcp => {
                                    config.ip_mode = IpMode::Static {
                                        ip: [0, 0, 0, 0],
                                        prefix: 24,
                                        gateway: gw,
                                    };
                                }
                            }
                        }
                    }
                    #[cfg(feature = "wifi")]
                    ("network", "prefix") => {
                        if let Some(p) = parse_u32(value) {
                            match &config.ip_mode {
                                IpMode::Static { ip, gateway, .. } => {
                                    config.ip_mode = IpMode::Static {
                                        ip: *ip,
                                        prefix: p as u8,
                                        gateway: *gateway,
                                    };
                                }
                                IpMode::Dhcp => {
                                    config.ip_mode = IpMode::Static {
                                        ip: [0, 0, 0, 0],
                                        prefix: p as u8,
                                        gateway: [0, 0, 0, 0],
                                    };
                                }
                            }
                        }
                    }
                    #[cfg(feature = "serial")]
                    ("serial", "baudrate") => {
                        if let Some(b) = parse_u32(value) {
                            config.baudrate = b;
                        }
                    }
                    ("zenoh", "locator") => {
                        config.zenoh_locator = value;
                    }
                    ("zenoh", "domain_id") => {
                        if let Some(d) = parse_u32(value) {
                            config.domain_id = d;
                        }
                    }
                    _ => {}
                }
            }
        }

        config
    }
}

// ── Minimal no_std parsers ──────────────────────────────────────────────

/// Parse an IPv4 address string ("192.0.3.10") into [u8; 4].
fn parse_ipv4(s: &str) -> Option<[u8; 4]> {
    let mut result = [0u8; 4];
    let mut octet_idx = 0;
    let mut current: u16 = 0;
    let mut has_digit = false;

    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' => {
                current = current * 10 + (*b - b'0') as u16;
                if current > 255 {
                    return None;
                }
                has_digit = true;
            }
            b'.' => {
                if !has_digit || octet_idx >= 3 {
                    return None;
                }
                result[octet_idx] = current as u8;
                octet_idx += 1;
                current = 0;
                has_digit = false;
            }
            _ => return None,
        }
    }

    if has_digit && octet_idx == 3 {
        result[3] = current as u8;
        Some(result)
    } else {
        None
    }
}

/// Parse a decimal integer string.
fn parse_u32(s: &str) -> Option<u32> {
    let mut result: u32 = 0;
    let mut has_digit = false;
    for b in s.as_bytes() {
        match b {
            b'0'..=b'9' => {
                result = result.checked_mul(10)?.checked_add((*b - b'0') as u32)?;
                has_digit = true;
            }
            _ => return None,
        }
    }
    if has_digit { Some(result) } else { None }
}

impl nros_platform::BoardConfig for NodeConfig {
    fn zenoh_locator(&self) -> &str {
        self.zenoh_locator
    }

    fn domain_id(&self) -> u32 {
        self.domain_id
    }
}
