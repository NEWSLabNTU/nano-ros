//! Concrete platform type alias resolved at compile time.
//!
//! When a platform feature is enabled, [`ConcretePlatform`] resolves to
//! the active backend. When no platform is selected (e.g., default workspace
//! build), the type is not defined — downstream crates that need it must
//! enable a platform feature.

// Phase 121.7.b / 121.8.e — uniform CffiPlatform routing for every
// platform that impls the full canonical surface (clock + alloc +
// sleep + yield + random + time + threading + net). POSIX, the four
// RTOS kernels, the four bare-metal embedded crates, and platform-cffi
// itself all route through CffiPlatform. Bare-metal net surface is
// backed by `nros_smoltcp::define_smoltcp_platform!` (PlatformTcp /
// Udp / SocketHelpers / UdpMulticast) emitted by each platform crate.
// orin-spe keeps its direct alias: it has no PlatformTcp/Udp impl
// (IVC replaces TCP/UDP at the link layer per Phase 100).
#[cfg(feature = "platform-posix")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-cffi")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-mps2-an385")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-stm32f4")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-esp32-qemu")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

// Phase 121.3.deprecate-rust-migrate: the four deprecated RTOS
// platforms resolve to `CffiPlatform` and reach their kernel impl
// through the deprecated Rust crate's `cffi-export` macro emission
// (enabled transitively by the `platform-<rtos>` feature). Same
// runtime behaviour as before — every Rust trait call now hops one
// extra `extern "C"` indirection. The deprecated Rust crates stay
// for that transitive emission until consumers move to a C-side
// symbol provider (`nros-platform-<rtos>-c`) and we drop the Rust
// kernel crates entirely.
#[cfg(feature = "platform-nuttx")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-freertos")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-threadx")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

#[cfg(feature = "platform-zephyr")]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

// Phase 121.10 — `platform-orin-spe` is now an alias for
// `platform-freertos` (see Cargo.toml). The board crate
// (`nros-board-orin-spe`) wires IVC + FSP init directly; no separate
// platform crate.

// ============================================================================
// Phase 71.22 — opaque-buffer sizes for `_z_sys_net_socket_t` /
// `_z_sys_net_endpoint_t`, resolved per platform.
//
// Each `nros-platform-*` crate computes these from `core::mem::size_of`
// over its private `Socket` / `Endpoint` struct (which mirrors zenoh-pico's
// platform header). Re-exporting them here lets callers like
// RMW transport adapters size their opaque buffers exactly via
// `nros_platform::NET_SOCKET_SIZE`, instead of paying for a `[u8; 64]`
// worst-case.
//
// Bare-metal platforms (`platform-mps2-an385`, `platform-stm32f4`,
// `platform-esp32`, `platform-esp32-qemu`, `platform-cffi`) don't yet
// have a typed socket struct exposed; callers there get a 64-byte
// fallback. Once the smoltcp platform crates publish their own
// `Socket` / `Endpoint` (Phase 71.26), they can plug in alongside the
// RTOS variants below.

// POSIX still publishes typed socket sizes (the only host-runnable
// platform crate left + the only one whose Socket / Endpoint layout
// varies meaningfully). Every other platform uses the 64-byte fallback
// — bare-metal smoltcp is 2 / 6 bytes; the RTOS C ports own the layout
// behind their `_z_sys_net_*` typedefs and can publish a tighter size
// later if the headroom matters.
// Phase 104.A.3 — POSIX net-size constants formerly re-exported from
// `nros_platform_posix::net::*`. Inlined here so `nros-platform`
// stops Rust-importing the concrete POSIX platform crate. The values
// mirror `nros-platform-posix/src/net.rs`:
//   * `Socket` = `{ int fd }` → `size_of::<c_int>() == 4` on every
//     POSIX ABI we target.
//   * `Endpoint` = `{ struct addrinfo* iptcp }` → native pointer
//     size (8 on 64-bit, 4 on 32-bit).
//
// Both expressible via `core::ffi` without pulling libc / the
// concrete platform crate. Phase 123's `nros-platform-posix`
// Rust-crate deletion adopts the same inline shape on the
// release-prep branch.
// Phase 129.C.3.b — exported unconditionally. Previously gated
// on a specific `platform-<rtos>` feature, which forced every
// RMW crate that imported them to
// forward a `nros-platform/platform-*` feature so the constants
// would resolve. Worst-case 64-byte / 8-aligned storage covers
// every supported platform — POSIX's `{ int fd }` socket and
// pointer endpoint, bare-metal smoltcp / lwIP / NetX handles
// alike. Consumers that want a tighter packing can opt into a
// per-platform `nros_platform_*` storage type at the link
// layer once that ABI lands.
pub const NET_SOCKET_SIZE: usize = 64;
pub const NET_SOCKET_ALIGN: usize = 8;
pub const NET_ENDPOINT_SIZE: usize = 64;
pub const NET_ENDPOINT_ALIGN: usize = 8;
