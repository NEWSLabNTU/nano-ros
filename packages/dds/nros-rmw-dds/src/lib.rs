//! DDS/RTPS RMW backend for nros.
//!
//! Uses [dust-dds](https://github.com/s2e-systems/dust-dds), a pure-Rust DDS
//! implementation with `no_std + alloc` support and OMG-certified RTPS
//! interoperability.
//!
//! This backend provides **brokerless peer-to-peer** discovery via standard
//! RTPS multicast — no router or agent process is needed.

#![cfg_attr(not(feature = "std"), no_std)]
extern crate alloc;

// Phase 101.4 — re-export dust-dds's `Arc` flavour so this crate
// stays in lockstep with dust-dds's `portable-atomic` feature.
// `transport_nros::write_message`'s `MpscSender<Arc<[u8]>>` boundary
// (and the matching `CacheChange::data_value`) requires the same
// `Arc<T>` flavour on both sides — `alloc::sync::Arc` and
// `portable_atomic_util::Arc` are ABI-incompatible. Routing through
// `dust_dds::sync` makes the choice transparent: when the
// `portable-atomic` feature lights up, every internal Arc here picks
// the polyfill automatically.
//
// Internal-only Arcs (e.g. `Arc<NrosPlatformRuntime>`, `Arc<WakerCell>`)
// don't strictly *need* to match dust-dds, but using one flavour
// crate-wide keeps the impl simple and avoids a second feature axis.
//
// Not gated on `feature = "alloc"` — `extern crate alloc` above is
// unconditional, and `subscriber.rs` / `waker_cell.rs` (compiled
// unconditionally) reference `crate::sync::Arc` in their struct fields.
pub(crate) mod sync {
    pub use dust_dds::sync::Arc;
    #[allow(unused_imports)]
    pub use dust_dds::sync::Weak;
}

// Make `std` resolvable at every call site of the `dbg_log!` macro
// (debug-stderr arm uses `std::println!`).  With `#[macro_export]`
// the macro is hygienic for variables but paths resolve at the call
// site — gating `extern crate std` inside `debug.rs` only made `std`
// visible in that module, which broke `dbg_log!` invocations from
// `transport_nros.rs` etc. when `feature = "std"` itself is off.
#[cfg(all(feature = "debug-stderr", not(feature = "std")))]
extern crate std;

#[cfg(feature = "alloc")]
#[macro_use]
mod debug;

mod publisher;
mod raw_type;
#[cfg(feature = "alloc")]
pub mod runtime;
mod service;
mod session;
mod subscriber;
mod transport;
#[cfg(feature = "alloc")]
pub mod transport_nros;
mod waker_cell;

pub use publisher::DdsPublisher;
pub use service::{DdsServiceClient, DdsServiceServer};
pub use session::DdsSession;
pub use subscriber::DdsSubscriber;
pub use transport::DdsRmw;
