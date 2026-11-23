//! phase-554 W9 -- two managed nodes in one image, driven by stock
//! `ros2 lifecycle`, in C and in C++.
//!
//! `examples/linux/{c,cpp}/lifecycle-pair` hold `/lc_alpha` and `/lc_beta`,
//! each with its own REP-2002 services and state machine. Each node publishes
//! `<node>: <n>` on `/<node>/chatter` from one shared timer, and
//! `<node>: activated` from its `on_activate`. This holds what a stock client
//! sees:
//!
//! 1. `ros2 lifecycle nodes` and `ros2 node list` show exactly the two nodes.
//! 2. **Silent until active:** nothing arrives on either topic while a node is
//!    `unconfigured` or `inactive`.
//! 3. `activate` on `/lc_alpha` runs its own callback, and its topic carries
//!    the `on_activate` publish and then the timer's counts. `/lc_beta` stays
//!    `unconfigured` and silent: the two machines are independent.
//! 4. `deactivate` silences `/lc_alpha`; the next `activate` resumes it.
//! 5. `/lc_beta` goes active on its own while `/lc_alpha` keeps publishing,
//!    and deactivating `/lc_beta` leaves `/lc_alpha` active.
//!
//! The C image drives each node through `rclc_node_register_lifecycle_services`
//! and `rclc_node_lifecycle_*`. Before those, the executor-level calls found
//! no machine once two nodes had one. The C++ image publishes from
//! `on_activate` through a `LifecyclePublisher`, which used to drop it.
//!
//! Build ahead of time with `just linux build-fixtures`; nothing compiles at
//! run time.

