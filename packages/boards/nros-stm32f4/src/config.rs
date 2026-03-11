//! Configuration for STM32F4 platform
//!
//! Provides sensible defaults for common development boards and allows
//! customization of network settings.

#[cfg(feature = "ethernet")]
use zpico_platform_stm32f4::pins::PinConfig;

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
