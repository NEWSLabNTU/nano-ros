//! Cross-board configuration trait.
//!
//! Every board crate (`nros-board-mps2-an385`, `nros-board-stm32f4`,
//! `nros-board-esp32-qemu`, …) declares its own `Config` / `NodeConfig`
//! struct with board-specific fields (MAC, IP, gateway, UART base,
//! WiFi SSID, etc.). The structs share a few universal fields —
//! Zenoh locator, ROS 2 domain ID — but cross-board generic code
//! (a benchmark harness, a multi-target test driver) had no way to
//! reach those without `cfg`-gating the type name.
//!
//! [`BoardConfig`] is the trait every board's config implements so
//! generic code can read the universal fields uniformly:
//!
//! ```ignore
//! fn print_config<C: nros_platform::BoardConfig>(c: &C) {
//!     println!("locator: {}", c.zenoh_locator());
//!     println!("domain:  {}", c.domain_id());
//! }
//! ```
//!
//! The trait stays minimal on purpose: each board's transport-specific
//! knobs (MAC address, WiFi credentials, UART base) remain on the
//! concrete `Config` struct as ordinary fields. Adding
//! transport-specific extension traits (e.g. `EthernetConfig`,
//! `WifiConfig`, `SerialConfig`) is a follow-up that can land
//! without changing this trait.

/// Universal board configuration accessors.
///
/// Implemented by every board crate's top-level config struct
/// (`Config`, `NodeConfig`, etc.). Generic code that needs to read the
/// Zenoh locator or the ROS 2 domain ID can take `&impl BoardConfig`
/// instead of `cfg`-gating on each board type.
pub trait BoardConfig {
    /// Zenoh router/peer locator string (e.g. `"tcp/192.168.1.50:7447"`).
    fn zenoh_locator(&self) -> &str;

    /// ROS 2 domain ID (default `0`).
    fn domain_id(&self) -> u32;
}

/// Phase 173.5 — mutable transport knobs the orchestration generator
/// writes into a board `Config` from `nros.toml` `[[transport]]` (the
/// `NanoRosOwned` net-stack path: the board owns smoltcp/lwIP/NetX, so
/// the IP / baud value lands in the board `Config` rather than an RTOS
/// config fragment).
///
/// Every method has a no-op default so a board only overrides the knobs
/// it actually has (a serial-only board ignores `set_ipv4`; an
/// ethernet-only board ignores `set_baudrate`). Boards whose net stack
/// is owned by the RTOS (`RtosOwned`: Zephyr / NuttX) do **not** impl
/// this — their IP lands in the emitted config fragment instead.
pub trait BoardTransportConfig {
    /// Static IPv4 address + prefix length for the board's ethernet
    /// stack. Boards without a `prefix` field ignore that argument.
    fn set_ipv4(&mut self, _addr: [u8; 4], _prefix: u8) {}

    /// Ethernet MAC address. Boards with a fixed/fused MAC ignore this.
    /// (Phase 172.J — the orchestration generator writes it from
    /// `nros.toml` `[[transport]]` `mac`, replacing `config.toml`'s
    /// `[network].mac`.)
    fn set_mac(&mut self, _mac: [u8; 6]) {}

    /// Default IPv4 gateway for the board's ethernet stack. Boards on a
    /// flat link (no gateway) ignore this. (Phase 172.J — from
    /// `nros.toml` `[[transport]]` `gateway`, replacing `config.toml`'s
    /// `[network].gateway`.)
    fn set_gateway(&mut self, _addr: [u8; 4]) {}

    /// Serial line rate for the board's UART transport.
    fn set_baudrate(&mut self, _baud: u32) {}

    /// WiFi SSID for boards with a WiFi transport (ESP32). Wired boards
    /// ignore it. (Phase 172.K.4 — from `nros.toml` `[[transport]]` `ssid`,
    /// replacing `config.toml`'s `[wifi].ssid`.)
    fn set_ssid(&mut self, _ssid: &str) {}

    /// WiFi password (paired with [`set_ssid`]). (Phase 172.K.4 —
    /// `[[transport]]` `password`, replacing `config.toml`'s `[wifi].password`.)
    fn set_password(&mut self, _password: &str) {}

    /// NIC name(s) this transport multi-homes over (`["eth0", "eth1"]`).
    /// Boards with a single fixed NIC (every embedded target today) ignore it;
    /// the seam exists for a multi-homed hosted board to fold several
    /// interfaces into one session. (Phase 172.K.7 — from `nros.toml`
    /// `[[transport]]` `interfaces`.)
    fn set_interfaces(&mut self, _interfaces: &[&str]) {}
}
