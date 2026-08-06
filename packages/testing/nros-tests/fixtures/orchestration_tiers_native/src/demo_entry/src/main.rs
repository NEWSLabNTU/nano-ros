//! Phase 228.G fixture — multi-tier Entry pkg.
//!
//! `system.toml` declares `[tiers.high]` + `[tiers.low]`, so the
//! `nros::main!()` macro resolves a 2-tier table and emits
//! `<LinuxBoard>::run_tiers(TIERS, run_plan)` (RFC-0032 §5) instead of the
//! single-tier `BoardEntry::run`.
//!
//! issue 0438 — this MUST be the `launch =` arm. Tier MEMBERSHIP comes from the
//! node packages' `callback_groups`, which only the launch arm walks; the
//! SystemModel carries `execution.tiers` but no group->tier bindings. Under the
//! deprecated `model =` arm the membership map is empty by construction, the
//! two authored tiers collapse to one synthesized `default`, and the macro
//! emits the SINGLE-tier path — silently, which is what made this look like a
//! missing boot marker three layers away.

nros::main!(launch = "demo_bringup");
