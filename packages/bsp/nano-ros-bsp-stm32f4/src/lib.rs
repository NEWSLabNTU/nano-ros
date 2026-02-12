//! nano-ros BSP for STM32F4 family microcontrollers
//!
//! This crate is a thin wrapper over `nano-ros-platform-stm32f4`.
//! All functionality is re-exported from the platform crate.

#![no_std]

pub use nano_ros_platform_stm32f4::*;
