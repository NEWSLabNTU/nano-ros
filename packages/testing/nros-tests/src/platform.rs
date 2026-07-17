//! Per-platform configuration constants for test isolation.
//!
//! Since the phase-295 W4 re-bake, every base here is DERIVED from the ONE
//! allocator ([`crate::alloc`], RFC-0051): a platform's zenohd base is
//! `alloc::platform_port_base(PlatformId)` and the per-(variant, lang)
//! split is the allocator's `workload.port_offset() + lang * 100` formula.
//! `PlatformConfig` survives as the QEMU-harness-facing VIEW of the
//! allocator (the harness APIs speak [`TestVariant`]/[`TestLang`]);
//! `zenohd_port_for(variant, lang)` returns exactly
//! `alloc::port_of(platform, lang, variant-workload)`.
//!
//! Historical layout (phase 89.9–89.13): pubsub/service/action get
//! `base + 0/10/20`, languages get `+ lang * 100` (Rust/C/C++ =
//! +0/+100/+200). The pre-W4 745x bases (with `lang_stride = 0` islands on
//! baremetal/esp32) are gone — every platform is on the formula with
//! `lang_stride = 100`, so every (variant, lang) image pair owns a unique
//! host port and same-platform lanes parallelize.
//!
//! Slirp-networked QEMU platforms (FreeRTOS, NuttX, ThreadX-RV64, ESP32)
//! isolate guest IPs per QEMU instance automatically — only the shared
//! host port matters. Bridge-networked platforms (ThreadX Linux sim)
//! also need per-variant guest IPs and interface names; those are encoded
//! in the per-example `config.toml`, not here.

use crate::{
    alloc::{platform_port_base, platform_xrce_base},
    matrix::PlatformId,
};

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
/// per (variant × language) combination.
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

/// Per-platform test configuration (the allocator's QEMU-harness view).
pub struct PlatformConfig {
    pub name: &'static str,
    /// Base port for pubsub / Rust tests — `alloc::platform_port_base`.
    /// Service tests use `zenohd_port + 10`, action tests `+ 20` (see
    /// [`TestVariant`]). Non-Rust languages add `TestLang::index() *
    /// lang_stride` on top — see [`Self::zenohd_port_for`].
    pub zenohd_port: u16,
    /// Stride between per-language ports within the same variant — 100 on
    /// every platform since the phase-295 W4 re-bake (the allocator's lang
    /// column width).
    pub lang_stride: u16,
    /// Base port for the XRCE-DDS Agent on this platform —
    /// `alloc::platform_xrce_base`. Same per-(variant, lang) split as
    /// `zenohd_port`. `0` means the platform doesn't run any XRCE tests
    /// (no agent port reserved).
    pub xrce_agent_port: u16,
    /// Stride for `xrce_agent_port_for` (100 where XRCE runs).
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

/// Bare-metal QEMU ARM (MPS2-AN385, RTIC demo set).
///
/// | Variant | Rust  |
/// |---------|-------|
/// | Pubsub  | 10200 |
/// | Service | 10210 |
/// | Action  | 10220 |
///
/// The extra bare-metal image pairs that outnumber the workload axis (BSP
/// pair, RTIC mixed-priority pair, large-msg bench) take the named
/// `alloc::BAREMETAL_*_PORT` aux slots (10500/10510/10520) — nothing
/// shares a router anymore (the pre-W4 `qemu-baremetal-shared` group is
/// retired).
pub const BAREMETAL: PlatformConfig = PlatformConfig {
    name: "baremetal",
    zenohd_port: platform_port_base(PlatformId::QemuBaremetal),
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// FreeRTOS QEMU ARM (MPS2-AN385, lwIP).
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7800 | 7900 | 8000 |
/// | Service | 7810 | 7910 | 8010 |
/// | Action  | 7820 | 7920 | 8020 |
pub const FREERTOS: PlatformConfig = PlatformConfig {
    name: "freertos",
    zenohd_port: platform_port_base(PlatformId::FreertosMps2),
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// NuttX QEMU ARM (virt, Cortex-A7).
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 8200 | 8300 | 8400 |
/// | Service | 8210 | 8310 | 8410 |
/// | Action  | 8220 | 8320 | 8420 |
pub const NUTTX: PlatformConfig = PlatformConfig {
    name: "nuttx",
    zenohd_port: platform_port_base(PlatformId::NuttxArm),
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// ThreadX QEMU RISC-V 64 (virt, virtio-net).
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 9400 | 9500 | 9600 |
/// | Service | 9410 | 9510 | 9610 |
/// | Action  | 9420 | 9520 | 9620 |
///
/// C++ service/action are skipped — examples not implemented — but the
/// port slots remain reserved for future use.
pub const THREADX_RISCV: PlatformConfig = PlatformConfig {
    name: "threadx-riscv",
    zenohd_port: platform_port_base(PlatformId::ThreadxRiscv64),
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// ESP32-C3 QEMU (Espressif fork, open_eth).
///
/// | Variant | Rust |
/// |---------|------|
/// | Pubsub  | 9800 |
/// | Service | 9810 |
/// | Action  | 9820 |
///
/// The workspace Entry image takes the `EntryPubsub` slot (9830) — see
/// `alloc::port_of` — so the ws-entry e2e no longer shares the pubsub
/// pair's router (the pre-W4 `qemu-esp32` serial group is retired).
pub const ESP32: PlatformConfig = PlatformConfig {
    name: "esp32",
    zenohd_port: platform_port_base(PlatformId::Esp32Qemu),
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// ThreadX Linux simulation (veth pairs / NSOS host sockets).
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 9000 | 9100 | 9200 |
/// | Service | 9010 | 9110 | 9210 |
/// | Action  | 9020 | 9120 | 9220 |
pub const THREADX_LINUX: PlatformConfig = PlatformConfig {
    name: "threadx-linux",
    zenohd_port: platform_port_base(PlatformId::ThreadxLinux),
    lang_stride: 100,
    xrce_agent_port: 0,
    xrce_lang_stride: 0,
};

/// Zephyr (native_sim or QEMU).
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 7400 | 7500 | 7600 |
/// | Service | 7410 | 7510 | 7610 |
/// | Action  | 7420 | 7520 | 7620 |
///
/// XRCE-DDS Agent ports follow the same per-(variant, lang) split in the
/// allocator's agent band:
///
/// | Variant | Rust | C    | C++  |
/// |---------|------|------|------|
/// | Pubsub  | 2400 | 2500 | 2600 |
/// | Service | 2410 | 2510 | 2610 |
/// | Action  | 2420 | 2520 | 2620 |
///
/// The compile-time `CONFIG_NROS_XRCE_AGENT_PORT` /
/// `CONFIG_NROS_ZENOH_LOCATOR` Kconfigs are baked at `west build` time per
/// (variant, lang) — see `scripts/build/zephyr-fixture-leaves.sh`, which
/// computes the SAME formula.
pub const ZEPHYR: PlatformConfig = PlatformConfig {
    name: "zephyr",
    zenohd_port: platform_port_base(PlatformId::ZephyrNativeSim),
    lang_stride: 100,
    xrce_agent_port: platform_xrce_base(PlatformId::ZephyrNativeSim),
    xrce_lang_stride: 100,
};
