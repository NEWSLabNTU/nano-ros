//! PX4 SITL ↔ nano-ros XRCE-DDS companion end-to-end test (RFC-0039 Track B).
//!
//! Boots **real PX4 SITL** (headless `none` — no simulator needed), starts a
//! `MicroXRCEAgent`, has PX4's `uxrce_dds_client` connect to it, and runs the
//! `px4-probe` nano-ros node which subscribes `/fmu/out/timesync_status`. PX4
//! publishes timesync continuously regardless of EKF/sensor state, so this
//! proves the full path: **PX4 firmware → uxrce_dds_client → agent → nano-ros
//! `nros-rmw-xrce` subscriber** receives real PX4 telemetry.
//!
//! Complements the stub-based `nros-tests::px4_xrce` (which uses `px4-stub`,
//! not real PX4). Gated behind `just px4 test-sitl`; PANICS (no silent skip)
//! when preconditions are unmet, per CLAUDE.md.
//!
//! Preconditions: `just px4 setup` (PX4 tree + submodules + py deps),
//! `just build-xrce-agent` (or `nros setup … --rmw xrce`), and
//! `just px4 build-fixtures` (builds `px4-probe` to `target-xrce/`).

use std::{
    env,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

use px4_sitl_tests::Px4Sitl;

const RX_TIMEOUT: Duration = Duration::from_secs(30);
const AGENT_PORT: u16 = 8899;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../..")
        .canonicalize()
        .expect("canonicalize project root")
}

fn ensure_px4_autopilot_dir() -> PathBuf {
    let dir = env::var("PX4_AUTOPILOT_DIR")
        .expect("PX4_AUTOPILOT_DIR unset. Run via `just px4 test-sitl`.");
    let path = PathBuf::from(&dir);
    assert!(
        path.join("Tools").is_dir(),
        "PX4_AUTOPILOT_DIR={dir} is not a PX4 checkout (missing Tools/). Run `just px4 setup`."
    );
    path
}

/// Vanilla `make px4_sitl_default` — the XRCE companion path needs no
/// EXTERNAL_MODULES (it talks to PX4's stock `uxrce_dds_client`).
fn build_vanilla_sitl() -> PathBuf {
    let px4 = ensure_px4_autopilot_dir();
    let status = Command::new("make")
        .current_dir(&px4)
        .arg("px4_sitl_default")
        .status()
        .expect("spawn make px4_sitl_default");
    assert!(
        status.success(),
        "PX4 SITL build failed (exit {:?})",
        status.code()
    );
    let build_dir = px4.join("build/px4_sitl_default");
    assert!(
        build_dir.join("bin/px4").is_file(),
        "missing bin/px4 after build at {}",
        build_dir.display()
    );
    build_dir
}

fn agent_binary() -> PathBuf {
    let p = project_root().join("build/xrce-agent/MicroXRCEAgent");
    assert!(
        p.is_file(),
        "MicroXRCEAgent not found at {} — run `just build-xrce-agent`",
        p.display()
    );
    p
}

fn probe_binary() -> PathBuf {
    // Built by `just px4 build-fixtures` (nros-fast-release profile / target-xrce).
    let base = project_root().join("examples/px4/rust/xrce/px4-probe/target-xrce");
    for profile in ["nros-fast-release", "release", "debug"] {
        let p = base.join(profile).join("px4-probe");
        if p.is_file() {
            return p;
        }
    }
    panic!(
        "px4-probe binary not found under {} — run `just px4 build-fixtures`",
        base.display()
    );
}

struct Killable(Child);
impl Drop for Killable {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
fn px4_sitl_xrce_companion_receives_timesync() {
    let agent_bin = agent_binary();
    let probe_bin = probe_binary();
    let build_dir = build_vanilla_sitl();

    // 1. Agent up first so PX4's uxrce_dds_client connects on boot.
    let _agent = Killable(
        Command::new(&agent_bin)
            .args(["udp4", "-p", &AGENT_PORT.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn MicroXRCEAgent"),
    );
    std::thread::sleep(Duration::from_millis(500));

    // 2. Boot PX4 SITL.
    let sitl = Px4Sitl::boot_in(&build_dir).expect("Px4Sitl::boot_in");

    // 3. Ensure uxrce_dds_client is running against our agent (idempotent —
    //    the SITL rcS may already start it; restart it pointed at our port).
    let _ = sitl.shell("uxrce_dds_client stop");
    std::thread::sleep(Duration::from_millis(300));
    sitl.shell(&format!(
        "uxrce_dds_client start -t udp -h 127.0.0.1 -p {AGENT_PORT}"
    ))
    .expect("start uxrce_dds_client");

    // 4. Run the nano-ros probe against the same agent (PX4 default domain 0).
    let locator = format!("127.0.0.1:{AGENT_PORT}");
    let mut probe = Command::new(&probe_bin)
        .env("NROS_LOCATOR", &locator)
        .env("ROS_DOMAIN_ID", "0")
        .env("PX4_PROBE_MAX", "5")
        .env("RUST_LOG", "info")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn px4-probe");

    // 5. Assert the probe received ≥1 real PX4 timesync sample within budget.
    use std::io::{BufRead, BufReader};
    let stderr = probe.stderr.take().expect("probe stderr");
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            eprintln!("[probe] {line}");
            if line.contains("probe rx[") {
                let _ = tx.send(());
            }
        }
    });

    let received = rx.recv_timeout(RX_TIMEOUT).is_ok();
    let _ = probe.kill();
    let _ = probe.wait();

    if !received {
        eprintln!("=== SITL log snapshot ===\n{}", sitl.log_snapshot());
    }
    assert!(
        received,
        "px4-probe did not receive /fmu/out/timesync_status from PX4 SITL within {RX_TIMEOUT:?}"
    );
    eprintln!("nano-ros companion received real PX4 timesync_status over XRCE-DDS");
}
