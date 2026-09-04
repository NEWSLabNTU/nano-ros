//! phase-325 W2.4 — a STOCK PX4 consumer reads what nano-ros published.
//!
//! This is the whole point of the uORB backend, and the only assertion that
//! proves it. On uORB there is no serialization: `publisher_publish_raw` checks
//! `len >= meta->o_size` and hands the caller's bytes straight to `orb_publish`,
//! so the payload IS the PX4 struct. The interesting failure is therefore a
//! layout or size disagreement with PX4's `orb_metadata` — and a test where a
//! nano-ros subscriber reads a nano-ros publisher is satisfied *identically* by a
//! correct encoding and a broken one, because both ends share the bug.
//!
//! So the assertion is on PX4's own `listener` command, which knows nothing about
//! nano-ros. Its output carries the decoded field values, so a garbled layout
//! shows up as wrong values rather than as a passing test.
//!
//! (The deleted `px4_e2e` made exactly that mistake — it drove `nros_listener` +
//! `nros_talker`, two nano-ros modules, and asserted one logged `recv:`. Issue
//! 0356.)
//!
//! ## No build here
//!
//! Per CLAUDE.md the compile belongs to the build stage: run
//! `just px4 build-sitl-example` first. This test asserts the artifact exists and
//! runs it. A missing or wrong-module build fails LOUDLY with the command to fix
//! it — never a silent skip.

use std::{env, fs, path::PathBuf, time::Duration};

use px4_sitl_tests::Px4Sitl;

/// How long to wait for `listener` to print a sample. The module publishes at
/// 1 Hz and `listener` needs a fresh one, so this is generous by design —
/// tightening it buys nothing and makes the lane flaky under load.
const LISTEN_TIMEOUT: Duration = Duration::from_secs(30);

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize project root")
}

/// Resolve the prebuilt SITL tree, asserting it actually contains THIS example.
///
/// PX4 accepts exactly one `EXTERNAL_MODULES_LOCATION` per build and every build
/// writes to the same `build/px4_sitl_default`, so whichever root was built last
/// wins. Without the second check below, a tree built from
/// `just px4 build-sitl-cpp` (the register-check gate) would boot fine here and
/// fail at `nros_uorb_demo start` with a confusing "command not found" — the
/// stale-fixture class, wearing a runtime-bug costume.
fn prebuilt_sitl_dir() -> PathBuf {
    let px4_dir = env::var("PX4_AUTOPILOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| project_root().join("third-party/px4/PX4-Autopilot"));

    let build_dir = px4_dir.join("build/px4_sitl_default");
    let bin = build_dir.join("bin/px4");

    assert!(
        bin.is_file(),
        "PX4 SITL binary missing at {}\n\
         Build it first:\n    just px4 build-sitl-example",
        bin.display()
    );

    // Issue 1046 — assert on the BINARY's contents, not on the module directory.
    //
    // The directory check this replaced could not observe the case its own
    // message describes. `external_modules/modules/<name>/` and the
    // `bin/px4-<name>` shim both SURVIVE across builds; only the last root's
    // modules are linked into `bin/px4`. Measured 2026-09-04 on a tree whose
    // last build was `just px4 build-bridge-example`:
    //
    //     module            module dir   shim      in bin/px4
    //     nros_uorb_demo    present      present   0
    //     nros_uorb_bridge  present      present   8
    //
    // So the guard passed on exactly the tree it exists to reject, and the test
    // then died at `nros_uorb_demo start` — the confusing failure the guard was
    // written to replace. It checked an artifact that outlives the thing it
    // proxies for, which is issue 0196's class one input over.
    //
    // A byte scan, not `strings`: no subprocess, no PATH dependency, and it is
    // milliseconds on the ~59 MB binary. The needle is the module's COMMAND
    // name, which is what `nros_uorb_demo start` resolves and therefore the
    // thing whose absence is the actual failure.
    const MODULE: &str = "nros_uorb_demo";
    let linked = fs::read(&bin)
        .map(|bytes| {
            bytes
                .windows(MODULE.len())
                .any(|w| w == MODULE.as_bytes())
        })
        .unwrap_or(false);
    assert!(
        linked,
        "SITL tree at {} was built WITHOUT the uORB interop example: {} does \
         not contain `{}`.\n\
         PX4 takes one EXTERNAL_MODULES_LOCATION per build and they share this \
         build dir, so another root (e.g. `just px4 build-sitl-cpp` or \
         `build-bridge-example`) built last. Its module DIRECTORY and \
         `bin/px4-{}` shim survive that, so neither is evidence the module is \
         linked (issue 1046).\n\
         Rebuild with:\n    just px4 build-sitl-example",
        build_dir.display(),
        bin.display(),
        MODULE,
        MODULE
    );

    build_dir
}

#[test]
fn stock_px4_listener_reads_a_nano_ros_publication() {
    let build_dir = prebuilt_sitl_dir();

    let sitl = Px4Sitl::boot_in(&build_dir).expect("Px4Sitl::boot_in");

    sitl.shell("nros_uorb_demo start")
        .expect("start nros_uorb_demo");

    // Give the module one publish cycle (it runs at 1 Hz) before asking a
    // consumer to read one.
    std::thread::sleep(Duration::from_secs(2));

    // PX4's OWN command. `listener <topic> <count>` prints decoded fields.
    //
    // Px4Sitl::shell() spawns `bin/px4-listener` as a SEPARATE process and
    // returns ITS stdout — the output does not pass through the daemon's log
    // stream, so `wait_for_log` never sees it. (Measured: the first draft waited
    // on the log and timed out at 30s while the module was demonstrably
    // publishing 30 samples. The daemon's own INFO lines DO go to the log, which
    // is why the vehicle_status direction below still uses wait_for_log.)
    let listener_out = sitl
        .shell("listener debug_key_value 2")
        .expect("run stock listener");

    // Assert on the KEY, not merely on the topic name appearing: `listener`
    // prints "TOPIC: debug_key_value" even for an all-zero sample, so matching
    // that would pass against a module that published nothing. `key` is a
    // char[10] the module fills with "nros", and it round-trips correctly only
    // if the struct layout agreed byte for byte.
    assert!(
        listener_out.contains("nros"),
        "stock `listener debug_key_value` did not report a nano-ros sample.\n         === listener stdout ===\n{listener_out}\n         === SITL log snapshot ===\n{}\n=== end snapshot ===",
        sitl.log_snapshot()
    );

    let line = listener_out
        .lines()
        .find(|l| l.contains("nros"))
        .unwrap_or("<unmatched>")
        .trim()
        .to_string();

    eprintln!("stock PX4 listener observed: {line}");

    // The other direction: nano-ros reading a topic PX4's commander publishes.
    // Same property, opposite way, and equally not provable from a nano-ros peer.
    sitl.wait_for_log("recv vehicle_status:", LISTEN_TIMEOUT)
        .unwrap_or_else(|e| {
            panic!(
                "nano-ros never received PX4's vehicle_status: {e:?}\n\
                 === SITL log snapshot ===\n{}\n=== end snapshot ===",
                sitl.log_snapshot()
            )
        });

    // Drop(sitl) -> SIGTERM the process group, grace, SIGKILL.
}
