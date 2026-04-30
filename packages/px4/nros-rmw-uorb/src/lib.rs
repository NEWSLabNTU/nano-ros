//! PX4 uORB RMW backend for nano-ros (Phase 99.L byte-shaped redesign).
//!
//! Implements [`nros_rmw::Rmw`] over [`px4_uorb`]'s byte-shaped
//! [`px4_uorb::RawPublication`] / [`px4_uorb::RawSubscription`].
//!
//! # Architecture (post-99.L)
//!
//! - **No registry, no `register::<T>`, no `topics.toml`.** Each
//!   publisher / subscriber stores a `&'static orb_metadata` directly
//!   and addresses uORB by metadata pointer + multi-instance index.
//! - **No alloc, no `critical_section`.** State lives inside the
//!   per-publisher / per-subscriber wrapper; lifecycle is bounded
//!   by Rust's borrow checker (publishers / subscribers live in the
//!   Node arena).
//! - **No public typed API.** All `T: UorbTopic` knowledge lives in
//!   the higher layer (`nros-px4::uorb`), which calls
//!   [`UorbSession::create_publisher_uorb`] /
//!   [`UorbSession::create_subscription_uorb`] via
//!   [`nros::Node::session_mut`].
//!
//! Service support is currently a stub — see `service.rs`.

#![cfg_attr(not(feature = "std"), no_std)]

mod publisher;
mod service;
mod session;
mod subscriber;

pub use publisher::UorbPublisher;
pub use service::{UorbServiceClient, UorbServiceServer};
pub use session::{UorbRmw, UorbSession};
pub use subscriber::UorbSubscriber;
