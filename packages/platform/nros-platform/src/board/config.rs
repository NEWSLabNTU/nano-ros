//! Cross-board configuration trait.
//!
//! Every board crate (`nros-board-mps2-an385`, `nros-board-stm32f4`,
//! `nros-board-esp32-qemu`, …) declares its own `Config` / `NodeConfig`
//! struct with board-specific fields (MAC, IP, gateway, UART base,
//! WiFi SSID, etc.). The structs share a few universal fields —
//! transport locator, ROS 2 domain ID — but cross-board generic code
//! (a benchmark harness, a multi-target test driver) had no way to
//! reach those without `cfg`-gating the type name.
//!
//! [`BoardConfig`] is the trait every board's config implements so
//! generic code can read the universal fields uniformly:
//!
//! ```ignore
//! fn print_config<C: nros_platform::BoardConfig>(c: &C) {
//!     println!("locator: {}", c.locator());
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
/// transport locator or the ROS 2 domain ID can take `&impl BoardConfig`
/// instead of `cfg`-gating on each board type.
pub trait BoardConfig {
    /// Transport locator string the RMW backend connects through
    /// (e.g. `"tcp/192.168.1.50:7447"`, `"serial/UART_0#baudrate=115200"`,
    /// `"ivc/2"`).
    ///
    /// The name matches the RMW vtable's own `locator` parameter and
    /// `ExecutorConfig::new`. It is deliberately backend-neutral: this is a
    /// core, RMW-agnostic trait, so it must not name a concrete backend
    /// (issue 0330 — the same class as issue 0225).
    fn locator(&self) -> &str;

    /// ROS 2 domain ID (default `0`).
    fn domain_id(&self) -> u32;

    /// Deprecated alias for [`locator`](BoardConfig::locator).
    ///
    /// Kept as a defaulted method (not a required one) so out-of-tree board
    /// crates that still spell the old name keep compiling: they get the
    /// forwarding default for free once they rename their own impl, and
    /// callers of the old name keep working with a deprecation warning.
    #[deprecated(
        since = "0.6.0",
        note = "renamed to `locator()` — the core trait must not name a backend (issue 0330)"
    )]
    fn zenoh_locator(&self) -> &str {
        self.locator()
    }
}

// `BoardTransportConfig` was removed here (issue 1064). It carried
// `set_ipv4` / `set_baudrate` — five and two real board implementations, and
// ZERO callers: its only writer was the orchestration generator deleted with
// the standalone-package pipeline in `11a00b0f8` (#202).
//
// The live path is the DEPLOY OVERLAY, and it is strictly better: `DeployOverlay`
// carries `ip`, `gateway`, `netmask`, `locator`, `domain_id` and `transport`
// (a superset — `gateway` is what the removed `set_gateway` was for), it is
// applied by `BoardEntry::run_with_deploy`, and it is actually READ —
// `nros-board-common/src/base_config.rs` does it for the whole family, and
// esp32-qemu, mps2-an385 and nuttx-qemu read it directly.
//
// So this was not a seam awaiting a caller; it was the dead twin of a live one.
// Same finding as phase-206 W4 (issue 1067) one layer over: the discoverable
// contract and the executed contract were different things.
