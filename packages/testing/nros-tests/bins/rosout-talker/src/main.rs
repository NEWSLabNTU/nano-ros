//! `/rosout` live-peer fixture — phase-467 Q4, ledger row
//! `c:logging_rosout_enabled`.
//!
//! Logs through `nros_log` and drives `nros::rosout::pump`, so the question
//! "does a STOCK `ros2 topic echo /rosout` see a nano-ros node's logs?" can be
//! asked of a real ROS 2 peer instead of reasoned about. Consumed by
//! `packages/testing/nros-tests/tests/rosout_interop.rs`
//! (interop cell `native-logging-rust-zenoh-n2r`).
//!
//! Env in:
//! * `NROS_LOCATOR`          — the zenoh router to connect to. Required.
//! * `ROSOUT_PROBE_RECORDS`  — how many records to raise (default 60).
//! * `ROSOUT_PROBE_PERIOD_MS`— gap between records (default 250).
//! * `ROSOUT_PROBE_UPSTREAM_QOS` — set to `1` to use
//!   [`nros::rosout::qos`] (TRANSIENT_LOCAL, KEEP_LAST(1000)) instead of the
//!   default [`nros::rosout::qos_bounded`]. The test uses BOTH, because the
//!   affordable profile is the one that is not upstream's and the interesting
//!   question is whether a stock peer still sees it.
//!
//! Markers out (stdout, flushed — a test greps these):
//! * `ROSOUT_PROBE_READY`      the publisher exists and the sink is installed
//! * `ROSOUT_PROBE_PUMPED n`   `n` records have reached `publish` in total
//! * `ROSOUT_PROBE_SINK_FULL`  `nros_log`'s appendable sink list was full
//! * `ROSOUT_PROBE_PUBLISH_ERR` the transport refused

use std::io::Write as _;

use nros::prelude::*;
use nros_log::{Logger, log_info, log_warn};
use nros_rcl_interfaces::msg::Log;

static PROBE: Logger = Logger::new("rosout_probe");

fn env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

fn say(line: &str) {
    println!("{line}");
    let _ = std::io::stdout().flush();
}

fn main() {
    env_logger::init();
    nros_board_linux::register_linked_rmw();
    nros_log::register_logger(&PROBE);
    nros_log::init(nros_platform_cffi::log::default_sinks());

    let records = env_usize("ROSOUT_PROBE_RECORDS", 60);
    let period_ms = env_usize("ROSOUT_PROBE_PERIOD_MS", 250) as u64;
    let upstream_qos = std::env::var("ROSOUT_PROBE_UPSTREAM_QOS").as_deref() == Ok("1");

    let ctx = nros::init_with_launch_auto().expect("nros init failed");
    let cfg = ctx.config("rosout_talker");
    let mut executor: Executor = Executor::open(&cfg).expect("Failed to open session");

    let qos = if upstream_qos {
        nros::rosout::qos()
    } else {
        nros::rosout::qos_bounded()
    };
    let rosout = {
        let mut node = executor
            .create_node("rosout_talker")
            .expect("Failed to create node");
        node.create_publisher_with_qos::<Log>(nros::rosout::TOPIC, qos)
            .expect("Failed to create the /rosout publisher")
    };

    // AFTER the publisher: `enable` starts the queue filling, and a queue
    // filled before there is anywhere to drain it to just reaches its depth
    // and starts counting drops.
    if !nros::rosout::enable() {
        say("ROSOUT_PROBE_SINK_FULL");
        std::process::exit(4);
    }
    say("ROSOUT_PROBE_READY");

    let mut total = 0usize;
    for i in 0..records {
        log_info!(&PROBE, "nros rosout probe record {i}");
        // Every other record, not every tenth. The test's wait is a CONDITION
        // on the count of `record` lines, so it returns as soon as a handful
        // land; a WARN at `i % 10 == 9` was never inside that window and the
        // severity assertion failed on a run that had in fact delivered
        // everything it was asked for. The cadence and the wait are one
        // decision.
        if i % 2 == 1 {
            log_warn!(&PROBE, "nros rosout probe warning at {i}");
        }
        let _ = executor.spin_once(core::time::Duration::from_millis(period_ms));
        match nros::rosout::pump(&rosout) {
            Ok(n) => total += n,
            Err((n, e)) => {
                say(&format!("ROSOUT_PROBE_PUBLISH_ERR pumped={} {e:?}", total + n));
                std::process::exit(5);
            }
        }
        say(&format!("ROSOUT_PROBE_PUMPED {total}"));
    }

    // Report what the run cost, so a reader of the log knows whether the
    // queue depth was enough rather than inferring it from a short echo.
    say(&format!(
        "ROSOUT_PROBE_DONE pumped={total} dropped={} suppressed={}",
        nros::rosout::dropped(),
        nros::rosout::suppressed()
    ));
}
