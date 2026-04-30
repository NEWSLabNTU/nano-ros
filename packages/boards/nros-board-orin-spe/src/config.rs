//! Configuration for AGX Orin SPE nodes.
//!
//! Drops every IP-related field from the FreeRTOS+lwIP boards — the
//! SPE has no network MAC, only IVC. The only locator that makes
//! sense is `ivc/<channel-id>`, defaulting to channel 2 (`aon_echo`).
//!
//! Defense-in-depth (Phase 100 design Q1 mitigation): [`Config::run_or_panic_on_serial`]
//! wraps `with_zenoh_locator` to **reject `serial/...` URIs** at boot.
//! `serial/2` and `ivc/2` are visually identical, and the SPE has no
//! UART transport — silently accepting a `serial/` locator would
//! disconnect cleanly with no diagnostic. We make the failure loud.

/// Board configuration for AGX Orin SPE.
///
/// # Default
///
/// - Locator: `ivc/2` (`aon_echo` channel).
/// - Domain: 0.
/// - App task priority 12 / stack 16 KB. The SPE's BTCM is 256 KB, so
///   stacks are deliberately tighter than the lwIP-bearing
///   `nros-board-mps2-an385-freertos` (which used 64 KB).
/// - Zenoh-pico read/lease tasks priority 16, stack 4 KB each.
#[derive(Clone)]
pub struct Config {
    /// Zenoh locator string. **Must start with `ivc/`** —
    /// [`with_zenoh_locator`](Self::with_zenoh_locator) panics if not.
    pub zenoh_locator: &'static str,
    /// ROS 2 domain ID (default: 0).
    pub domain_id: u32,

    // ── Scheduling ─────────────────────────────────────────────────────
    // Normalized 0–31 scale; mapped to FSP FreeRTOS priorities at
    // task-create time via [`to_freertos_priority`]. The FSP's
    // `configMAX_PRIORITIES` is typically 8, matching the MPS2 build.
    /// Application task priority (normalized 0–31, default 12).
    pub app_priority: u8,
    /// Application task stack size in bytes (default 16384).
    pub app_stack_bytes: u32,
    /// Zenoh-pico read task priority (normalized 0–31, default 16).
    pub zenoh_read_priority: u8,
    /// Zenoh-pico read task stack size in bytes (default 4096).
    pub zenoh_read_stack_bytes: u32,
    /// Zenoh-pico lease task priority (normalized 0–31, default 16).
    pub zenoh_lease_priority: u8,
    /// Zenoh-pico lease task stack size in bytes (default 4096).
    pub zenoh_lease_stack_bytes: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            // Channel 2 is `aon_echo` per NVIDIA's L4T 36.4 device tree
            // (the canonical IVC bring-up channel). Other carveouts
            // would be configured by the firmware's NVIDIA Makefile;
            // in that case override via `with_zenoh_locator("ivc/N")`.
            zenoh_locator: "ivc/2",
            domain_id: 0,
            app_priority: 12,
            app_stack_bytes: 16384,
            zenoh_read_priority: 16,
            zenoh_read_stack_bytes: 4096,
            zenoh_lease_priority: 16,
            zenoh_lease_stack_bytes: 4096,
        }
    }
}

impl Config {
    /// Builder: set zenoh locator. Panics at boot if the locator does
    /// not start with `ivc/` — the SPE has no UART or Ethernet
    /// transport, so any other scheme is misconfiguration.
    ///
    /// This catches the visual collision between `serial/N` and
    /// `ivc/N` that Phase 100 design §9 flagged: the URI prefix
    /// disambiguates at parse time, but a copy-paste between board
    /// templates can land on the wrong scheme silently. Asserting at
    /// boot turns the failure mode "no peer ever shows up" into
    /// "panic with a clear message".
    pub fn with_zenoh_locator(mut self, locator: &'static str) -> Self {
        assert!(
            locator.starts_with("ivc/"),
            "nros-board-orin-spe: zenoh locator must use the `ivc/` scheme. \
             The SPE has no UART or Ethernet — `serial/...` and `tcp/...` \
             would parse but find no transport. Got: {locator}"
        );
        self.zenoh_locator = locator;
        self
    }

    /// Builder: set ROS 2 domain ID.
    pub fn with_domain_id(mut self, domain_id: u32) -> Self {
        self.domain_id = domain_id;
        self
    }

    /// Builder: set application task priority (normalized 0–31).
    pub fn with_app_priority(mut self, p: u8) -> Self {
        self.app_priority = p.min(31);
        self
    }

    /// Builder: set application task stack size in bytes.
    pub fn with_app_stack_bytes(mut self, s: u32) -> Self {
        self.app_stack_bytes = s;
        self
    }

    /// Map a normalized 0–31 priority to FreeRTOS priority. The FSP's
    /// `configMAX_PRIORITIES` is typically 8, so the output range is
    /// 0–7. Same mapping as the MPS2 board crate.
    pub fn to_freertos_priority(normalized: u8) -> u32 {
        let n = if normalized > 31 { 31 } else { normalized };
        (n as u32 * 7) / 31
    }
}
