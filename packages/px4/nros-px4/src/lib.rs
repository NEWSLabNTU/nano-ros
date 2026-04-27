//! Board-style entry point for hosting a nano-ros [`Executor`] inside a PX4
//! `ScheduledWorkItem`. Mirrors the `nros-mps2-an385::run` shape.

#![cfg_attr(not(feature = "std"), no_std)]

mod config;
mod run;

pub use config::Config;
pub use run::run;
