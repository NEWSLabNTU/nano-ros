//! Entry pkg — boots the 2-tier realtime system on the native board.
//!
//! `system.toml` declares `[tiers.high]` + `[tiers.low]`, and the two node pkgs
//! map their callback groups to those tiers (`callback_groups` metadata). So the
//! `nros::main!()` macro resolves a 2-tier table and emits the multi-tier
//! `run_tiers` entry (RFC-0032 §5) — one POSIX-priority task per tier — instead of
//! the single-tier `run`. (On native, priorities are advisory; the tiers are real
//! priority tasks on an RTOS deploy.)

nros::main!(launch = "demo_bringup");
