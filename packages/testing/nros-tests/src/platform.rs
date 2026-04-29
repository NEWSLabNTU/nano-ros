//! Per-platform configuration constants for test isolation.
//!
//! Each QEMU platform uses a fixed zenohd port so that platforms can run in
//! parallel (each with its own zenohd instance).
//!
//! Phase 89.9/89.10: to also allow **within-platform** parallelism for the
//! three test variants (pubsub / service / action), each variant gets its
//! own derived port: `zenohd_port + 0` for pubsub, `+ 10` for service,
//! `+ 20` for action. Ports 7450–7479 are all in the IANA unassigned
//! range.
//!
//! Phase 89.13 (pilot on FreeRTOS): further split each (variant) port by
//! language, so same-variant Rust / C / C++ tests can also run in parallel.
//! The per-language offset is controlled by `PlatformConfig::lang_stride`:
//! platforms that have had their example `config.toml` files migrated set
//! `lang_stride = 100` (Rust=+0, C=+100, C++=+200); un-migrated platforms
//! keep `lang_stride = 0` so all three language binaries still target the
//! Rust port (and must stay serialized via per-variant nextest sub-groups).
//!
//! Slirp-networked QEMU platforms (FreeRTOS, NuttX, ThreadX-RV64, ESP32)
//! isolate guest IPs per QEMU instance automatically — only the shared
//! host port matters. Bridge-networked platforms (ThreadX Linux sim)
//! also need per-variant guest IPs and interface names; those are encoded
//! in the per-example `config.toml`, not here.

/// Which of the three rtos_e2e test variants a port is for.
///
/// Lives in `nros_tests::platform` so both the test harness (to pick the
/// right router port) and downstream tooling can reference it by name.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TestVariant {
    /// Publisher/subscriber end-to-end test (uses port `base + 0`).
    Pubsub,
    /// Service server/client end-to-end test (uses port `base + 10`).
    Service,
    /// Action server/client end-to-end test (uses port `base + 20`).
    Action,
}

impl TestVariant {
    /// Offset added to each platform's base port to derive the per-variant port.
    pub const fn port_offset(self) -> u16 {
        match self {
            TestVariant::Pubsub => 0,
            TestVariant::Service => 10,
            TestVariant::Action => 20,
        }
    }
}

/// Language binding of the example under test. Used together with
/// [`PlatformConfig::lang_stride`] to derive a unique host zenohd port
/// per (variant × language) combination on migrated platforms.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TestLang {
    /// Rust example (lang multiplier 0).
    Rust,
    /// C example (lang multiplier 1).
    C,
    /// C++ example (lang multiplier 2).
    Cpp,
}

impl TestLang {
    /// Multiplier applied to `PlatformConfig::lang_stride` when computing
    /// the per-(variant, lang) port.
    pub const fn index(self) -> u16 {
        match self {
            TestLang::Rust => 0,
            TestLang::C => 1,
            TestLang::Cpp => 2,
        }
    }
}

/// Per-platform test configuration.
pub struct PlatformConfig {
    pub name: &'static str,
    /// Base port for pubsub / Rust tests. Service tests use `zenohd_port + 10`,
    /// action tests use `zenohd_port + 20` (see [`TestVariant`]). Non-Rust
    /// languages add `TestLang::index() * lang_stride` on top — see
    /// [`Self::zenohd_port_for`].
    pub zenohd_port: u16,
    /// Stride between per-language ports within the same variant. `0`
    /// means the platform hasn't had its example `config.toml` files
    /// migrated to the (variant, lang) scheme yet — all three languages
    /// share the Rust port (and must stay serialized at the test-group
    /// level). Migrated platforms use `100` so C = Rust+100, C++ = Rust+200.
    pub lang_stride: u16,
    /// Base port for the XRCE-DDS Agent on this platform. Same per-(variant,
    /// lang) split as `zenohd_port` (variant offset = 10 / 20, lang stride =
    /// `xrce_lang_stride`). `0` means the platform doesn't run any XRCE
    /// tests (no agent port reserved).
    pub xrce_agent_port: u16,
    /// Stride for `xrce_agent_port_for`. `0` collapses every (variant, lang)
    /// onto the base port (legacy serialized mode).
    pub xrce_lang_stride: u16,
}

impl PlatformConfig {
    /// Compute the zenohd port for a specific (variant, language) combination.
    pub const fn zenohd_port_for(&self, variant: TestVariant, lang: TestLang) -> u16 {
        self.zenohd_port + variant.port_offset() + lang.index() * self.lang_stride
    }

