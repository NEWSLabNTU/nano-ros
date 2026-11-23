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
//! A second test asks the image's `/lc_alpha` and stock rclcpp's
//! `lifecycle_talker` (`/lc_talker`, the `lifecycle` demo package) the same
//! `ros2 lifecycle` questions, each on its own router, and compares the
//! answers: `list` in every primary state, a `set` the current state has no
//! transition for, and a raw `change_state` by id that the node's own machine
//! must refuse.
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
        ManagedProcess, RequireFixture, Rmw as FixtureRmw, XrceAgent, ZenohRouter,
        build_c_lifecycle_pair, build_cpp_lifecycle_pair, require_ros2, require_ros2_cyclonedds,
        require_ros2_dds, require_xrce_agent, require_zenohd, ros2_env_setup_with_locator,
    },
    ros2::{
        DEFAULT_ROS_DISTRO, is_ros2_package_available, lifecycle_get_state,
        ros2_env_setup_rmw_with_domain,
    },
    ros2_compat::{Compared, GraphSide, Implementation, Pair, only_lines_containing},
    unique_ros_domain_id,
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

/// One RMW's bus: how the image joins it and how a stock `ros2` reaches it.
struct Bus {
    rmw: FixtureRmw,
    /// The `ros2` CLI environment: ROS sourced, the RMW and its discovery.
    env: String,
    /// What the image is started with to reach the same bus.
    image_env: Vec<(&'static str, String)>,
    _router: Option<ZenohRouter>,
    _agent: Option<XrceAgent>,
    /// The zenoh peer's session config, which must outlive every `ros2`.
    _config: Option<tempfile::TempDir>,
}

impl Bus {
    fn new(rmw: FixtureRmw) -> Self {
        require_ros2();
        match rmw {
            FixtureRmw::Zenoh => {
                require_zenohd();
                let router = ZenohRouter::start_unique()
                    .unwrap_or_else(|e| nros_tests::unmet!("start a zenoh router: {e}"));
                let locator = router.locator();
                let (env, config) = ros2_env_setup_with_locator(DEFAULT_ROS_DISTRO, &locator);
                Bus {
                    rmw,
                    env,
                    image_env: vec![
                        ("NROS_LOCATOR", locator),
                        ("NROS_SESSION_MODE", "client".to_string()),
                    ],
                    _router: Some(router),
                    _agent: None,
                    _config: Some(config),
                }
            }
            FixtureRmw::Cyclonedds => {
                require_ros2_cyclonedds();
                let domain = unique_ros_domain_id();
                // The image dlopens the in-tree `libddsc`.
                let lib = nros_tests::project_root().join("build/install/lib");
                let ld = match std::env::var("LD_LIBRARY_PATH") {
                    Ok(old) if !old.is_empty() => format!("{}:{old}", lib.display()),
                    _ => lib.display().to_string(),
                };
                Bus {
                    rmw,
                    env: ros2_env_setup_rmw_with_domain(
                        DEFAULT_ROS_DISTRO,
                        "rmw_cyclonedds_cpp",
                        domain,
                    ),
                    image_env: vec![
                        ("ROS_DOMAIN_ID", domain.to_string()),
                        ("LD_LIBRARY_PATH", ld),
                    ],
                    _router: None,
                    _agent: None,
                    _config: None,
                }
            }
            FixtureRmw::Xrce => {
                require_xrce_agent();
                require_ros2_dds();
                let agent = XrceAgent::start_unique()
                    .unwrap_or_else(|e| nros_tests::unmet!("start an XRCE agent: {e}"));
                let addr = agent.addr();
                let domain = unique_ros_domain_id();
                Bus {
                    rmw,
                    env: ros2_env_setup_rmw_with_domain(
                        DEFAULT_ROS_DISTRO,
                        "rmw_fastrtps_cpp",
                        domain,
                    ),
                    image_env: vec![
                        ("NROS_LOCATOR", addr.clone()),
                        ("XRCE_AGENT_ADDR", addr),
                        ("ROS_DOMAIN_ID", domain.to_string()),
                    ],
                    _router: None,
                    _agent: Some(agent),
                    _config: None,
                }
            }
        }
    }

    /// How long one `ros2` call spins to discover the graph. A router answers
    /// at once; DDS discovery takes seconds.
    fn spin(&self) -> &'static str {
        match self.rmw {
            FixtureRmw::Zenoh => "0.1",
            _ => "2",
        }
    }

