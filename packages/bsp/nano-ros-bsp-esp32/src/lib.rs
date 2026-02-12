//! # nano-ros-bsp-esp32
//!
//! Board Support Package for running nano-ros on ESP32-C3 with WiFi.
//!
//! This is a thin wrapper around [`nano_ros_platform_esp32`] that provides
//! backwards compatibility. All functionality is delegated to the platform
//! crate.

#![no_std]

pub use nano_ros_platform_esp32::*;
