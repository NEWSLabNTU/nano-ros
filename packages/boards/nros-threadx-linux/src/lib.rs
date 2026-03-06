//! # nros-threadx-linux
//!
//! Board crate for running nros on Linux with ThreadX + NetX Duo.
//!
//! ThreadX runs as pthreads via its Linux simulation port, and NetX Duo
//! uses a raw-socket Linux driver for real Ethernet over TAP interfaces.
//! This mirrors the FreeRTOS board crate pattern but is simpler since we
//! have `std`.
//!
//! Users call [`run()`] with a closure that receives `&Config` and creates
//! an `Executor` for full API access (publishers, subscriptions, services,
//! actions, timers, callbacks).

mod config;
mod node;

pub use config::Config;
pub use node::{init_hardware, run};