    /// Compute the XRCE-Agent UDP port for a specific (variant, language)
    /// combination. Mirrors [`Self::zenohd_port_for`] for tests that use
    /// XRCE-DDS instead of zenoh.
    ///
    /// Returns `0` when the platform reserves no XRCE port (i.e. doesn't
    /// run XRCE tests). Callers must treat that as "no XRCE backend on
    /// this platform" and skip.
    pub const fn xrce_agent_port_for(&self, variant: TestVariant, lang: TestLang) -> u16 {
        if self.xrce_agent_port == 0 {
            0
        } else {
            self.xrce_agent_port + variant.port_offset() + lang.index() * self.xrce_lang_stride
        }
    }
}

/// Bare-metal QEMU ARM (MPS2-AN385, RTIC).
pub const BAREMETAL: PlatformConfig = PlatformConfig {
    name: "baremetal",
    zenohd_port: 7450,
    lang_stride: 0,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// FreeRTOS QEMU ARM (MPS2-AN385, lwIP).
///
/// Phase 89.13 pilot: migrated to per-(variant, lang) ports. The 9 slots are:
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7451 | 7551 | 7651 |
/// | Service | 7461 | 7561 | 7661 |
/// | Action  | 7471 | 7571 | 7671 |
pub const FREERTOS: PlatformConfig = PlatformConfig {
    name: "freertos",
    zenohd_port: 7451,
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// NuttX QEMU ARM (virt, Cortex-A7).
///
/// Phase 89.13: migrated to per-(variant, lang) ports.
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7452 | 7552 | 7652 |
/// | Service | 7462 | 7562 | 7662 |
/// | Action  | 7472 | 7572 | 7672 |
pub const NUTTX: PlatformConfig = PlatformConfig {
    name: "nuttx",
    zenohd_port: 7452,
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// ThreadX QEMU RISC-V 64 (virt, virtio-net).
///
/// Phase 89.13: migrated to per-(variant, lang) ports.
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7453 | 7553 | 7653 |
/// | Service | 7463 | 7563 | 7663 |
/// | Action  | 7473 | 7573 | 7673 |
///
/// C++ service/action are skipped — examples not implemented — but the
/// port slots remain reserved for future use.
pub const THREADX_RISCV: PlatformConfig = PlatformConfig {
    name: "threadx-riscv",
    zenohd_port: 7453,
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// ESP32-C3 QEMU (Espressif fork, open_eth).
pub const ESP32: PlatformConfig = PlatformConfig {
    name: "esp32",
    zenohd_port: 7454,
    lang_stride: 0,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// ThreadX Linux simulation (veth pairs).
///
/// Phase 89.13: migrated to per-(variant, lang) ports. NSOS offloads BSD
/// sockets to the host kernel (bypassing the legacy veth `interface`/`ip`
/// fields in `config.toml`), so only the zenohd port matters for
/// cross-test isolation.
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7455 | 7555 | 7655 |
/// | Service | 7465 | 7565 | 7665 |
/// | Action  | 7475 | 7575 | 7675 |
pub const THREADX_LINUX: PlatformConfig = PlatformConfig {
    name: "threadx-linux",
    zenohd_port: 7455,
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// Zephyr (native_sim or QEMU).
///
/// Phase 89.13: migrated to per-(variant, lang) ports.
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7456 | 7556 | 7656 |
/// | Service | 7466 | 7566 | 7666 |
/// | Action  | 7476 | 7576 | 7676 |
///
/// Only Rust + C++ zenoh tests currently exist on Zephyr; the C column
/// is reserved for future zenoh-backed C examples.
///
/// XRCE-DDS Agent ports follow the same per-(variant, lang) split via
/// `xrce_agent_port` / `xrce_lang_stride` so cpp/xrce/c-xrce/rust-xrce
/// tests can run in parallel:
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 2018 | 2118 | 2218 |
/// | Service | 2028 | 2128 | 2228 |
/// | Action  | 2038 | 2138 | 2238 |
///
/// The compile-time `CONFIG_NROS_XRCE_AGENT_PORT` Kconfig is overridden
/// at `west build` time per (variant, lang) — see `just/zephyr.just`.
pub const ZEPHYR: PlatformConfig = PlatformConfig {
    name: "zephyr",
    zenohd_port: 7456,
    lang_stride: 100,
    xrce_agent_port: 2018,
    xrce_lang_stride: 100,
};
