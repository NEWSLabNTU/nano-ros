//! Configuration for STM32F4 platform
//!
//! Provides sensible defaults for common development boards and allows
//! customization of network settings.

#[cfg(feature = "ethernet")]
use nros_platform_stm32f4::pins::PinConfig;

/// Platform configuration
#[derive(Clone)]
pub struct Config {
    // -- Ethernet-specific fields --
    /// MAC address (locally administered)
    #[cfg(feature = "ethernet")]
    pub mac: [u8; 6],
    /// Static IP address
    #[cfg(feature = "ethernet")]
    pub ip: [u8; 4],
    /// Network prefix length (e.g., 24 for /24)
    #[cfg(feature = "ethernet")]
    pub prefix: u8,
    /// Gateway IP address
    #[cfg(feature = "ethernet")]
    pub gateway: [u8; 4],
    /// Pin configuration preset
    #[cfg(feature = "ethernet")]
    pub pins: PinConfig,

    // -- Serial-specific fields --
    /// USART peripheral index (1-based, e.g., 1 = USART1, 2 = USART2)
    #[cfg(feature = "serial")]
    pub usart_index: u8,
    /// Baud rate (default: 115200)
    #[cfg(feature = "serial")]
    pub baudrate: u32,

    // -- Common fields --
    /// Zenoh router locator (Rust string, null termination handled internally)
    pub zenoh_locator: &'static str,
    /// External oscillator frequency in MHz (board-specific)
    pub hse_freq_mhz: u8,
    /// ROS 2 domain ID (default: 0)
    pub domain_id: u32,
}

impl Config {
    /// Create configuration for NUCLEO-F429ZI board
    ///
    /// Default network settings:
    /// - IP: 192.168.1.10/24
    /// - Gateway: 192.168.1.1
    /// - Zenoh: tcp/192.168.1.1:7447
    #[cfg(feature = "ethernet")]
    pub fn nucleo_f429zi() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [192, 168, 1, 10],
            prefix: 24,
            gateway: [192, 168, 1, 1],
            pins: PinConfig::NucleoF429ZI,
            #[cfg(feature = "serial")]
            usart_index: 3,
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.168.1.1:7447",
            hse_freq_mhz: 8,
            domain_id: 0,
        }
    }

    /// Create configuration for STM32F4-Discovery board (STM32F407)
    ///
    /// Note: The Discovery board doesn't have built-in Ethernet.
    /// This config assumes an external PHY is connected.
    #[cfg(feature = "ethernet")]
    pub fn discovery_f407() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
            ip: [192, 168, 1, 11],
            prefix: 24,
            gateway: [192, 168, 1, 1],
            pins: PinConfig::DiscoveryF407,
            #[cfg(feature = "serial")]
            usart_index: 2,
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.168.1.1:7447",
            hse_freq_mhz: 8,
            domain_id: 0,
        }
    }

    /// Create a custom configuration with ethernet
    #[cfg(feature = "ethernet")]
    pub fn custom(pins: PinConfig) -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [192, 168, 1, 10],
            prefix: 24,
            gateway: [192, 168, 1, 1],
            pins,
            #[cfg(feature = "serial")]
            usart_index: 3,
            #[cfg(feature = "serial")]
            baudrate: 115200,
            zenoh_locator: "tcp/192.168.1.1:7447",
            hse_freq_mhz: 8,
            domain_id: 0,
        }
    }

    /// Configuration preset for serial transport with default settings.
    ///
    /// Uses USART3 at 115200 baud with a serial zenoh locator.
    #[cfg(feature = "serial")]
    pub fn serial_default() -> Self {
        Self {
            #[cfg(feature = "ethernet")]
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            #[cfg(feature = "ethernet")]
            ip: [192, 168, 1, 10],
            #[cfg(feature = "ethernet")]
            prefix: 24,
            #[cfg(feature = "ethernet")]
            gateway: [192, 168, 1, 1],
            #[cfg(feature = "ethernet")]
            pins: PinConfig::NucleoF429ZI,
            usart_index: 3,
            baudrate: 115200,
            zenoh_locator: "serial/UART_0#baudrate=115200",
            hse_freq_mhz: 8,
            domain_id: 0,
        }
    }

    /// Set MAC address
    #[cfg(feature = "ethernet")]
    pub fn mac(mut self, mac: [u8; 6]) -> Self {
        self.mac = mac;
        self
    }

    /// Set IP address
    #[cfg(feature = "ethernet")]
    pub fn ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Set network prefix length
    #[cfg(feature = "ethernet")]
    pub fn prefix(mut self, prefix: u8) -> Self {
        self.prefix = prefix;
        self
    }

    /// Set gateway address
    #[cfg(feature = "ethernet")]
    pub fn gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Set zenoh router locator
    ///
    /// e.g., `"tcp/192.168.1.1:7447"` or `"serial/UART_0#baudrate=115200"`
    pub fn zenoh_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Set external oscillator frequency in MHz
    pub fn hse_freq_mhz(mut self, freq: u8) -> Self {
        self.hse_freq_mhz = freq;
        self
    }

    /// Set ROS 2 domain ID
    pub fn domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Set USART peripheral index (1-based)
    #[cfg(feature = "serial")]
    pub fn usart_index(mut self, index: u8) -> Self {
        self.usart_index = index;
        self
    }

    /// Set baud rate
    #[cfg(feature = "serial")]
    pub fn baudrate(mut self, baudrate: u32) -> Self {
        self.baudrate = baudrate;
        self
    }
}

#[cfg(feature = "ethernet")]
impl Default for Config {
    fn default() -> Self {
        Self::nucleo_f429zi()
    }
}

#[cfg(all(feature = "serial", not(feature = "ethernet")))]
impl Default for Config {
    fn default() -> Self {
        Self::serial_default()
    }
}

impl Config {
    /// Parse configuration from a TOML string.
    ///
    /// Missing fields use board-specific defaults. This is designed to work
    /// with `include_str!("../config.toml")` for compile-time embedding.
    ///
    /// Note: The `pins` and `hse_freq_mhz` fields cannot be set from TOML
    /// (they require Rust enum values). Use builder methods for those.
    ///
    /// # Supported fields
    ///
    /// ```toml
    /// [network]
    /// ip = "192.168.1.10"
    /// mac = "02:00:00:00:00:01"
    /// gateway = "192.168.1.1"
    /// prefix = 24
    ///
    /// [serial]
    /// baudrate = 115200
    /// usart_index = 3
    ///
    /// [zenoh]
    /// locator = "tcp/192.168.1.1:7447"
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
                            config.mac = mac;
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
                    #[cfg(feature = "serial")]
                    ("serial", "usart_index") => {
                        if let Some(u) = parse_u32(value) {
                            config.usart_index = u as u8;
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
