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

/// Phase 173.5 — mutable transport knobs a board `Config` exposes on the
/// `NanoRosOwned` net-stack path: the board owns smoltcp/lwIP/NetX, so the
/// IP / baud value lands in the board `Config` rather than an RTOS config
/// fragment. Boards whose net stack is owned by the RTOS (`RtosOwned`:
/// Zephyr / NuttX) do **not** impl this — their IP lands in the emitted
/// config fragment instead.
///
/// Both methods keep a no-op default so a board overrides only the knob it
/// actually has (a serial-only board ignores `set_ipv4`; an ethernet-only
/// board ignores `set_baudrate`).
///
/// **nano-ros does not model NICs, MAC addresses or WiFi credentials**
/// (phase-206 W5). `set_mac` / `set_gateway` / `set_ssid` / `set_password` /
/// `set_interfaces` are gone: the device is up before ROS exists, so it is
/// the board's (or the RTOS's) job, and anything the *middleware* binds to is
/// configured in the middleware's own language — CycloneDDS XML
/// (`<General><Interfaces>`), a zenoh config's `listen`/`connect`. A
/// nano-ros-owned vocabulary for either would need a resolver, a gate and a
/// per-platform story only Linux can satisfy. The emitter for all five was
/// deleted with the standalone-package pipeline in `11a00b0f8`; the seams
/// outlived it with zero call sites.
///
/// **The two survivors have no callers either, and that is UNRESOLVED.** They
/// are kept because they are not dead in the same sense: five boards implement
/// `set_ipv4` (threadx-linux, threadx-qemu-riscv64, freertos, mps2-an385,
/// esp32-qemu) and two implement `set_baudrate` (mps2-an385, esp32-qemu) with
/// real bodies. Boards doing work nobody asks for is a different defect from a
/// dead seam, and deciding it means deciding who writes a `NanoRosOwned`
/// board's IP now that the generator is gone — not a NIC-vocabulary question.
/// Do not read the absence of callers here as "safe to delete": measure the
/// board impls first.
pub trait BoardTransportConfig {
    /// Static IPv4 address + prefix length for the board's ethernet
    /// stack. Boards without a `prefix` field ignore that argument.
    fn set_ipv4(&mut self, _addr: [u8; 4], _prefix: u8) {}

    /// Serial line rate for the board's UART transport.
    fn set_baudrate(&mut self, _baud: u32) {}
}
