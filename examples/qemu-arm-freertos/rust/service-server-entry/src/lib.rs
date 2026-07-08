//! Phase 213.C follow-up — Entry-pkg lib companion.
//!
//! `nros::main!()` (Form 1) emits a call to
//! `::<this_crate>::register(runtime)` — the macro resolves the
//! current pkg name and dispatches to its lib crate's `register`
//! symbol. This Entry pkg is sibling to the `freertos_rs_service_server` Node pkg;
//! the Node pkg's `nros::node!` invocation already emits a
//! `pub fn register(runtime)`, so we just re-export it here.

#![no_std]

pub use freertos_rs_service_server::register;
