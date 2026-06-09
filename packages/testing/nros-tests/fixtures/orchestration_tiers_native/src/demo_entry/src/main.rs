//! Phase 228.G fixture — multi-tier Entry pkg.
//!
//! `system.toml` declares `[tiers.high]` + `[tiers.low]`, so the
//! `nros::main!()` macro resolves a 2-tier table and emits
//! `<NativeBoard>::run_tiers(TIERS, run_plan)` (RFC-0032 §5) instead of the
//! single-tier `BoardEntry::run`.

nros::main!(launch = "demo_bringup");
