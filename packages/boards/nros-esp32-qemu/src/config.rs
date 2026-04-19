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

    /// Parse configuration from a TOML string.
    ///
    /// Missing fields use board-specific defaults. This is designed to work
    /// with `include_str!("../config.toml")` for compile-time embedding.
    ///
    /// # Supported fields
    ///
    /// ```toml
    /// [network]
    /// ip = "192.0.3.10"
    /// mac = "02:00:00:00:00:01"
    /// gateway = "192.0.3.1"
    /// prefix = 24
    ///
    /// [serial]
    /// baudrate = 115200
    ///
    /// [zenoh]
    /// locator = "tcp/192.0.3.1:7448"
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
                    #[cfg(feature = "ethernet")]
                    ("network", "ip") => {
                        if let Some(ip) = parse_ipv4(value) {
                            config.ip = ip;
                        }
                    }
                    #[cfg(feature = "ethernet")]
                    ("network", "mac") => {
                        if let Some(mac) = parse_mac(value) {
                            config.mac_addr = mac;
                        }
                    }
                    #[cfg(feature = "ethernet")]
                    ("network", "gateway") => {
                        if let Some(gw) = parse_ipv4(value) {
                            config.gateway = gw;
                        }
                    }
                    #[cfg(feature = "ethernet")]
                    ("network", "prefix") => {
                        if let Some(p) = parse_u32(value) {
                            config.prefix = p as u8;
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

/// Parse a MAC address string ("02:00:00:00:00:00") into [u8; 6].
fn parse_mac(s: &str) -> Option<[u8; 6]> {
    let mut result = [0u8; 6];
    let mut byte_idx = 0;

    for part in s.split(':') {
        if byte_idx >= 6 || part.len() != 2 {
            return None;
        }
        result[byte_idx] = parse_hex_byte(part)?;
        byte_idx += 1;
    }

    if byte_idx == 6 { Some(result) } else { None }
}

/// Parse a two-character hex string ("0a") into a u8.
fn parse_hex_byte(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    if bytes.len() != 2 {
        return None;
    }
    let hi = hex_digit(bytes[0])?;
    let lo = hex_digit(bytes[1])?;
    Some(hi * 16 + lo)
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
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

impl nros_platform::BoardConfig for Config {
    fn zenoh_locator(&self) -> &str {
        self.zenoh_locator
    }

    fn domain_id(&self) -> u32 {
        self.domain_id
    }
}
