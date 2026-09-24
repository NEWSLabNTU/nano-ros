//! phase-467 W1 (issue 1471) -- how many contract-monitor rows one executor
//! accepts.
//!
//! The Autoware Safety Island bakes 14 rate rows (one per contracted publisher
//! carrying `min_rate_hz`) and no age rows, all on ONE executor. Before
//! phase-467 `MAX_MONITORS` was a literal 8, so the C++ install refused the
//! table and the generated setup returned -6 before creating a node. It is
//! now `NROS_EXECUTOR_MAX_MONITORS`, derived from the contract, and this
//! fixture is built with it STATED at 14 (its `examples/fixtures.toml` row):
//!
//! * `MC_ROWS=14` prints `mc: installed 14 monitor rows (cap 14)`;
//! * `MC_ROWS=15` prints `mc: install refused: <reason>`, where the reason is
//!   the runtime's own `MonitorTableFull` text naming the knob to raise.
//!
//! It goes through `Executor::try_set_monitor_tables`, the refusing installer,
//! which makes the same check `nros_cpp_install_monitors` makes. The table is
//! only installed, never spun: whether a row is WATCHED is the
//! contract-monitor twins' subject; this one is whether it is ACCEPTED.

use nros::{
    monitor::{MAX_MONITORS, MonitorSpec, PubMonitorCell},
    prelude::*,
};

const MAX_ROWS: usize = 15;

static CELLS: [PubMonitorCell; MAX_ROWS] = [const { PubMonitorCell::new() }; MAX_ROWS];

const TOPICS: [&str; MAX_ROWS] = [
    "/mc_0", "/mc_1", "/mc_2", "/mc_3", "/mc_4", "/mc_5", "/mc_6", "/mc_7", "/mc_8", "/mc_9",
    "/mc_10", "/mc_11", "/mc_12", "/mc_13", "/mc_14",
];
const FQNS: [&str; MAX_ROWS] = [
    "/mc/main/mc_0",
    "/mc/main/mc_1",
    "/mc/main/mc_2",
    "/mc/main/mc_3",
    "/mc/main/mc_4",
    "/mc/main/mc_5",
    "/mc/main/mc_6",
    "/mc/main/mc_7",
    "/mc/main/mc_8",
    "/mc/main/mc_9",
    "/mc/main/mc_10",
    "/mc/main/mc_11",
    "/mc/main/mc_12",
    "/mc/main/mc_13",
    "/mc/main/mc_14",
];

/// Fifteen distinct endpoints, so a row past the knob is a real row and not a
/// duplicate the executor could fold.
static ROWS: [MonitorSpec; MAX_ROWS] = {
    let mut i = 0;
    let mut rows = [MonitorSpec {
        topic: "",
        fqn: "",
        min_rate_hz_milli: 0,
        max_latency_ms: 0,
        cell: &CELLS[0],
    }; MAX_ROWS];
    while i < MAX_ROWS {
        rows[i] = MonitorSpec {
            topic: TOPICS[i],
            fqn: FQNS[i],
            min_rate_hz_milli: 10_000,
            max_latency_ms: 0,
            cell: &CELLS[i],
        };
        i += 1;
    }
    rows
};

fn main() {
    let n = std::env::var("MC_ROWS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(14)
        .min(MAX_ROWS);
    nros_rmw_zenoh::register().expect("register zenoh backend");
    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("mc");
    let mut executor: Executor = Executor::open(&cfg).expect("open session");
    match executor.try_set_monitor_tables(&ROWS[..n], &[]) {
        Ok(()) => println!("mc: installed {n} monitor rows (cap {MAX_MONITORS})"),
        Err(full) => {
            println!("mc: install refused: {full}");
            std::process::exit(2);
        }
    }
}
