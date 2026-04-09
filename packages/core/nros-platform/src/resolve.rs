//! Concrete platform type alias resolved at compile time.
//!
//! When a platform feature is enabled, [`ConcretePlatform`] resolves to
//! the active backend. When no platform is selected (e.g., default workspace
//! build), the type is not defined — downstream crates that need it must
//! enable a platform feature.

#[cfg(feature = "platform-posix")]
pub type ConcretePlatform = nros_platform_posix::PosixPlatform;

#[cfg(feature = "platform-cffi")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-mps2-an385")]
pub type ConcretePlatform = nros_platform_mps2_an385::Mps2An385Platform;

#[cfg(feature = "platform-stm32f4")]
pub type ConcretePlatform = nros_platform_stm32f4::Stm32f4Platform;

#[cfg(feature = "platform-esp32")]
pub type ConcretePlatform = nros_platform_esp32::Esp32Platform;

#[cfg(feature = "platform-esp32-qemu")]
pub type ConcretePlatform = nros_platform_esp32_qemu::Esp32QemuPlatform;

#[cfg(feature = "platform-nuttx")]
pub type ConcretePlatform = nros_platform_nuttx::NuttxPlatform;

#[cfg(feature = "platform-freertos")]
pub type ConcretePlatform = nros_platform_freertos::FreeRtosPlatform;

#[cfg(feature = "platform-threadx")]
pub type ConcretePlatform = nros_platform_threadx::ThreadxPlatform;
