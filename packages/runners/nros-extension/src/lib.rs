//! The SDK crate for nano-ros runner and command packages.
//!
//! A **runner** package drives a launcher, flasher or console for `nros run`,
//! `nros flash` and `nros monitor`; a **command** package adds a verb to
//! `nros` (RFC-0119). Both are ordinary executables that nros starts through a
//! small, versioned, process-based contract. This crate holds the Rust side of
//! that contract, so nano-ros's own runners, a user's runners and the core CLI
//! all speak it through the same types:
//!
//! | Module | Holds |
//! | --- | --- |
//! | [`protocol`] | the protocol versions this crate speaks, and the negotiation rule |
//! | [`plan`] | the RunPlan (RFC-0118 D3), its JSON form and its placeholders |
//! | [`describe`] | the `describe` reply and its builder |
//! | [`invocation`] | parsing a runner's own command line (`describe`, `run --plan …`) |
//! | [`console`] | the console pump and the `nros-runner: link <endpoint>` line |
//! | [`usb`] | USB VID:PID parsing and serial-port lookup |
//! | [`exit`] | the exit codes |
//!
//! A runner written in another language does not need this crate: it reads
//! and writes the same JSON.
//!
//! # A minimal runner
//!
//! ```no_run
//! use nros_extension::{
//!     describe::Describe, exit::RunnerExit, invocation::Invocation, plan::Verb,
//! };
//!
//! fn main() -> std::process::ExitCode {
//!     match Invocation::from_env() {
//!         Ok(Invocation::Describe) => {
//!             Describe::builder()
//!                 .verb(Verb::Run)
//!                 .need("elf")
//!                 .build()
//!                 .print();
//!             RunnerExit::Success.into()
//!         }
//!         Ok(Invocation::Verb { plan, .. }) => {
//!             let _plan = match nros_extension::plan::RunPlan::read(&plan) {
//!                 Ok(p) => p,
//!                 Err(e) => {
//!                     eprintln!("{e}");
//!                     return RunnerExit::RunnerFailed.into();
//!                 }
//!             };
//!             // ... start the target, pump its console ...
//!             RunnerExit::Success.into()
//!         }
//!         Err(e) => {
//!             eprintln!("{e}");
//!             RunnerExit::RunnerFailed.into()
//!         }
//!     }
//! }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod console;
pub mod describe;
pub mod exit;
pub mod invocation;
pub mod plan;
pub mod protocol;
pub mod usb;
