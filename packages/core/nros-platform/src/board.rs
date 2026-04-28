//! Cross-board configuration trait.
//!
//! Every board crate (`nros-board-mps2-an385`, `nros-board-stm32f4`, `nros-board-esp32`,
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
