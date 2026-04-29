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

#[cfg(feature = "platform-zephyr")]
pub type ConcretePlatform = nros_platform_zephyr::ZephyrPlatform;

#[cfg(feature = "platform-orin-spe")]
pub type ConcretePlatform = nros_platform_orin_spe::OrinSpe;

// ============================================================================
// Phase 71.22 — opaque-buffer sizes for `_z_sys_net_socket_t` /
// `_z_sys_net_endpoint_t`, resolved per platform.
//
// Each `nros-platform-*` crate computes these from `core::mem::size_of`
// over its private `Socket` / `Endpoint` struct (which mirrors zenoh-pico's
// platform header). Re-exporting them here lets callers like
// `nros-rmw-dds::transport_nros` size their opaque buffers exactly via
// `nros_platform::NET_SOCKET_SIZE`, instead of paying for a `[u8; 64]`
// worst-case.
//
// Bare-metal platforms (`platform-mps2-an385`, `platform-stm32f4`,
// `platform-esp32`, `platform-esp32-qemu`, `platform-cffi`) don't yet
// have a typed socket struct exposed; callers there get a 64-byte
// fallback. Once the smoltcp platform crates publish their own
// `Socket` / `Endpoint` (Phase 71.26), they can plug in alongside the
// RTOS variants below.

#[cfg(feature = "platform-posix")]
pub use nros_platform_posix::net::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

#[cfg(feature = "platform-zephyr")]
pub use nros_platform_zephyr::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

#[cfg(feature = "platform-freertos")]
pub use nros_platform_freertos::net::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

#[cfg(feature = "platform-nuttx")]
pub use nros_platform_nuttx::net::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

#[cfg(feature = "platform-threadx")]
pub use nros_platform_threadx::net::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

// Phase 100 — SPE has no TCP/UDP at the platform level (IVC replaces
// them at the link layer). The platform crate publishes the same
// 64-byte fallback the other no-network platforms use, exposed as a
// crate-root constant rather than a `net::` submodule because there
// is no `net` module to host it on the SPE.
#[cfg(feature = "platform-orin-spe")]
pub use nros_platform_orin_spe::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};

#[cfg(any(
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32",
    feature = "platform-esp32-qemu",
    feature = "platform-cffi",
))]
mod fallback_net_sizes {
    pub const NET_SOCKET_SIZE: usize = 64;
    pub const NET_SOCKET_ALIGN: usize = 8;
    pub const NET_ENDPOINT_SIZE: usize = 64;
    pub const NET_ENDPOINT_ALIGN: usize = 8;
}

#[cfg(any(
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32",
    feature = "platform-esp32-qemu",
    feature = "platform-cffi",
))]
pub use fallback_net_sizes::{
    NET_ENDPOINT_ALIGN, NET_ENDPOINT_SIZE, NET_SOCKET_ALIGN, NET_SOCKET_SIZE,
};
