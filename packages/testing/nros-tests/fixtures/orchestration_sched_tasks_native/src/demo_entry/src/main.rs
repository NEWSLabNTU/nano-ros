//! Phase 228.G fixture — multi-sched-task Entry pkg.
//!
//! `system.toml` declares `[tasks.high]` + `[tasks.low]`, so the
//! `nros::main!()` macro resolves a 2-sched-task table and emits
//! `<LinuxBoard>::run_sched_tasks(SCHED_TASKS, run_plan)` (RFC-0032 §5) instead of the
//! single-sched-task `BoardEntry::run`.
//!
//! issue 0438 — this MUST be the `launch =` arm. Sched task MEMBERSHIP comes from the
//! node packages' `callback_groups`, which only the launch arm walks; the
//! SystemModel carries `execution.tasks` but no group->sched task bindings. Under the
//! deprecated `model =` arm the membership map is empty by construction, the
//! two authored sched tasks collapse to one synthesized `default`, and the macro
//! emits the SINGLE-sched-task path — silently, which is what made this look like a
//! missing boot marker three layers away.

nros::main!(launch = "demo_bringup");
