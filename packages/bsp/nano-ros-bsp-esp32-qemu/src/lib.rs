//! # nano-ros-bsp-esp32-qemu
//!
//! Board Support Package for running nano-ros on ESP32-C3 in QEMU.
//!
//! This is a thin wrapper around [`nano_ros_platform_esp32_qemu`] that provides
//! backwards compatibility. All functionality is delegated to the platform
//! crate.

#![no_std]

pub use nano_ros_platform_esp32_qemu::*;
