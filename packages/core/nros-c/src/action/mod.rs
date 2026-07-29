//! Action API for nros C API.
//!
//! Actions provide long-running task execution with feedback and cancellation.
//! This module implements both action servers and clients.
//!
//! The action server follows the same metadata-only init → executor registration
//! pattern as subscriptions and services:
//! 1. `nros_action_server_init()` stores metadata (name, type, callbacks)
//! 2. `nros_executor_add_action_server()` creates RMW entities and registers
//!    with the nros-node executor
//! 3. Operation functions delegate through `ActionServerRawHandle`

mod client;
mod common;
mod server;

pub use client::*;
pub use common::*;
pub use server::*;