use nros_tests::{
    TestResult,
    fixtures::{
        ManagedProcess, RequireFixture, ZenohRouter, build_c_lifecycle_pair,
        build_cpp_lifecycle_pair, require_ros2, require_zenohd, ros2_env_setup_with_locator,
        zenohd_unique,
    },
    ros2::DEFAULT_ROS_DISTRO,
};
use rstest::rstest;
use std::{
    io::Read,
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

const ALPHA: &str = "/lc_alpha";
const BETA: &str = "/lc_beta";

/// How long a topic must stay quiet to count as silent: several timer
/// periods (200 ms) after the subscriber has joined.
const QUIET: Duration = Duration::from_secs(3);
/// How long the host subscriber gets to join before a window starts.
const JOIN: Duration = Duration::from_secs(2);

fn start_image(binary: &Path, label: &str, locator: &str) -> ManagedProcess {
    let mut cmd = Command::new(binary);
    cmd.env("NROS_LOCATOR", locator)
        .env("NROS_SESSION_MODE", "client")
        // Long enough for every step below; the process is killed on drop.
        .env("NROS_SPIN_MS", "180000");
    let mut image =
        ManagedProcess::spawn_image(cmd, label).expect("spawn the lifecycle-pair image");
    let boot = image
        .wait_for_output_pattern("Ready. Drive them", Duration::from_secs(15))
        .expect("the lifecycle-pair image never reached its ready line");
    for node in [ALPHA, BETA] {
        assert!(
            boot.contains(&format!("Lifecycle services registered on {node}")),
            "boot log lacks {node}'s registration line:\n{boot}"
        );
    }
    image
}

/// `ros2 <subcommand> --no-daemon` against `locator`, stdout and stderr
/// together.
fn ros2(locator: &str, subcommand: &str) -> String {
    let (env, _config_guard) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
    let deadline = nros_tests::process::deadline(10);
    let script = format!("{env} && {deadline} ros2 {subcommand} --no-daemon 2>&1");
    let out = Command::new("bash")
        .args(["-c", &script])
        .output()
        .expect("spawn bash for ros2");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Poll `ros2 <subcommand>` until its output holds `marker` (case-insensitive):
/// discovery through the router is not instantaneous.
fn until(locator: &str, subcommand: &str, marker: &str, timeout: Duration) -> String {
    let end = Instant::now() + timeout;
    let marker = marker.to_lowercase();
    let mut last = String::new();
    while Instant::now() < end {
        last = ros2(locator, subcommand);
        if last.to_lowercase().contains(&marker) {
            break;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    last
}

/// The `/…` lines of a `ros2 … list`-shaped output, sorted.
fn names(out: &str) -> Vec<&str> {
    let mut v: Vec<&str> = out
        .lines()
        .map(str::trim)
        .filter(|l| l.starts_with('/'))
        .collect();
    v.sort_unstable();
    v
}

fn set(image: &mut ManagedProcess, locator: &str, node: &str, transition: &str) {
    let out = ros2(
        locator,
        &format!("lifecycle set --spin-time 0.1 {node} {transition}"),
    );
    assert!(
        out.contains("Transitioning successful"),
        "`ros2 lifecycle set {node} {transition}` did not succeed:\n{out}"
    );
    let callback = format!("{} on_{transition}", &node[1..]);
    image
        .wait_for_output_pattern(&callback, Duration::from_secs(5))
        .unwrap_or_else(|e| panic!("`{transition}` on {node} never ran `{callback}`: {e}"));
}

/// `get` on `node` reads exactly `state` (`active` must not match `inactive`).
fn assert_state(locator: &str, node: &str, state: &str) {
    let out = until(
        locator,
        &format!("lifecycle get --spin-time 0.1 {node}"),
        state,
        Duration::from_secs(10),
    );
    assert!(
        out.trim().to_lowercase().starts_with(state),
        "`ros2 lifecycle get {node}` should read `{state}`, got:\n{out}"
    );
}

/// A host subscriber on `<node>/chatter`, whose output is read when it is
/// stopped.
struct Echo {
    child: Child,
    topic: String,
    /// The peer's session config, which must outlive the subscriber.
    _config: tempfile::TempDir,
}

impl Echo {
    /// Start echoing `node`'s topic, then give the subscriber [`JOIN`] to be
    /// discovered before the caller acts.
    fn start(locator: &str, node: &str) -> TestResult<Self> {
        let (env, config) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, locator);
        let topic = format!("{node}/chatter");
        let script = format!("{env} && exec ros2 topic echo {topic} std_msgs/msg/String 2>&1");
        let child = Command::new("bash")
            .args(["-c", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| nros_tests::TestError::ProcessFailed(format!("ros2 topic echo: {e}")))?;
        std::thread::sleep(JOIN);
        Ok(Self {
            child,
            topic,
            _config: config,
        })
    }

    /// Stop the subscriber and return the `data:` payloads it printed.
    fn stop(mut self) -> Vec<String> {
        let _ = self.child.kill();
        let mut out = String::new();
        if let Some(mut so) = self.child.stdout.take() {
            let _ = so.read_to_string(&mut out);
        }
        let _ = self.child.wait();
        out.lines()
            .filter_map(|l| l.trim().strip_prefix("data:"))
            .map(|d| d.trim().trim_matches('\'').trim_matches('\x22').to_string())
            .collect()
    }
}

/// Echo `node`'s topic for [`QUIET`] and assert nothing arrived.
fn assert_silent(locator: &str, node: &str, why: &str) {
    let echo = Echo::start(locator, node).expect("start ros2 topic echo");
    let topic = echo.topic.clone();
    std::thread::sleep(QUIET);
    let heard = echo.stop();
    assert!(
        heard.is_empty(),
        "{topic} must be silent {why}, but it carried: {heard:?}"
    );
}

/// What `echo` printed once `node`'s topic carried `want` and at least one
/// timer count, polled for up to 10 s.
fn hears(echo: Echo, node: &str, want: &str) -> Vec<String> {
    let name = &node[1..];
    // The echo subprocess's output is read when it stops, so wait the whole
    // window: enough for the activate publish and several timer periods.
    std::thread::sleep(QUIET);
    let topic = echo.topic.clone();
    let heard = echo.stop();
    assert!(
        heard.iter().any(|d| d == &format!("{name}: {want}")),
        "{topic} should carry `{name}: {want}`, got: {heard:?}"
    );
    assert!(
        heard.iter().any(|d| d
            .strip_prefix(&format!("{name}: "))
            .is_some_and(|n| n.parse::<u32>().is_ok())),
        "{topic} should carry the timer's counts once active, got: {heard:?}"
    );
    heard
}

#[rstest]
#[case::c("c-lifecycle-pair")]
#[case::cpp("cpp-lifecycle-pair")]
fn two_managed_nodes_are_silent_until_each_is_active(
    #[case] label: &str,
    zenohd_unique: ZenohRouter,
) {
    require_zenohd();
    require_ros2();
    let locator = zenohd_unique.locator();
    let built = match label {
        "c-lifecycle-pair" => build_c_lifecycle_pair(),
        _ => build_cpp_lifecycle_pair(),
    };
    let binary = built.require(label);
    let mut image = start_image(binary, label, &locator);

    // 1. exactly the two nodes, each under its own name.
    let nodes = until(
        &locator,
        "lifecycle nodes --spin-time 0.1",
        BETA,
        Duration::from_secs(15),
    );
    assert_eq!(
        names(&nodes),
        [ALPHA, BETA],
        "`ros2 lifecycle nodes` must list exactly the two nodes:\n{nodes}"
    );
    let graph = until(&locator, "node list", BETA, Duration::from_secs(10));
    assert_eq!(
        names(&graph),
        [ALPHA, BETA],
        "`ros2 node list` must show only the two nodes:\n{graph}"
    );

    // 2. silent while unconfigured, and while inactive.
    assert_state(&locator, ALPHA, "unconfigured");
    assert_state(&locator, BETA, "unconfigured");
    assert_silent(&locator, ALPHA, "while unconfigured");
    set(&mut image, &locator, ALPHA, "configure");
    assert_state(&locator, ALPHA, "inactive");
    assert_silent(&locator, ALPHA, "while inactive");

    // 3. activate alpha alone: it publishes, beta does not move.
    let echo = Echo::start(&locator, ALPHA).expect("start ros2 topic echo");
    set(&mut image, &locator, ALPHA, "activate");
    hears(echo, ALPHA, "activated");
    assert_state(&locator, ALPHA, "active");
    assert_state(&locator, BETA, "unconfigured");
    assert_silent(&locator, BETA, "while unconfigured, beside an active node");

    // 4. deactivate silences alpha; the next activate resumes it.
    set(&mut image, &locator, ALPHA, "deactivate");
    assert_state(&locator, ALPHA, "inactive");
    assert_silent(&locator, ALPHA, "after deactivate");
    let echo = Echo::start(&locator, ALPHA).expect("start ros2 topic echo");
    set(&mut image, &locator, ALPHA, "activate");
    hears(echo, ALPHA, "activated");

    // 5. beta goes active on its own; deactivating it leaves alpha active.
    set(&mut image, &locator, BETA, "configure");
    let echo = Echo::start(&locator, BETA).expect("start ros2 topic echo");
    set(&mut image, &locator, BETA, "activate");
    hears(echo, BETA, "activated");
    set(&mut image, &locator, BETA, "deactivate");
    assert_state(&locator, BETA, "inactive");
    assert_state(&locator, ALPHA, "active");
    let echo = Echo::start(&locator, ALPHA).expect("start ros2 topic echo");
    let heard = {
        std::thread::sleep(QUIET);
        echo.stop()
    };
    assert!(
        heard.iter().any(|d| d.starts_with("lc_alpha: ")),
        "{ALPHA} must keep publishing while {BETA} is deactivated, got: {heard:?}"
    );
    assert_silent(&locator, BETA, "after deactivate, beside an active node");
}

// Bound to `interop::CELLS`; drift turns this red.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "lifecycle_pair_interop",
        &[(Linux, C, Zenoh, Lifecycle), (Linux, Cpp, Zenoh, Lifecycle)],
    );
}
