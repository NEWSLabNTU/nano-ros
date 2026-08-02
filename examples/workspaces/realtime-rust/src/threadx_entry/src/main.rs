//! Entry pkg for the RT-tiers Rust workspace on ThreadX Linux.
//!
//! phase-297 W5 (RFC-0053) — the tiers-on-ThreadX projection of
//! `ws-realtime-rust`. Same one-line `nros::main!(model = ...)` as the native /
//! nuttx siblings; `deploy = "threadx-linux"` (Cargo.toml) selects the
//! ThreadX-Linux board (`ThreadxLinux`, `Framework::OwnedSpin`), and the
//! `execution.tiers.*.threadx` sub-tables in the committed
//! `demo_bringup/config/system_model.yaml` flip the macro's generic OwnedSpin
//! arm onto `<ThreadxLinux>::run_tiers`:
//!   1. resolves `demo_bringup` via the workspace pkg-index,
//!   2. bakes the model's nodes (ctrl + telem) + tier table,
//!   3. opens the ONE zenoh session and spawns one ThreadX thread per tier
//!      over it — stacks come from the shared byte pool
//!      (`nros_threadx_create_task`, RFC-0053 Revision). `resolve_tiers`
//!      sorts descending by raw priority number (no per-RTOS inversion), so
//!      on ThreadX (lower = higher) the BOOT tier is `low` (`telem`, 100 ms)
//!      and `high` (`ctrl`, 10 ms) is chain-spawned after telem's setup,
//!   4. the nodes publish `/ctrl` + `/telem` for cross-process observers
//!      (tests/realtime_tiers_e2e.rs, `Proof::CounterRatio3x`).

// RFC-0052 / phase-296 R2 — canonical model bake path (see native_entry).
// `deploy = "..."` picks the board + its RTOS tier sub-table from the SAME
// committed `demo_bringup/config/system_model.yaml`.
nros::main!(model = "demo_bringup");
