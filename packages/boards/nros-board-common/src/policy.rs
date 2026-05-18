//! Link-feature env reader + per-platform policy mask.
//!
//! Phase 134.2 introduced the `LinkFeatures` env reader + the
//! `LinkPolicy` mask that overrides per-platform invariants
//! (Orin SPE has no Ethernet → `Force(false)` masks TCP / UDP /
//! MC / SERIAL / TLS, etc.). Phase 136.2 extracted it from
//! `zpico-sys/build.rs` into `zpico-sys/build/policy.rs`. Phase
//! 152.5 lifted it into the `nros-board-common` library so the
//! per-kernel generic board crates can share one canonical
//! implementation alongside the manifest parser.
//!
//! Use from `build.rs`:
//! ```ignore
//! use nros_board_common::policy::{LinkFeatures, LinkPolicy};
//! let link = LinkFeatures::from_env().apply(&LinkPolicy::posix());
//! ```

use std::env;

/// Protocol link features read from Cargo feature flags.
///
/// Each field corresponds to a `link-*` Cargo feature that controls
/// the matching `Z_FEATURE_LINK_*` flag passed to zenoh-pico at compile time.
pub struct LinkFeatures {
    pub tcp: bool,
    pub udp_unicast: bool,
    pub udp_multicast: bool,
    pub serial: bool,
    pub raweth: bool,
    pub tls: bool,
    // Phase 100.4 — NVIDIA Tegra IVC link transport.
    pub ivc: bool,
    // Phase 115.B — runtime-pluggable user transport.
    pub custom: bool,
}

impl LinkFeatures {
    /// Read link features from Cargo environment variables.
    ///
    /// Phase 128.E.1 — `tcp`, `udp_unicast`, `udp_multicast`, and
    /// `serial` are always on. Their vendor C sources have no
    /// external build-host dependency and the wire format isn't
    /// selected until session-open consults the locator string
    /// (`tcp/...`, `udp4/...`, `serial/...`). Keeping them gated on
    /// `link-*` Cargo features only forced every consumer to
    /// duplicate the same selection that already lives in the
    /// locator string.
    ///
    /// `raweth`, `tls`, `ivc`, `custom` stay explicit because each
    /// carries a real build-host requirement (raw-socket capability,
    /// mbedTLS / OpenSSL provider, NVIDIA IVC headers, the
    /// `zpico-platform-custom` crate).
    pub fn from_env() -> Self {
        Self {
            tcp: true,
            udp_unicast: true,
            udp_multicast: true,
            serial: true,
            raweth: env::var("CARGO_FEATURE_LINK_RAWETH").is_ok(),
            tls: env::var("CARGO_FEATURE_LINK_TLS").is_ok(),
            ivc: env::var("CARGO_FEATURE_LINK_IVC").is_ok(),
            custom: env::var("CARGO_FEATURE_LINK_CUSTOM").is_ok(),
        }
    }

    /// Phase 134.2 — apply a platform-invariant policy mask. Each
    /// `PolicyChoice` value either forces the field to a literal (SPE
    /// has no Ethernet → `Force(false)` masks TCP/UDP/MC/SERIAL/TLS)
    /// or lets the upstream `LinkFeatures::from_env()` value through
    /// (`Follow`). Constructor matches what the previous per-build-fn
    /// `build.define("Z_FEATURE_LINK_*", "0")` literals encoded, just
    /// in declarative form.
    pub fn apply(mut self, policy: &LinkPolicy) -> Self {
        self.tcp = policy.tcp.resolve(self.tcp);
        self.udp_unicast = policy.udp_unicast.resolve(self.udp_unicast);
        self.udp_multicast = policy.udp_multicast.resolve(self.udp_multicast);
        self.serial = policy.serial.resolve(self.serial);
        self.raweth = policy.raweth.resolve(self.raweth);
        self.tls = policy.tls.resolve(self.tls);
        self.ivc = policy.ivc.resolve(self.ivc);
        self.custom = policy.custom.resolve(self.custom);
        self
    }

    pub fn tcp_flag(&self) -> u8 {
        self.tcp as u8
    }
    pub fn udp_unicast_flag(&self) -> u8 {
        self.udp_unicast as u8
    }
    pub fn udp_multicast_flag(&self) -> u8 {
        self.udp_multicast as u8
    }
    pub fn serial_flag(&self) -> u8 {
        self.serial as u8
    }
    pub fn raweth_flag(&self) -> u8 {
        self.raweth as u8
    }
    pub fn tls_flag(&self) -> u8 {
        self.tls as u8
    }
    pub fn ivc_flag(&self) -> u8 {
        self.ivc as u8
    }
    pub fn custom_flag(&self) -> u8 {
        self.custom as u8
    }
}

/// Phase 134.2 — per-platform link-feature policy mask.
///
/// Layered on top of `LinkFeatures::from_env()`. Each field is a
/// `PolicyChoice`: `Force(bool)` overrides the env-derived value;
/// `Follow` lets it through. Replaces the eight functions' worth of
/// scattered `build.define("Z_FEATURE_LINK_*", "0")` literals with
/// one declarative table per platform.
#[derive(Copy, Clone)]
pub enum PolicyChoice {
    Force(bool),
    Follow,
}

impl PolicyChoice {
    pub fn resolve(self, env_value: bool) -> bool {
        match self {
            PolicyChoice::Force(v) => v,
            PolicyChoice::Follow => env_value,
        }
    }
}

