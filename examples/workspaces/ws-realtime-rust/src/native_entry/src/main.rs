//! Entry pkg — boots the 2-tier realtime system on the native board.
//!
//! `system.toml` declares `[tiers.high]` + `[tiers.low]`, and the two node pkgs
//! map their callback groups to those tiers (`callback_groups` metadata). So the
//! `nros::main!()` macro resolves a 2-tier table and emits the multi-tier
//! `run_tiers` entry (RFC-0032 §5) — one POSIX-priority task per tier — instead of
//! the single-tier `run`. (On native, priorities are advisory; the tiers are real
//! priority tasks on an RTOS deploy.)

// RFC-0052 / phase-296 R2 — the CANONICAL bake path: the entry resolves the
// 2-tier system from the committed `demo_bringup/config/system_model.yaml`
// (a play_launch-resolved artifact) instead of re-parsing launch XML +
// system.toml. The `deploy = "native"` metadata (Cargo.toml) picks the board
// and the POSIX tier sub-table; the same model drives the nuttx/zephyr/riscv
// entries against their own RTOS sub-tables.
nros::main!(model = "demo_bringup");
