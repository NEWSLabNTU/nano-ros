//! Phase 228.E.2 fixture — multi-tier FreeRTOS Entry pkg.
//!
//! `system.toml` declares `[tiers.high]` + `[tiers.low]`, so `nros::main!()`
//! resolves a 2-tier table (rtos = `freertos` from `deploy = "freertos"`) and
//! emits `<Mps2An385>::run_tiers(TIERS, run_plan)` — the FreeRTOS per-tier entry
//! (228.E.2). Keep the Node-pkg rlibs alive against `--gc-sections`.

#![no_std]
#![no_main]

extern crate ctrl_pkg as _;
extern crate telem_pkg as _;
use panic_semihosting as _;

nros::main!(launch = "demo_bringup");
