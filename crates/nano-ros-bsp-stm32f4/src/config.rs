//! Configuration for STM32F4 BSP
//!
//! Provides sensible defaults for common development boards and allows
//! customization of network settings.

use crate::pins::PinConfig;

/// BSP configuration
#[derive(Clone)]
pub struct Config {
    /// MAC address (locally administered)
    pub mac: [u8; 6],
    /// Static IP address
    pub ip: [u8; 4],
    /// Network prefix length (e.g., 24 for /24)
    pub prefix: u8,
    /// Gateway IP address
    pub gateway: [u8; 4],
    /// Zenoh router locator (null-terminated C string)
    pub zenoh_locator: &'static [u8],
    /// External oscillator frequency in MHz (board-specific)
    pub hse_freq_mhz: u8,
    /// Pin configuration preset
    pub pins: PinConfig,
}

impl Config {
    /// Create configuration for NUCLEO-F429ZI board
    ///
    /// Default network settings:
    /// - IP: 192.168.1.10/24
    /// - Gateway: 192.168.1.1
    /// - Zenoh: tcp/192.168.1.1:7447
    pub fn nucleo_f429zi() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x01],
            ip: [192, 168, 1, 10],
            prefix: 24,
            gateway: [192, 168, 1, 1],
            zenoh_locator: b"tcp/192.168.1.1:7447\0",
            hse_freq_mhz: 8, // NUCLEO-F429ZI uses 8 MHz HSE
            pins: PinConfig::NucleoF429ZI,
        }
    }

    /// Create configuration for STM32F4-Discovery board (STM32F407)
    ///
    /// Note: The Discovery board doesn't have built-in Ethernet.
    /// This config assumes an external PHY is connected.
    pub fn discovery_f407() -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x02],
            ip: [192, 168, 1, 11],
            prefix: 24,
            gateway: [192, 168, 1, 1],
            zenoh_locator: b"tcp/192.168.1.1:7447\0",
            hse_freq_mhz: 8, // Discovery uses 8 MHz HSE
            pins: PinConfig::DiscoveryF407,
        }
    }

    /// Create a custom configuration
    pub fn custom(pins: PinConfig) -> Self {
        Self {
            mac: [0x02, 0x00, 0x00, 0x00, 0x00, 0x00],
            ip: [192, 168, 1, 10],
            prefix: 24,
            gateway: [192, 168, 1, 1],
            zenoh_locator: b"tcp/192.168.1.1:7447\0",
            hse_freq_mhz: 8,
            pins,
        }
    }

    /// Set MAC address
    pub fn mac(mut self, mac: [u8; 6]) -> Self {
        self.mac = mac;
        self
    }

    /// Set IP address
    pub fn ip(mut self, ip: [u8; 4]) -> Self {
        self.ip = ip;
        self
    }

    /// Set network prefix length
    pub fn prefix(mut self, prefix: u8) -> Self {
        self.prefix = prefix;
        self
    }

    /// Set gateway address
    pub fn gateway(mut self, gateway: [u8; 4]) -> Self {
        self.gateway = gateway;
        self
    }

    /// Set zenoh router locator
    ///
    /// Must be a null-terminated byte string, e.g., `b"tcp/192.168.1.1:7447\0"`
    pub fn zenoh_locator(mut self, locator: &'static [u8]) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Set external oscillator frequency in MHz
    pub fn hse_freq_mhz(mut self, freq: u8) -> Self {
        self.hse_freq_mhz = freq;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::nucleo_f429zi()
    }
}
