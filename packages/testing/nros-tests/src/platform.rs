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

/// Per-platform test configuration.
pub struct PlatformConfig {
    pub name: &'static str,
    /// Base port for pubsub tests. Service tests use `zenohd_port + 10`,
    /// action tests use `zenohd_port + 20` (see [`TestVariant`]).
    pub zenohd_port: u16,
}

impl PlatformConfig {
    /// Compute the zenohd port for a specific test variant.
    pub const fn zenohd_port_for(&self, variant: TestVariant) -> u16 {
        self.zenohd_port + variant.port_offset()
    }
}

/// Bare-metal QEMU ARM (MPS2-AN385, RTIC).
pub const BAREMETAL: PlatformConfig = PlatformConfig {
    name: "baremetal",
    zenohd_port: 7450,
};

/// FreeRTOS QEMU ARM (MPS2-AN385, lwIP).
pub const FREERTOS: PlatformConfig = PlatformConfig {
    name: "freertos",
    zenohd_port: 7451,
};

/// NuttX QEMU ARM (virt, Cortex-A7).
pub const NUTTX: PlatformConfig = PlatformConfig {
    name: "nuttx",
    zenohd_port: 7452,
};

/// ThreadX QEMU RISC-V 64 (virt, virtio-net).
pub const THREADX_RISCV: PlatformConfig = PlatformConfig {
    name: "threadx-riscv",
    zenohd_port: 7453,
};

/// ESP32-C3 QEMU (Espressif fork, open_eth).
pub const ESP32: PlatformConfig = PlatformConfig {
    name: "esp32",
    zenohd_port: 7454,
};

/// ThreadX Linux simulation (veth pairs).
pub const THREADX_LINUX: PlatformConfig = PlatformConfig {
    name: "threadx-linux",
    zenohd_port: 7455,
};

/// Zephyr (native_sim or QEMU).
pub const ZEPHYR: PlatformConfig = PlatformConfig {
    name: "zephyr",
    zenohd_port: 7456,
};
