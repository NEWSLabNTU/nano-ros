//! Board-style entry point for hosting a nano-ros [`Executor`] inside a PX4
//! `ScheduledWorkItem`. Mirrors the `nros-mps2-an385::run` shape.

#![cfg_attr(not(feature = "std"), no_std)]

mod config;
mod run;
#[cfg(feature = "std")]
mod run_async;
pub mod uorb;

pub use config::Config;
pub use run::run;
#[cfg(feature = "std")]
pub use run_async::{pump, run_async};

#[cfg(all(feature = "std", any(test, feature = "test-helpers")))]
pub use run_async::pump_until;
