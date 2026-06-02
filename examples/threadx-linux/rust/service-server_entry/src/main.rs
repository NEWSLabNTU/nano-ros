//! Phase 212.N.7 step-2 — ThreadX-Linux Rust service-server Entry pkg.

use nros_board_threadx_linux::ThreadxLinux;
use nros_platform::BoardEntry;

include!(concat!(env!("OUT_DIR"), "/run_plan.rs"));

fn main() {
    let outcome: Result<(), nros_platform::RuntimeError> =
        <ThreadxLinux as BoardEntry>::run(|runtime| run_plan(runtime));
    if let Err(err) = outcome {
        eprintln!("threadx_linux_rs_service_server_entry: run_plan failed: {err}");
        std::process::exit(1);
    }
}
