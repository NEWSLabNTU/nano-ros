//! Configuration for ESP32-S3 nodes (serial transport).

/// Node configuration for ESP32-S3.
///
/// Serial-only: the S3 has no QEMU NIC, so the link layer is zenoh-pico's
/// built-in serial. `baudrate` feeds the serial locator string.
#[derive(Clone)]
pub struct Config {
    /// Serial baud rate (default 115200).
    pub baudrate: u32,
    /// Zenoh locator (e.g. `serial/UART_0#baudrate=115200`).
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (default 0).
    pub domain_id: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            baudrate: 115200,
            zenoh_locator: "serial/UART_0#baudrate=115200",
            domain_id: 0,
        }
    }
}

impl Config {
    /// Builder: set baud rate.
    pub fn with_baudrate(mut self, baudrate: u32) -> Self {
        self.baudrate = baudrate;
        self
    }

    /// Builder: set the transport locator the RMW backend connects
    /// through (e.g. `"tcp/192.0.3.1:7447"`,
    /// `"serial/UART_0#baudrate=115200"`).
    pub fn with_locator(mut self, locator: &'static str) -> Self {
        self.zenoh_locator = locator;
        self
    }

    /// Deprecated alias for [`with_locator`](Self::with_locator).
    #[deprecated(
        since = "0.6.0",
        note = "renamed to `with_locator()` — the config API must not name a backend (issue 0330)"
    )]
    pub fn with_zenoh_locator(self, locator: &'static str) -> Self {
        self.with_locator(locator)
    }

    /// Builder: set ROS 2 domain ID.
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }
}

impl nros_platform::BoardConfig for Config {
    fn locator(&self) -> &str {
        self.zenoh_locator
    }

    fn domain_id(&self) -> u32 {
        self.domain_id
    }
}

// Phase 173.5 — the orchestration generator writes the nros.toml serial
// baudrate here (NanoRosOwned). No ethernet field, so `set_ipv4` keeps
// the trait's no-op default.
impl nros_platform::BoardTransportConfig for Config {
    fn set_baudrate(&mut self, baud: u32) {
        self.baudrate = baud;
    }
}
