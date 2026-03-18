//! Per-platform configuration constants for test isolation.
//!
//! Each QEMU platform uses a fixed zenohd port so that platforms can run in
//! parallel (each with its own zenohd instance) while tests within a platform
//! remain serialized (`max-threads = 1`).
//!
//! Ports 7450–7459 are in the IANA unassigned range.

/// Per-platform test configuration.
pub struct PlatformConfig {
    pub name: &'static str,
    pub zenohd_port: u16,
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
