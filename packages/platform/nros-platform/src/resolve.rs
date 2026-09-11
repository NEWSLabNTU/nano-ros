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
// issue 1315 / phase-451 W4 — ONE arm, because all nine define the SAME type.
//
// These were nine separate `#[cfg(feature = "platform-<x>")]` blocks, each
// `pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;` — identical
// right-hand sides, mutually exclusive only by CONVENTION. Turning on two
// platform features at once is then E0428, "defined multiple times", and
// nothing in the tree did that until three crates became workspace members:
// `cargo check --workspace` UNIFIES features across every member, so one
// member wanting `platform-freertos` beside another wanting `platform-posix`
// broke a crate neither of them names.
//
// `any(...)` is not a widening: nine arms that produce one type ARE one arm,
// and writing them separately only bought a collision. A future platform whose
// ConcretePlatform is NOT `CffiPlatform` gets its own `cfg` with a
// `not(...)` on this one — which is the point at which the exclusivity becomes
// real and should be stated, rather than assumed nine times.
#[cfg(any(
    feature = "platform-posix",
    feature = "platform-cffi",
    feature = "platform-mps2-an385",
    feature = "platform-stm32f4",
    feature = "platform-esp32-qemu",
    feature = "platform-nuttx",
    feature = "platform-freertos",
    feature = "platform-threadx",
    feature = "platform-zephyr",
))]
pub type ConcretePlatform = nros_platform_cffi::CffiPlatform;

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
