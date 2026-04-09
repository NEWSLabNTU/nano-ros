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