    /// How long a host subscriber gets to be discovered before a window
    /// starts.
    fn join(&self) -> Duration {
        match self.rmw {
            FixtureRmw::Zenoh => Duration::from_secs(2),
            _ => Duration::from_secs(5),
        }
    }

    fn start_image(&self, binary: &Path, label: &str) -> ManagedProcess {
        let mut cmd = Command::new(binary);
        // Long enough for every step below; the process is killed on drop.
        cmd.env("NROS_SPIN_MS", "180000");
        for (k, v) in &self.image_env {
            cmd.env(k, v);
        }
        if self.rmw == FixtureRmw::Cyclonedds {
            // The same loopback-pinned bus as the `ros2` peer (issue 1137).
            nros_tests::dds_isolation::apply_to_command(&mut cmd);
        }
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

    /// `ros2 <subcommand> --no-daemon` on this bus, stdout and stderr
    /// together. A call that did not exit 0 gets a last line saying how it
    /// ended and after how long — a deadline kill prints nothing of its own,
    /// and an empty failure names no cause.
    fn ros2(&self, subcommand: &str) -> String {
        let deadline = nros_tests::process::deadline(20);
        let script = format!("{} && {deadline} ros2 {subcommand} --no-daemon 2>&1", self.env);
        let started = Instant::now();
        let out = Command::new("bash")
            .args(["-c", &script])
            .output()
            .expect("spawn bash for ros2");
        let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
        if !out.status.success() {
            text.push_str(&format!(
                "\n[`ros2 {subcommand}` ended with {} after {:.1} s; 124 is the 20 s deadline]",
                out.status,
                started.elapsed().as_secs_f64()
            ));
        }
        text
    }

    /// Poll `ros2 <subcommand>` until `ready` accepts its output: discovery
    /// is not instantaneous.
    fn until(&self, subcommand: &str, timeout: Duration, ready: impl Fn(&str) -> bool) -> String {
        let end = Instant::now() + timeout;
        loop {
            let out = self.ros2(subcommand);
            if ready(&out) || Instant::now() >= end {
                return out;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn set(&self, image: &mut ManagedProcess, node: &str, transition: &str) {
        let out = self.ros2(&format!(
            "lifecycle set --spin-time {} {node} {transition}",
            self.spin()
        ));
        assert!(
            out.contains("Transitioning successful"),
            "`ros2 lifecycle set {node} {transition}` did not succeed:\n{out}"
        );
        let callback = format!("{} on_{transition}", &node[1..]);
        image
            .wait_for_output_pattern(&callback, Duration::from_secs(5))
            .unwrap_or_else(|e| panic!("`{transition}` on {node} never ran `{callback}`: {e}"));
    }

    /// `get` on `node` reads exactly `state`.
    fn assert_state(&self, node: &str, state: &str) {
        let out = self.until(
            &format!("lifecycle get --spin-time {} {node}", self.spin()),
            Duration::from_secs(20),
            |o| lifecycle_get_state(o) == Some(state),
        );
        assert_eq!(
            lifecycle_get_state(&out),
            Some(state),
            "`ros2 lifecycle get {node}` should read `{state}`, got:\n{out}"
        );
    }

    /// Echo `node`'s topic for [`QUIET`] and assert nothing arrived.
    fn assert_silent(&self, node: &str, why: &str) {
        let echo = Echo::start(self, node).expect("start ros2 topic echo");
        let topic = echo.topic.clone();
        std::thread::sleep(QUIET);
        let heard = echo.stop();
        assert!(
            heard.is_empty(),
            "{topic} must be silent {why}, but it carried: {heard:?}"
        );
    }
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

/// A host subscriber on `<node>/chatter`, whose output is read when it is
/// stopped.
struct Echo {
    child: Child,
    topic: String,
}

impl Echo {
    /// Start echoing `node`'s topic, then give the subscriber the bus's
    /// join time to be discovered before the caller acts.
    fn start(bus: &Bus, node: &str) -> TestResult<Self> {
        let topic = format!("{node}/chatter");
        let script = format!(
            "{} && exec ros2 topic echo {topic} std_msgs/msg/String 2>&1",
            bus.env
        );
        let child = Command::new("bash")
            .args(["-c", &script])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| nros_tests::TestError::ProcessFailed(format!("ros2 topic echo: {e}")))?;
        std::thread::sleep(bus.join());
        Ok(Self { child, topic })
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
            .map(|d| d.trim().trim_matches('\'').trim_matches('"').to_string())
            .collect()
    }
}

/// What `echo` printed once `node`'s topic carried `want` and at least one
/// timer count.
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

/// The image for `lang` on `rmw`, prebuilt.
fn pair_binary(lang: &str, rmw: FixtureRmw) -> &'static Path {
    let label = format!("{lang}-lifecycle-pair");
    let built = match lang {
        "c" => build_c_lifecycle_pair(rmw),
        _ => build_cpp_lifecycle_pair(rmw),
    };
    built.require(&label)
}

#[rstest]
#[case::c("c", FixtureRmw::Zenoh)]
#[case::cpp("cpp", FixtureRmw::Zenoh)]
#[case::c_cyclonedds("c", FixtureRmw::Cyclonedds)]
#[case::cpp_cyclonedds("cpp", FixtureRmw::Cyclonedds)]
#[case::c_xrce("c", FixtureRmw::Xrce)]
#[case::cpp_xrce("cpp", FixtureRmw::Xrce)]
fn two_managed_nodes_are_silent_until_each_is_active(
    #[case] lang: &str,
    #[case] rmw: FixtureRmw,
) {
    let bus = Bus::new(rmw);
    let binary = pair_binary(lang, rmw);
    let mut image = bus.start_image(binary, &format!("{lang}-lifecycle-pair-{rmw:?}"));
    let spin = bus.spin();

    // 1. exactly the two nodes, each under its own name.
    let both = |o: &str| names(o) == [ALPHA, BETA];
    let nodes = bus.until(
        &format!("lifecycle nodes --spin-time {spin}"),
        Duration::from_secs(30),
        both,
    );
    assert_eq!(
        names(&nodes),
        [ALPHA, BETA],
        "`ros2 lifecycle nodes` must list exactly the two nodes:\n{nodes}"
    );
    let graph = bus.until(
        &format!("node list --spin-time {spin}"),
        Duration::from_secs(20),
        both,
    );
    assert_eq!(
        names(&graph),
        [ALPHA, BETA],
        "`ros2 node list` must show only the two nodes:\n{graph}"
    );

    // 2. silent while unconfigured, and while inactive.
    bus.assert_state(ALPHA, "unconfigured");
    bus.assert_state(BETA, "unconfigured");
    bus.assert_silent(ALPHA, "while unconfigured");
    bus.set(&mut image, ALPHA, "configure");
    bus.assert_state(ALPHA, "inactive");
    bus.assert_silent(ALPHA, "while inactive");

    // 3. activate alpha alone: it publishes, beta does not move.
    let echo = Echo::start(&bus, ALPHA).expect("start ros2 topic echo");
    bus.set(&mut image, ALPHA, "activate");
    hears(echo, ALPHA, "activated");
    bus.assert_state(ALPHA, "active");
    bus.assert_state(BETA, "unconfigured");
    bus.assert_silent(BETA, "while unconfigured, beside an active node");

    // 4. deactivate silences alpha; the next activate resumes it.
    bus.set(&mut image, ALPHA, "deactivate");
    bus.assert_state(ALPHA, "inactive");
    bus.assert_silent(ALPHA, "after deactivate");
    let echo = Echo::start(&bus, ALPHA).expect("start ros2 topic echo");
    bus.set(&mut image, ALPHA, "activate");
    hears(echo, ALPHA, "activated");

    // 5. beta goes active on its own; deactivating it leaves alpha active.
    bus.set(&mut image, BETA, "configure");
    let echo = Echo::start(&bus, BETA).expect("start ros2 topic echo");
    bus.set(&mut image, BETA, "activate");
    hears(echo, BETA, "activated");
    bus.set(&mut image, BETA, "deactivate");
    bus.assert_state(BETA, "inactive");
    bus.assert_state(ALPHA, "active");
    let echo = Echo::start(&bus, ALPHA).expect("start ros2 topic echo");
    let heard = {
        std::thread::sleep(QUIET);
        echo.stop()
    };
    assert!(
        heard.iter().any(|d| d.starts_with("lc_alpha: ")),
        "{ALPHA} must keep publishing while {BETA} is deactivated, got: {heard:?}"
    );
    bus.assert_silent(BETA, "after deactivate, beside an active node");
}

/// The stock managed node the CLI answers are compared against.
const STOCK: &str = "/lc_talker";

/// Both sides' `get`, each polled until it reads `want` exactly, compared.
fn both_in(pair: &Pair, want: &str) {
    let get = pair.ask_until("lifecycle get {node}", Duration::from_secs(15), |a| {
        lifecycle_get_state(&a.output) == Some(want)
    });
    get.assert_same(&format!("`get` once both nodes are `{want}`"));
    assert_eq!(
        lifecycle_get_state(&get.nano.output),
        Some(want),
        "{}",
        get.report()
    );
}

/// One query on both sides, asked once each: a `set` or a service call
/// changes the node, so it is never repeated until it looks ready.
fn once(pair: &Pair, args: &str) -> Compared {
    pair.ask_until(args, Duration::from_secs(15), |_| true)
}

#[rstest]
#[case::c("c")]
#[case::cpp("cpp")]
fn lifecycle_cli_answers_as_stock_lifecycle_talker(#[case] lang: &str) {
    require_zenohd();
    require_ros2();
    if !is_ros2_package_available(DEFAULT_ROS_DISTRO, "lifecycle") {
        nros_tests::unmet!(
            "the stock ROS 2 package `lifecycle` is not installed (apt: \
             `ros-{DEFAULT_ROS_DISTRO}-lifecycle`); this test compares the image's managed \
             node against its `lifecycle_talker`"
        );
    }
    let binary = pair_binary(lang, FixtureRmw::Zenoh);
    let label = format!("{lang}-lifecycle-pair");

    let mut nano = GraphSide::new(Implementation::NanoRos, ALPHA);
    nano.spawn_nano_image_with_env(binary, &label, &[("NROS_SPIN_MS", "180000")]);
    nano.await_node();
    let mut stock = GraphSide::new(Implementation::Stock, STOCK);
    stock.spawn_stock("ros2 run lifecycle lifecycle_talker", "lifecycle_talker");
    stock.await_node();
    let pair = Pair::new(nano, stock);

    // unconfigured: what `list` offers, a `set` it does not offer, and a
    // raw `activate` (id 3) the node's machine refuses without moving.
    both_in(&pair, "unconfigured");
    pair.ask("lifecycle list {node}")
        .assert_same("`list` while unconfigured");
    once(&pair, "lifecycle set {node} activate")
        .assert_same("`set activate` while unconfigured, which has no such transition");
    once(
        &pair,
        "service call {node}/change_state lifecycle_msgs/srv/ChangeState '{transition: {id: 3}}'",
    )
    .filtered(only_lines_containing(&["ChangeState_Response"]))
    .assert_same("a raw `change_state` to id 3 (activate) while unconfigured");
    both_in(&pair, "unconfigured");

    // inactive
    once(&pair, "lifecycle set {node} configure").assert_same("`set configure`");
    both_in(&pair, "inactive");
    pair.ask("lifecycle list {node}")
        .assert_same("`list` while inactive");
    once(&pair, "lifecycle set {node} configure")
        .assert_same("`set configure` while inactive, which has no such transition");

    // active
    once(&pair, "lifecycle set {node} activate").assert_same("`set activate`");
    both_in(&pair, "active");
    pair.ask("lifecycle list {node}")
        .assert_same("`list` while active");
    once(&pair, "lifecycle set {node} cleanup")
        .assert_same("`set cleanup` while active, which has no such transition");

    // back down, and the last primary state.
    once(&pair, "lifecycle set {node} deactivate").assert_same("`set deactivate`");
    once(&pair, "lifecycle set {node} cleanup").assert_same("`set cleanup`");
    both_in(&pair, "unconfigured");
    once(&pair, "lifecycle set {node} shutdown").assert_same("`set shutdown`");
    both_in(&pair, "finalized");
    pair.ask("lifecycle list {node}")
        .assert_same("`list` once finalized");
}

// Bound to `interop::CELLS`; drift turns this red.
#[test]
fn cases_bound_to_interop_cells() {
    #[allow(unused_imports)]
    use nros_tests::matrix::{Lang::*, PlatformId::*, Rmw::*, Workload::*};
    nros_tests::interop::assert_test_bound(
        "lifecycle_pair_interop",
        &[
            (Linux, C, Zenoh, Lifecycle),
            (Linux, Cpp, Zenoh, Lifecycle),
            (Linux, C, Cyclonedds, Lifecycle),
            (Linux, Cpp, Cyclonedds, Lifecycle),
            (Linux, C, Xrce, Lifecycle),
            (Linux, Cpp, Xrce, Lifecycle),
        ],
    );
}