#[derive(Copy, Clone)]
pub struct LinkPolicy {
    pub tcp: PolicyChoice,
    pub udp_unicast: PolicyChoice,
    pub udp_multicast: PolicyChoice,
    pub serial: PolicyChoice,
    pub raweth: PolicyChoice,
    pub tls: PolicyChoice,
    pub ivc: PolicyChoice,
    pub custom: PolicyChoice,
}

impl LinkPolicy {
    /// All-`Follow` baseline: every flag tracks Cargo env exactly.
    /// Used by every platform whose network stack supports the full
    /// set of transports (FreeRTOS+lwIP, NuttX, ThreadX/NetX,
    /// bare-metal/smoltcp).
    pub const fn passthrough() -> Self {
        Self {
            tcp: PolicyChoice::Follow,
            udp_unicast: PolicyChoice::Follow,
            udp_multicast: PolicyChoice::Follow,
            serial: PolicyChoice::Follow,
            raweth: PolicyChoice::Follow,
            tls: PolicyChoice::Follow,
            ivc: PolicyChoice::Follow,
            custom: PolicyChoice::Follow,
        }
    }

    /// POSIX policy — same as passthrough today. Phase 134.7 adds
    /// the missing multicast aliases in `platform_aliases.c`; until
    /// that lands the linker still fails on `_z_read_udp_multicast`
    /// if multicast is on, but `LinkFeatures::from_env()` already
    /// hardcodes `udp_multicast=true` so the policy can't paper
    /// over the gap.
    pub const fn posix() -> Self {
        Self::passthrough()
    }

    /// AGX Orin SPE — Cortex-R5F + NVIDIA FSP, no Ethernet, no
    /// serial, no TLS. Only IVC + custom transports are valid.
    /// Encodes the invariants that `build_zenoh_pico_orin_spe`
    /// used to scatter as `build.define("Z_FEATURE_LINK_*", "0")`
    /// literals at the bottom of the function body.
    pub const fn orin_spe() -> Self {
        Self {
            tcp: PolicyChoice::Force(false),
            udp_unicast: PolicyChoice::Force(false),
            udp_multicast: PolicyChoice::Force(false),
            serial: PolicyChoice::Force(false),
            raweth: PolicyChoice::Force(false),
            tls: PolicyChoice::Force(false),
            ivc: PolicyChoice::Follow,
            custom: PolicyChoice::Follow,
        }
    }

    /// Phase 146.2 — FreeRTOS + lwIP. zenoh-pico's
    /// `src/system/freertos/lwip/network.c` ships an explicit
    /// `#error "Serial not supported yet on FreeRTOS + LWIP port"`
    /// behind `Z_FEATURE_LINK_SERIAL == 1`, and no FreeRTOS user
    /// has wired a serial backend through `zpico-serial`. Force
    /// serial off so neither the upstream `#error` fires nor
    /// `src/system/common/serial.c` builds and emits unresolved
    /// `_z_*_serial_internal` calls. Same shape forces raweth and
    /// TLS off because no backend exists for them on FreeRTOS.
    pub const fn freertos_lwip() -> Self {
        Self {
            tcp: PolicyChoice::Follow,
            udp_unicast: PolicyChoice::Follow,
            // Phase 154 — vendor `system/freertos/lwip/network.c`
            // line 780 has a typo (`sockrecv->socket` for
            // `_z_close_udp_multicast`; field is `_socket`).
            // nano-ros doesn't use UDP multicast over zenoh-pico
            // on FreeRTOS+lwIP (router is TCP-only), so force
            // the feature off to stop the vendor typo from
            // firing once we start compiling
            // `system/freertos/lwip` (Phase 154 added it to the
            // manifest's `include`).
            udp_multicast: PolicyChoice::Force(false),
            serial: PolicyChoice::Force(false),
            raweth: PolicyChoice::Force(false),
            tls: PolicyChoice::Force(false),
            ivc: PolicyChoice::Follow,
            custom: PolicyChoice::Follow,
        }
    }

    /// Phase 146.2 — NuttX. Same shape as `freertos_lwip()`: NuttX
    /// ships no serial backend, no raweth, no TLS provider; forcing
    /// these features on under `LinkFeatures::from_env`'s Phase
    /// 128.E.1 "always-on" defaults causes `_z_*_serial_internal`
    /// to surface as link-time undefined symbols on every NuttX
    /// Rust example build.
    pub const fn nuttx() -> Self {
        Self {
            tcp: PolicyChoice::Follow,
            udp_unicast: PolicyChoice::Follow,
            udp_multicast: PolicyChoice::Follow,
            serial: PolicyChoice::Force(false),
            raweth: PolicyChoice::Force(false),
            tls: PolicyChoice::Force(false),
            ivc: PolicyChoice::Follow,
            custom: PolicyChoice::Follow,
        }
    }

    /// Phase 146.2 — ThreadX (both Linux sim and RV64 board).
    /// `c/platform/threadx/network.c` ships TCP/UDP/serial code over
    /// NetX Duo BSD but is NOT listed under `[platform.threadx]
    /// extra_sources` — threadx uses `platform_aliases.c` for
    /// network ops instead, and the alias TU has no serial wrapper.
    /// Force serial off to match: every ThreadX example uses
    /// TCP/UDP over NetX Duo BSD, none use serial.
    pub const fn threadx() -> Self {
        Self {
            tcp: PolicyChoice::Follow,
            udp_unicast: PolicyChoice::Follow,
            udp_multicast: PolicyChoice::Follow,
            serial: PolicyChoice::Force(false),
            raweth: PolicyChoice::Force(false),
            tls: PolicyChoice::Force(false),
            ivc: PolicyChoice::Follow,
            custom: PolicyChoice::Follow,
        }
    }
}
