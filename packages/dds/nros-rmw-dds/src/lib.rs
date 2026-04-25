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

pub use publisher::DdsPublisher;
pub use service::{DdsServiceClient, DdsServiceServer};
pub use session::DdsSession;
pub use subscriber::DdsSubscriber;
pub use transport::DdsRmw;
