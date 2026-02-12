//! # nano-ros-bsp-qemu
//!
//! Board Support Package for running nano-ros on QEMU MPS2-AN385.
//!
//! This is a thin wrapper around [`nano_ros_platform_qemu`] that provides
//! backwards compatibility. All functionality is delegated to the platform
//! crate.

#![no_std]

pub use nano_ros_platform_qemu::*;
