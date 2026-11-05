//! Phase 228.E.2 fixture — multi-sched-task FreeRTOS Entry pkg.
//!
//! `system.toml` declares `[tasks.high]` + `[tasks.low]`, so `nros::main!()`
//! resolves a 2-sched-task table (rtos = `freertos` from `deploy = "freertos"`) and
//! emits `<Mps2An385>::run_sched_tasks(SCHED_TASKS, run_plan)` — the FreeRTOS per-sched-task entry
//! (228.E.2). Keep the Node-pkg rlibs alive against `--gc-sections`.

#![no_std]
#![no_main]

extern crate ctrl_pkg as _;
extern crate telem_pkg as _;

nros::main!(panic = "own", launch = "demo_bringup");
