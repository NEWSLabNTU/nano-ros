//! Edition-parametric ROS 2 environment provider (RFC-0058 / phase-309).
//!
//! A host installs exactly one ROS 2 edition (humble → Ubuntu 22.04, jazzy →
//! 24.04; the apt trees do not coexist). To exercise nano-ros codegen + interop
//! against MORE than one edition, tests obtain a [`RosEnv`] for the edition they
//! want and run every ROS-touching step through it. Two backends sit behind the
//! trait:
//!
//! - [`HostRosEnv`] — sources `/opt/ros/<distro>/setup.bash` on the host. The
//!   DEFAULT edition (humble today). This is the mechanism the legacy
//!   [`crate::ros2`] helpers already use; `HostRosEnv` delegates to them, so it
//!   is a thin, behavior-preserving wrapper — existing host tests are untouched.
//! - [`DockerRosEnv`] — runs commands inside a locally-built
//!   `nano-ros-ros:<distro>` container (`--network host`). Used for EXTRA
//!   editions in opt-in lanes. Filled in over phase-309 W3+.
//!
//! **Test-only.** Nothing here is referenced by the product crates, the `nros`
//! CLI, or an example build. Users build nano-ros against their own host ROS.
//!
//! The one primitive every backend provides is [`RosEnv::shell`]: given an inner
//! shell command, return a [`Command`] that runs it inside the edition's ROS
//! environment. [`RosEnv::run`] / [`RosEnv::spawn`] are default methods over it,
//! so a backend only implements `shell` + `available`.

use std::{
    ffi::OsStr,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
};

/// Per-process counter for unique docker container names.
static PEER_SEQ: AtomicU64 = AtomicU64::new(0);

use crate::{
    TestError, TestResult,
    process::{kill_process_group, set_new_process_group},
    ros2,
};

/// Which ROS 2 middleware the environment selects. Mirrors the existing
/// `ros2::ros2_env_setup_*` variants so [`HostRosEnv`] can delegate.
#[derive(Debug, Clone)]
pub enum Middleware {
    /// `rmw_zenoh_cpp` with a client-mode session pointed at `locator`, from
    /// the distro install (`ros-<distro>-rmw-zenoh-cpp`). The
    /// `domain_id` scopes discovery — it is the FIRST segment of the rmw_zenoh
    /// keyexpr, so a peer and the nano-ros node MUST share it to match.
    Zenoh { locator: String, domain_id: u8 },
    /// `rmw_fastrtps_cpp` on an explicit ROS domain (multicast discovery).
    FastRtps { domain_id: u8 },
    /// `rmw_cyclonedds_cpp` on an explicit ROS domain (RTPS/SPDP).
    Cyclonedds { domain_id: u8 },
}

/// The zenoh locators `rmw_zenoh_cpp`'s own shipped session config already
/// targets, in the spellings this tree uses. Under `--network host` these name
/// the same endpoint as the router's default listen address, which is why a
/// container peer reaches the router with no config of its own.
pub const DEFAULT_ZENOH_LOCATORS: [&str; 2] = ["tcp/127.0.0.1:7447", "tcp/localhost:7447"];

/// Does `locator` match what the shipped RMW config already points at?
///
/// The question a backend has to answer before it can claim to honour a
/// locator: if the answer is yes, doing nothing IS honouring it.
pub fn is_default_zenoh_locator(locator: &str) -> bool {
    DEFAULT_ZENOH_LOCATORS.contains(&locator)
}

impl Middleware {
    /// The default zenoh locator used across the interop suite.
    pub fn zenoh_default() -> Self {
        Middleware::Zenoh {
            locator: "tcp/127.0.0.1:7447".to_string(),
            domain_id: 0,
        }
    }
}

/// A ROS 2 environment for a specific edition. See the module docs.
pub trait RosEnv {
    /// The ROS 2 distro this env targets (`"humble"`, `"jazzy"`, …).
    fn edition(&self) -> &str;

    /// Is this environment usable right now? Host: the distro sources and
    /// `ros2 --help` works. Docker: the image is built and `docker` is on PATH.
    /// Never panics — a test `skip!`s on `false`, never silently passes.
    fn available(&self) -> bool;

    /// Build a [`Command`] that runs `inner` (an arbitrary shell command, e.g.
    /// `"ros2 topic echo /pose geometry_msgs/msg/PoseStamped"`) inside this
    /// edition's sourced ROS environment. The middleware env vars are already
    /// exported when the command runs.
    fn shell(&self, inner: &str) -> Command;

    /// Run `inner` to completion, capturing stdout+stderr.
    fn run(&self, inner: &str) -> TestResult<std::process::Output> {
        self.shell(inner)
            .output()
            .map_err(|e| TestError::ProcessFailed(format!("[{}] run failed: {e}", self.edition())))
    }

    /// Run `inner` and return its stdout as a lossy `String` (stderr merged when
    /// the inner command redirects `2>&1`, matching the `ros2::ros2_*_list`
    /// helpers).
    fn run_text(&self, inner: &str) -> TestResult<String> {
        let out = self.run(inner)?;
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    }

    /// Spawn `inner` as a long-lived peer in its own process group. The returned
    /// [`RosPeer`] kills the whole group (and, for docker, the container) on
    /// drop — no orphan `ros2` daemons survive a test.
    fn spawn(&self, name: &str, inner: &str) -> TestResult<RosPeer> {
        spawn_command(self.shell(inner), name, None)
    }
}

/// Spawn a prepared [`Command`] as a [`RosPeer`] in its own process group, with
/// an optional `cleanup` command run on drop (e.g. `docker kill <name>` for the
/// docker backend, whose container outlives a killed `docker run` client).
fn spawn_command(
    mut cmd: Command,
    name: &str,
    cleanup: Option<Vec<String>>,
) -> TestResult<RosPeer> {
    let name = name.to_string();
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    set_new_process_group(&mut cmd);
    let handle = cmd
        .spawn()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to start {name}: {e}")))?;
    Ok(RosPeer {
        handle,
        name,
        cleanup,
    })
}

/// Spawn an already-configured [`Command`] (e.g. a host nano-ros publisher
/// binary with its env set) as a [`RosPeer`] — process-group isolated, killed on
/// drop. Lets an interop lane run a real nano-ros node as the publisher, not
/// only `ros2` CLI peers.
pub fn spawn_process(cmd: Command, name: &str) -> TestResult<RosPeer> {
    spawn_command(cmd, name, None)
}

/// A running ROS 2 peer (publisher, subscriber, server, `domain_bridge`, …).
/// Killed on drop. For the docker backend, killing the `docker run` client does
/// NOT stop the container — so a `cleanup` command (`docker kill <name>`) is run
/// on drop to tear the container down.
pub struct RosPeer {
    handle: std::process::Child,
    name: String,
    cleanup: Option<Vec<String>>,
}

impl RosPeer {
    /// Best-effort read of everything the peer printed within `timeout`.
    pub fn wait_for_output(&mut self, timeout: std::time::Duration) -> TestResult<String> {
        crate::ros2::wait_child_output(&mut self.handle, &self.name, timeout)
    }

    /// Still running?
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }

    /// Kill the peer's process group now, then run the backend cleanup (docker
    /// container teardown) if any. Cleanup is best-effort + idempotent.
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
        if let Some(argv) = self.cleanup.take()
            && let Some((prog, args)) = argv.split_first()
        {
            let _ = Command::new(prog)
                .args(args)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
        }
    }
}

impl Drop for RosPeer {
    fn drop(&mut self) {
        self.kill();
    }
}

// =============================================================================
// Host backend — the default edition, sourced from /opt/ros/<distro>.
// =============================================================================

/// Runs ROS commands against the host `/opt/ros/<distro>` install. Delegates to
/// the [`crate::ros2`] env-setup helpers so behavior matches the legacy path.
pub struct HostRosEnv {
    distro: String,
    mw: Middleware,
}

impl HostRosEnv {
    /// A host env for `distro` with the given middleware.
    pub fn new(distro: impl Into<String>, mw: Middleware) -> Self {
        Self {
            distro: distro.into(),
            mw,
        }
    }

    /// The default host env: the default distro over zenoh (the interop suite's
    /// common case).
    pub fn default_zenoh() -> Self {
        Self::new(ros2::DEFAULT_ROS_DISTRO, Middleware::zenoh_default())
    }

    /// The bash snippet that sources ROS + exports the middleware env. Returns
    /// the snippet plus an optional temp-dir guard (the zenoh session config)
    /// that must outlive any process reading it.
    fn env_snippet(&self) -> (String, Option<tempfile::TempDir>) {
        match &self.mw {
            // `domain_id`, NOT `..` — issue 0763. This match dropped it, so a
            // host zenoh peer ran on domain 0 whatever the cell asked for,
            // while the docker backend exported it: one `Middleware` value,
            // two different environments depending on the backend.
            Middleware::Zenoh { locator, domain_id } => {
                let (snip, dir) = ros2::ros2_env_setup_zenoh(&self.distro, locator, *domain_id);
                (snip, Some(dir))
            }
            Middleware::FastRtps { domain_id } => (
                ros2::ros2_env_setup_dds_with_domain(&self.distro, *domain_id),
                None,
            ),
            Middleware::Cyclonedds { domain_id } => (
                ros2::ros2_env_setup_cyclonedds_with_domain(&self.distro, *domain_id),
                None,
            ),
        }
    }
}

impl RosEnv for HostRosEnv {
    fn edition(&self) -> &str {
        &self.distro
    }

    fn available(&self) -> bool {
        ros2::is_ros2_distro_available(&self.distro)
    }

    fn shell(&self, inner: &str) -> Command {
        let (snippet, guard) = self.env_snippet();
        // Leak the zenoh-config temp dir into the command's lifetime by keeping
        // it in an env var path that the child reads; the snippet already points
        // `ZENOH_SESSION_CONFIG_URI` at it. We must NOT drop `guard` before the
        // child runs, so persist it (the file is small + in the OS temp dir).
        if let Some(dir) = guard {
            // `into_path` keeps the dir on disk for the process lifetime. Tests
            // are short-lived; the OS temp reaper cleans it. This mirrors the
            // legacy helpers, which hold the guard for the process duration.
            let _ = dir.keep();
        }
        let mut cmd = Command::new("bash");
        cmd.args([
            OsStr::new("-c"),
            OsStr::new(&format!("{snippet} && {inner}")),
        ]);
        cmd
    }
}

// =============================================================================
// Docker backend — extra editions, nano-ros-ros:<distro> (filled in W3).
// =============================================================================

/// Wrap a python script in a `timeout N python3 - <<'PYEOF' … PYEOF` heredoc so
/// its real newlines reach python (a `python3 -c '…\n…'` one-liner is a
/// SyntaxError). Used for the rclpy E2E servers below.
fn pyrun(script: &str, timeout_s: u32) -> String {
    format!("timeout {timeout_s} python3 - <<'NROS_PYEOF'\n{script}\nNROS_PYEOF")
}

/// rclpy `example_interfaces/AddTwoInts` server on `/add_two_ints` (verbatim from
/// the host `ros2.rs` helper — pinned to `example_interfaces`, the type nano-ros
/// service nodes use).
const RCLPY_ADD_TWO_INTS_SERVER: &str = r#"
import rclpy
from rclpy.node import Node
from example_interfaces.srv import AddTwoInts

class Server(Node):
    def __init__(self):
        super().__init__('add_two_ints_server')
        self.srv = self.create_service(AddTwoInts, '/add_two_ints', self.callback)
        print('SERVER READY', flush=True)
    def callback(self, request, response):
        response.sum = request.a + request.b
        print(f'Request: {request.a} + {request.b} = {response.sum}', flush=True)
        return response

rclpy.init()
node = Server()
rclpy.spin(node)
"#;

/// rclpy `example_interfaces/Fibonacci` action server on `/fibonacci`.
const RCLPY_FIBONACCI_SERVER: &str = r#"
import rclpy
from rclpy.node import Node
from rclpy.action import ActionServer
from example_interfaces.action import Fibonacci

class Server(Node):
    def __init__(self):
        super().__init__('fibonacci_action_server')
        self._srv = ActionServer(self, Fibonacci, '/fibonacci', self.execute)
        print('SERVER READY', flush=True)
    def execute(self, goal_handle):
        order = goal_handle.request.order
        seq = [0, 1]
        for i in range(1, order):
            seq.append(seq[i] + seq[i - 1])
            fb = Fibonacci.Feedback()
            fb.sequence = seq
            goal_handle.publish_feedback(fb)
        goal_handle.succeed()
        result = Fibonacci.Result()
        result.sequence = seq
        print(f'SERVER DONE {seq}', flush=True)
        return result

rclpy.init()
node = Server()
rclpy.spin(node)
"#;

/// Runs ROS commands inside a locally-built `nano-ros-ros:<distro>` container
/// (`docker run --network host`). Used for editions the host does not have.
pub struct DockerRosEnv {
    distro: String,
    mw: Middleware,
}

impl DockerRosEnv {
    /// A docker env for `distro` with the given middleware. The image is built
    /// by `just ros-edition-image <distro>` (phase-309 W2).
    pub fn new(distro: impl Into<String>, mw: Middleware) -> Self {
        Self {
            distro: distro.into(),
            mw,
        }
    }

    /// The image tag this backend runs.
    pub fn image(&self) -> String {
        format!("nano-ros-ros:{}", self.distro)
    }

    /// The bash snippet sourcing ROS + exporting middleware INSIDE the container
    /// (the pinned zenoh overlay is baked at `/opt/nros-overlay`).
    fn env_snippet(&self) -> String {
        let distro = &self.distro;
        match &self.mw {
            // Issue 0763 — the locator is the one `Middleware` field the two
            // backends still disagree about, and the disagreement is silent.
            //
            // The host backend WRITES a session config (`write_zenoh_session_config`)
            // overlaying `mode: client` + `connect.endpoints: [locator]` onto
            // what the RMW ships, and exports `ZENOH_SESSION_CONFIG_URI` — the
            // variable `rmw_zenoh_cpp` actually reads. This arm exports
            // `NROS_LOCATOR`, which is a NANO-ROS variable
            // (`nros::env::resolve_hosted`, `nros::main!`): no ROS 2 peer reads
            // it, and `rmw_zenohd` takes its endpoint from
            // `ZENOH_ROUTER_CONFIG_URI` (empty here = the shipped default,
            // which listens on 7447). So a container peer runs on the RMW's
            // default config no matter what locator the cell asked for.
            //
            // It has never mattered because the only docker zenoh lane pins the
            // default `tcp/127.0.0.1:7447` and runs serially against one router
            // — but `for_edition()` picks host OR docker for the SAME
            // `Middleware`, so a cell naming a non-default locator would have it
            // honoured on one backend and dropped on the other, with nothing
            // said.
            //
            // Refused rather than dropped. Honouring it means generating the
            // session config INSIDE the container from ITS OWN shipped
            // `DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5` — the host's copy is the
            // wrong base for exactly the case this backend exists for, a jazzy
            // container on a humble host (issue 0609's class: overlay what it
            // ships, never replace it) — and that needs a JSON5 parser in the
            // image. Worth doing when a lane needs it; not worth guessing at
            // now, and far better than a silent wrong environment.
            Middleware::Zenoh { locator, domain_id } => {
                assert!(
                    is_default_zenoh_locator(locator),
                    "DockerRosEnv cannot honour the zenoh locator `{locator}`: a container \
                     peer reads ZENOH_SESSION_CONFIG_URI, and this backend has no way to \
                     write one from the CONTAINER's shipped RMW config. Only the default \
                     ({DEFAULT_ZENOH_LOCATORS:?}) works, because the RMW's own config \
                     already targets it. Use HostRosEnv for a non-default locator, or \
                     implement in-container config generation — see the comment above. \
                     Refusing loudly beats the silent drop this used to do (issue 0763)."
                );
                format!(
                    "source /opt/ros/{distro}/setup.bash && \
                     [ -f /opt/nros-overlay/install/setup.bash ] && \
                     source /opt/nros-overlay/install/setup.bash; \
                     export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
                     export ROS_DOMAIN_ID={domain_id} && \
                     export ZENOH_ROUTER_CONFIG_URI= && \
                     export NROS_LOCATOR={locator}"
                )
            }
            Middleware::FastRtps { domain_id } => format!(
                "source /opt/ros/{distro}/setup.bash && \
                 export RMW_IMPLEMENTATION=rmw_fastrtps_cpp && \
                 export ROS_DOMAIN_ID={domain_id}"
            ),
            Middleware::Cyclonedds { domain_id } => format!(
                "source /opt/ros/{distro}/setup.bash && \
                 export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp && \
                 export ROS_DOMAIN_ID={domain_id}"
            ),
        }
    }
}

impl RosEnv for DockerRosEnv {
    fn edition(&self) -> &str {
        &self.distro
    }

    fn available(&self) -> bool {
        docker_available() && docker_image_present(&self.image())
    }

    fn shell(&self, inner: &str) -> Command {
        // One-shot (`run`/`run_text`): --rm auto-removes on completion.
        self.docker_run(inner, None)
    }

    fn spawn(&self, name: &str, inner: &str) -> TestResult<RosPeer> {
        // Long-lived peer: name the container so drop can `docker kill` it —
        // killing the `docker run` client alone leaves the container running.
        let cname = format!(
            "nros-peer-{}-{}",
            std::process::id(),
            PEER_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let cmd = self.docker_run(inner, Some(&cname));
        spawn_command(cmd, name, Some(vec!["docker".into(), "kill".into(), cname]))
    }
}

impl DockerRosEnv {
    /// **Codegen axis (phase-309 W4).** Run `nros generate-rust` INSIDE this
    /// edition's container, so the bindings are generated against the edition's
    /// own message definitions (`/opt/ros/<distro>/share/**`). The host-built
    /// `nros` binary is bind-mounted (glibc is backward-compatible: a 22.04
    /// binary runs on a 24.04 base), and `out_host` is bind-mounted at `/out`,
    /// so the generated tree lands on the host (typically an edition-scoped
    /// `generated-editions/<distro>/`, gitignored).
    ///
    /// `workdir_in_container` is the dir holding the `package.xml` to generate
    /// from (e.g. `/opt/ros/jazzy/share/std_msgs`). `extra_args` are appended to
    /// `nros generate-rust -o /out` (e.g. `["--ros-edition", "iron", "--force"]`).
    pub fn generate(
        &self,
        nros_bin: &std::path::Path,
        workdir_in_container: &str,
        out_host: &std::path::Path,
        extra_args: &[&str],
    ) -> TestResult<std::process::Output> {
        std::fs::create_dir_all(out_host)
            .map_err(|e| TestError::ProcessFailed(format!("mkdir {out_host:?}: {e}")))?;
        let inner = format!(
            "cd {workdir_in_container} && nros generate-rust -o /out {}",
            extra_args.join(" ")
        );
        let snippet = self.env_snippet();
        let mut cmd = Command::new("docker");
        cmd.args(["run", "--rm", "--network", "host", "--init"]);
        cmd.args([
            "-v",
            &format!("{}:/usr/local/bin/nros:ro", nros_bin.display()),
            "-v",
            &format!("{}:/out", out_host.display()),
        ]);
        cmd.args([
            self.image().as_str(),
            "bash",
            "-lc",
            &format!("{snippet} && {inner}"),
        ]);
        cmd.output().map_err(|e| {
            TestError::ProcessFailed(format!("[{}] generate failed: {e}", self.distro))
        })
    }

    /// **Testing axis (phase-309 W5).** Spawn a `domain_bridge` that republishes
    /// `topic` (`ros_type`, e.g. `geometry_msgs/msg/PoseStamped`) from
    /// `from_domain` to `to_domain` — the #0267 topology as reusable harness
    /// infra (supersedes the manual `scripts/ros/domain-bridge-repro.sh`). Runs
    /// in this edition's container; the per-topic domains in the generated YAML
    /// drive the bridge, independent of any exported `ROS_DOMAIN_ID`. RAII: the
    /// returned peer `docker kill`s the container on drop.
    pub fn spawn_domain_bridge(
        &self,
        topic: &str,
        ros_type: &str,
        from_domain: u8,
        to_domain: u8,
    ) -> TestResult<RosPeer> {
        let inner = format!(
            "cat > /tmp/nros_bridge.yaml <<'YAML'\n\
             name: nros_edition_bridge\n\
             topics:\n\
             \x20 {topic}:\n\
             \x20   type: {ros_type}\n\
             \x20   from_domain: {from_domain}\n\
             \x20   to_domain: {to_domain}\n\
             YAML\n\
             exec ros2 run domain_bridge domain_bridge /tmp/nros_bridge.yaml"
        );
        // Issue 0763's other half. This used to source the distro by hand,
        // reasoning that the bridge sets its own domains — true, and beside the
        // point: BOTH bridge endpoints are ordinary ROS nodes that must speak
        // the same RMW as the peers they bridge, and a hand-built setup line
        // left the bridge on the image DEFAULT (`rmw_fastrtps_cpp`) while both
        // callers run cyclone. The lane passed only because two RTPS vendors
        // happen to interoperate on a plain topic; over zenoh — no shared wire
        // at all — the same bypass is simply a bridge nobody can reach. The
        // domain the snippet exports is inert here, which is why routing
        // through it costs nothing: `domain_bridge` builds one context per
        // domain from the YAML above (`InitOptions::set_domain_id`), and an
        // explicit init option outranks `ROS_DOMAIN_ID`.
        let cname = format!(
            "nros-bridge-{}-{}",
            std::process::id(),
            PEER_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let cmd = self.docker_run(&inner, Some(&cname));
        spawn_command(
            cmd,
            "domain_bridge",
            Some(vec!["docker".into(), "kill".into(), cname]),
        )
    }

    // ---- phase-310 E2E peer helpers (direct same-domain cyclone) -----------
    // Construct the env with `Middleware::Cyclonedds { domain_id }`; every helper
    // then runs on that shared domain, so a host nano-ros cyclone node (via
    // `spawn_process`) and these container peers discover over RTPS.

    /// Spawn a `ros2 topic pub` publisher (long-lived). Pairs with a nano-ros
    /// subscriber (ROS → nano direction).
    pub fn spawn_topic_pub(
        &self,
        topic: &str,
        ros_type: &str,
        data: &str,
        rate: u32,
    ) -> TestResult<RosPeer> {
        self.spawn(
            "ros2-topic-pub",
            &format!("ros2 topic pub -r {rate} {topic} {ros_type} \"{data}\""),
        )
    }

    /// Run `ros2 topic echo --once` and return its output. Pairs with a nano-ros
    /// publisher (nano → ROS direction).
    pub fn echo_topic_once(
        &self,
        topic: &str,
        ros_type: &str,
        timeout_s: u32,
    ) -> TestResult<String> {
        self.run_text(&format!(
            "timeout {timeout_s} ros2 topic echo --once {topic} {ros_type} 2>&1"
        ))
    }

    /// Spawn the rclpy `example_interfaces/AddTwoInts` server on `/add_two_ints`.
    pub fn spawn_add_two_ints_server(&self) -> TestResult<RosPeer> {
        self.spawn("rclpy-add-two-ints", &pyrun(RCLPY_ADD_TWO_INTS_SERVER, 60))
    }

    /// Run `ros2 service call /add_two_ints` and return its output.
    pub fn service_call_add_two_ints(&self, a: i64, b: i64, timeout_s: u32) -> TestResult<String> {
        self.run_text(&format!(
            "timeout {timeout_s} ros2 service call /add_two_ints \
             example_interfaces/srv/AddTwoInts \"{{a: {a}, b: {b}}}\" 2>&1"
        ))
    }

    /// Spawn the rclpy `example_interfaces/Fibonacci` action server on `/fibonacci`.
    pub fn spawn_fibonacci_server(&self) -> TestResult<RosPeer> {
        self.spawn("rclpy-fibonacci", &pyrun(RCLPY_FIBONACCI_SERVER, 60))
    }

    /// Run `ros2 action send_goal --feedback /fibonacci` and return its output.
    pub fn action_send_goal_fibonacci(&self, order: u32, timeout_s: u32) -> TestResult<String> {
        self.run_text(&format!(
            "timeout {timeout_s} ros2 action send_goal --feedback /fibonacci \
             example_interfaces/action/Fibonacci \"{{order: {order}}}\" 2>&1"
        ))
    }

    /// Codegen golden variant (phase-309 W4): run `nros generate-rust` in-container
    /// against a HOST consumer directory (a dir with a `package.xml` that `<depend>`s
    /// the interface packages to generate), writing bindings to `out_host`. Unlike
    /// [`generate`], which points at an installed `share/<pkg>` dir (and so emits
    /// that manifest's DEPENDENCIES), a consumer manifest emits the DECLARED
    /// packages themselves — how the golden lane gets `geometry_msgs`.
    pub fn generate_from_consumer(
        &self,
        nros_bin: &std::path::Path,
        consumer_host: &std::path::Path,
        out_host: &std::path::Path,
    ) -> TestResult<std::process::Output> {
        std::fs::create_dir_all(out_host)
            .map_err(|e| TestError::ProcessFailed(format!("mkdir {out_host:?}: {e}")))?;
        let snippet = self.env_snippet();
        let mut cmd = Command::new("docker");
        cmd.args(["run", "--rm", "--network", "host", "--init"]);
        cmd.args([
            "-v",
            &format!("{}:/usr/local/bin/nros:ro", nros_bin.display()),
            "-v",
            &format!("{}:/consumer", consumer_host.display()),
            "-v",
            &format!("{}:/out", out_host.display()),
        ]);
        cmd.args([
            self.image().as_str(),
            "bash",
            "-lc",
            &format!("{snippet} && cd /consumer && nros generate-rust --force -o /out"),
        ]);
        cmd.output().map_err(|e| {
            TestError::ProcessFailed(format!(
                "[{}] generate_from_consumer failed: {e}",
                self.distro
            ))
        })
    }

    /// **phase-311 zenoh lane.** Spawn the ROS `rmw_zenohd` router in this
    /// edition's container (`--network host`, so it binds host `tcp/…:7447`).
    /// `rmw_zenoh_cpp` peers do NOT auto-connect to an arbitrary `zenohd` — they
    /// require THIS router — and the nano-ros zpico node reaches the same router
    /// via `NROS_LOCATOR=tcp/127.0.0.1:7447`. RAII: `docker kill`ed on drop.
    ///
    /// Proto version `0x09` is stable across zenoh 1.x, so the pinned zpico 1.7.2
    /// interoperates with a stock jazzy router (1.11.2); the interop key is the
    /// RIHS01 type-hash tail (baked by the `ros-<edition>` build), not the zenoh
    /// version — see issue #0291.
    pub fn spawn_zenoh_router(&self) -> TestResult<RosPeer> {
        let cname = format!(
            "nros-zenohd-{}-{}",
            std::process::id(),
            PEER_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        // The router goes through the same snippet as the peers it serves, not
        // a hand-built `source`: it must come from the install THEY resolve
        // `rmw_zenoh_cpp` out of, because router and RMW link one `libzenohc`
        // and a router from a different install is exactly the drift issue 0609
        // measured. Nothing the snippet exports can disturb a router — it joins
        // no domain and selects no RMW, so `ROS_DOMAIN_ID` /
        // `RMW_IMPLEMENTATION` pass it by — and the one export that DOES reach
        // it is the reason to want the snippet: `rmw_zenohd` reads
        // `ZENOH_ROUTER_CONFIG_URI`, an empty value reads as unset (it starts
        // on the shipped default config), and setting it empty is what stops an
        // inherited router config from silently reconfiguring the lane's router.
        let cmd = self.docker_run("exec ros2 run rmw_zenoh_cpp rmw_zenohd", Some(&cname));
        let peer = spawn_command(
            cmd,
            "rmw_zenohd",
            Some(vec!["docker".into(), "kill".into(), cname]),
        )?;
        // The container zenohd takes a few seconds to bind tcp/7447; a zpico node
        // or rmw_zenoh_cpp peer that connects before then silently fails to open
        // a session. Block until the router accepts a TCP connection (or give up
        // after ~20s and let the test's own assertion fail loudly).
        for _ in 0..40 {
            if std::net::TcpStream::connect_timeout(
                &"127.0.0.1:7447".parse().unwrap(),
                std::time::Duration::from_millis(300),
            )
            .is_ok()
            {
                return Ok(peer);
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Ok(peer)
    }

    /// Build a `docker run --rm --network host [--name <cname>] <image> bash -lc
    /// '<sourced inner>'` command.
    fn docker_run(&self, inner: &str, cname: Option<&str>) -> Command {
        let snippet = self.env_snippet();
        let mut cmd = Command::new("docker");
        cmd.args(["run", "--rm", "--network", "host", "--init"]);
        if let Some(name) = cname {
            cmd.args(["--name", name]);
        }
        cmd.args([
            self.image().as_str(),
            "bash",
            "-lc",
            &format!("{snippet} && {inner}"),
        ]);
        cmd
    }
}

/// phase-310 E2E lane guard: resolve the target edition, its per-edition example
/// binary, a fresh RTPS domain, and a docker env on it. `skip!`s (never a silent
/// pass) when the fixture, docker, or image is absent.
pub fn e2e_setup(example: &str) -> (DockerRosEnv, std::path::PathBuf, u8) {
    let ed = test_edition();
    let bin = example_bin(example, &ed);
    if !bin.is_file() {
        crate::skip!(
            "example `{example}` not built for {ed} — run `just ros_editions build-e2e-fixtures {ed}`"
        );
    }
    let domain = crate::unique_ros_domain_id();
    let env = DockerRosEnv::new(&ed, Middleware::Cyclonedds { domain_id: domain });
    if !env.available() {
        crate::skip!("{ed} image not built or docker absent — run `just ros_editions image {ed}`");
    }
    (env, bin, domain)
}

/// The per-edition cyclone build of native example `name` (phase-310), under
/// `examples/native/rust/<name>/target-ros-edition-<edition>-cyclone/debug/<name>`
/// — produced by `just ros_editions build-e2e-fixtures <edition>` (cyclone is the
/// default RMW). Delegates to [`example_bin_rmw`] so the cyclone lanes share the
/// phase-311 per-(edition, rmw) path convention.
pub fn example_bin(name: &str, edition: &str) -> std::path::PathBuf {
    example_bin_rmw(name, edition, Rmw::Cyclone)
}

/// A [`Command`] that runs a nano-ros example node `bin` on `domain` over the
/// CycloneDDS RMW, wrapped in `bash -c 'exec … 2>&1'` so the node's `env_logger`
/// output (STDERR) merges into the captured stdout — [`RosPeer::wait_for_output`]
/// reads stdout, and the nodes log their receive/result markers, not print them.
/// `extra_args` are appended (e.g. service-client summands).
pub fn nano_node_cmd(bin: &std::path::Path, domain: u8, extra_args: &[&str]) -> Command {
    let mut c = Command::new("bash");
    let args = if extra_args.is_empty() {
        String::new()
    } else {
        format!(" {}", extra_args.join(" "))
    };
    c.arg("-c")
        .arg(format!("exec {}{args} 2>&1", bin.display()))
        .env("ROS_DOMAIN_ID", domain.to_string())
        .env("RMW_IMPLEMENTATION", "rmw_cyclonedds_cpp")
        // Pin the nano-ros backend selector to cyclone's CANONICAL registry name
        // so an ambient `NROS_RMW` (e.g. the lane-selector `NROS_RMW=cyclone`
        // exported by `just ros_editions ci`) can't mis-resolve the backend and
        // fail `Executor::open` with `Transport(ConnectionFailed)`.
        .env("NROS_RMW", "cyclonedds")
        .env("RUST_LOG", "info");
    c
}

/// The ROS 2 edition the gated harness lanes target, from `NROS_ROS_EDITION`
/// (default `"jazzy"`). `just ros_editions ci <distro>` exports it so the same
/// tests run against any built edition image (jazzy, iron, …).
pub fn test_edition() -> String {
    std::env::var("NROS_ROS_EDITION").unwrap_or_else(|_| "jazzy".to_string())
}

/// The nano-ros RMW axis (phase-311), crossed with the edition axis. Selects how
/// a nano-ros example node reaches a live ROS 2 edition peer:
/// - `Cyclone` — nano-ros `rmw-cyclonedds` ↔ `rmw_cyclonedds_cpp` on a shared
///   RTPS domain (phase-310).
/// - `Zenoh` — nano-ros `rmw-zenoh` (zpico, pinned 1.7.2) ↔ a `rmw_zenohd`
///   router ↔ `rmw_zenoh_cpp`.
/// - `Xrce` — nano-ros `rmw-xrce` → a micro-XRCE Agent ↔ `rmw_fastrtps_cpp`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rmw {
    Cyclone,
    Zenoh,
    Xrce,
}

impl Rmw {
    /// The `NROS_RMW` token (`cyclone` | `zenoh` | `xrce`).
    pub fn as_str(&self) -> &'static str {
        match self {
            Rmw::Cyclone => "cyclone",
            Rmw::Zenoh => "zenoh",
            Rmw::Xrce => "xrce",
        }
    }

    /// The example-crate cargo feature that links this backend.
    pub fn cargo_feature(&self) -> &'static str {
        match self {
            Rmw::Cyclone => "rmw-cyclonedds",
            Rmw::Zenoh => "rmw-zenoh",
            Rmw::Xrce => "rmw-xrce",
        }
    }

    /// The canonical `NROS_RMW` backend selector the nano-ros node honors — the
    /// name the backend registers under (`nros_rmw_cffi_register_named`), NOT the
    /// short harness token [`as_str`]. Cyclone registers as `"cyclonedds"` (the
    /// token is `"cyclone"`); zenoh/xrce match their tokens. A wrong selector
    /// makes `Executor::open` fail `Transport(ConnectionFailed)` (the resolver
    /// finds no matching backend), so the node command sets this explicitly
    /// rather than inheriting an ambient `NROS_RMW`.
    ///
    /// [`as_str`]: Rmw::as_str
    pub fn nros_rmw_name(&self) -> &'static str {
        match self {
            Rmw::Cyclone => "cyclonedds",
            Rmw::Zenoh => "zenoh",
            Rmw::Xrce => "xrce",
        }
    }

    /// The `RMW_IMPLEMENTATION` the matching ROS 2 peer must run.
    pub fn ros_rmw_impl(&self) -> &'static str {
        match self {
            Rmw::Cyclone => "rmw_cyclonedds_cpp",
            Rmw::Zenoh => "rmw_zenoh_cpp",
            // XRCE bridges to DDS via the Agent; the ROS peer speaks fastrtps.
            Rmw::Xrce => "rmw_fastrtps_cpp",
        }
    }
}

/// The RMW the gated lanes target, from `NROS_RMW` (default `cyclone`).
/// `just ros_editions ci` exports it to sweep the RMW axis.
pub fn test_rmw() -> Rmw {
    match std::env::var("NROS_RMW").unwrap_or_default().as_str() {
        "zenoh" => Rmw::Zenoh,
        "xrce" => Rmw::Xrce,
        _ => Rmw::Cyclone,
    }
}

/// The per-(edition, rmw) build of native example `name` (phase-311), under
/// `target-ros-edition-<edition>-<rmw>/debug/<name>` — produced by
/// `just ros_editions build-e2e-fixtures <edition> <rmw>`.
pub fn example_bin_rmw(name: &str, edition: &str, rmw: Rmw) -> std::path::PathBuf {
    // issue 0488 residue 2 — one build root per (edition, rmw) under
    // `$NROS_BUILD_ROOT`, derived by the same `<kind>/<coordinate>` rule
    // `just ros_editions build-e2e-fixtures` builds with, rather than six
    // per-leaf `target-ros-edition-*/` dirs named by hand on both sides.
    crate::build_dir(
        crate::kind::ROS_EDITIONS,
        &[&format!("{edition}-{}", rmw.as_str())],
    )
    // profile-literal-ok: unprofiled: ros-edition fixtures are built with a plain `cargo build`
    .join("debug")
    .join(name)
}

/// A [`Command`] running nano-ros example `bin` over `rmw`, wrapped `bash -c
/// 'exec … 2>&1'` (env_logger → stderr → captured stdout). `domain` seeds the
/// cyclone RTPS domain; `locator` is the zenohd/agent endpoint for zenoh/xrce.
pub fn nano_node_cmd_rmw(
    bin: &std::path::Path,
    rmw: Rmw,
    domain: u8,
    locator: &str,
    extra_args: &[&str],
) -> Command {
    let mut c = Command::new("bash");
    let args = if extra_args.is_empty() {
        String::new()
    } else {
        format!(" {}", extra_args.join(" "))
    };
    c.arg("-c")
        .arg(format!("exec {}{args} 2>&1", bin.display()))
        .env("RUST_LOG", "info")
        // Pin the backend selector to the CANONICAL registry name (not the short
        // token) so selection is self-contained — never at the mercy of an
        // ambient `NROS_RMW`. Cyclone: `cyclonedds`; zenoh/xrce: same as token.
        .env("NROS_RMW", rmw.nros_rmw_name());
    match rmw {
        Rmw::Cyclone => {
            c.env("ROS_DOMAIN_ID", domain.to_string())
                .env("RMW_IMPLEMENTATION", "rmw_cyclonedds_cpp");
        }
        // zpico + XRCE clients reach their router/agent via NROS_LOCATOR; the
        // domain still scopes discovery where the backend honors it.
        Rmw::Zenoh | Rmw::Xrce => {
            c.env("NROS_LOCATOR", locator)
                .env("ROS_DOMAIN_ID", domain.to_string());
        }
    }
    c
}

/// Locate the host relocatable **micro-XRCE Agent** launcher — the phase-311
/// XRCE lane's DDS↔XRCE bridge. Provisioned into the nros SDK store by
/// `nros setup … --rmw xrce` at `~/.nros/sdk/xrce-agent/<ver>/bin/MicroXRCEAgent`
/// (a launcher that resolves its bundled Fast-DDS/fastcdr `.so`s next to itself,
/// so it runs on the host with no system Fast-DDS). `None` → the lane `skip!`s.
pub fn host_xrce_agent_bin() -> Option<std::path::PathBuf> {
    let home = std::env::var("NROS_HOME")
        .ok()
        .map(std::path::PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|h| std::path::PathBuf::from(h).join(".nros"))
        })?;
    let store = home.join("sdk/xrce-agent");
    let mut versions: Vec<_> = std::fs::read_dir(&store)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    versions.sort();
    versions
        .into_iter()
        .rev()
        .map(|v| v.join("bin/MicroXRCEAgent"))
        .find(|p| p.is_file())
}

/// Spawn the micro-XRCE Agent on the host as a UDP4 bridge on `port`, with its
/// Fast-DDS side scoped to ROS `domain`. The nano-ros `rmw-xrce` client reaches
/// it over UDP (`NROS_LOCATOR=127.0.0.1:<port>`); the Agent republishes onto the
/// DDS domain where a `rmw_fastrtps_cpp` container peer (same domain,
/// `--network host`) discovers it. RAII: killed on drop.
pub fn spawn_xrce_agent(agent: &std::path::Path, port: u16, domain: u8) -> TestResult<RosPeer> {
    let mut c = Command::new("bash");
    c.arg("-c")
        .arg(format!("exec {} udp4 -p {port} 2>&1", agent.display()))
        .env("ROS_DOMAIN_ID", domain.to_string());
    spawn_process(c, "xrce-agent")
}

/// phase-311 XRCE lane guard: resolve the target edition, its per-(edition, xrce)
/// example binary, a fresh RTPS/fastrtps domain, the host Agent launcher, and a
/// `rmw_fastrtps_cpp` docker env. `skip!`s (never a silent pass) when the
/// fixture, the Agent, docker, or the image is absent. Returns
/// `(fastrtps_env, xrce_bin, domain, agent_path, agent_port)`.
pub fn e2e_setup_xrce(
    example: &str,
) -> (
    DockerRosEnv,
    std::path::PathBuf,
    u8,
    std::path::PathBuf,
    u16,
) {
    let ed = test_edition();
    let bin = example_bin_rmw(example, &ed, Rmw::Xrce);
    if !bin.is_file() {
        crate::skip!(
            "example `{example}` not built for {ed}/xrce — run `just ros_editions build-e2e-fixtures {ed} xrce`"
        );
    }
    let agent = host_xrce_agent_bin().unwrap_or_else(|| {
        crate::skip!("micro-XRCE Agent not in the nros store — run `nros setup … --rmw xrce`")
    });
    let domain = crate::unique_ros_domain_id();
    // UDP port unique per-domain so parallel xrce lanes don't collide.
    let port = 8000 + domain as u16;
    let env = DockerRosEnv::new(&ed, Middleware::FastRtps { domain_id: domain });
    if !env.available() {
        crate::skip!("{ed} image not built or docker absent — run `just ros_editions image {ed}`");
    }
    (env, bin, domain, agent, port)
}

/// phase-311 zenoh lane guard: resolve the target edition, its per-(edition,
/// zenoh) example binary, a fresh domain, the shared `tcp/127.0.0.1:7447`
/// locator, and a `rmw_zenoh_cpp` docker env. Caller spawns the router with
/// [`DockerRosEnv::spawn_zenoh_router`]. `skip!`s (never a silent pass) when the
/// fixture, docker, or the image is absent. Returns
/// `(zenoh_env, zenoh_bin, domain, locator)`.
pub fn e2e_setup_zenoh(example: &str) -> (DockerRosEnv, std::path::PathBuf, u8, String) {
    let ed = test_edition();
    let bin = example_bin_rmw(example, &ed, Rmw::Zenoh);
    if !bin.is_file() {
        crate::skip!(
            "example `{example}` not built for {ed}/zenoh — run `just ros_editions build-e2e-fixtures {ed} zenoh`"
        );
    }
    // The native zpico zenoh node uses a COMPILE-TIME domain (0) — unlike the
    // cyclone path it does not read `ROS_DOMAIN_ID`/`NROS_DOMAIN_ID` at runtime,
    // and the domain is the FIRST keyexpr segment, so the `rmw_zenoh_cpp` peer
    // must run on domain 0 to match. The lane is serial anyway (one host router
    // on tcp/7447), so a fixed domain does not collide.
    let domain = 0u8;
    let locator = "tcp/127.0.0.1:7447".to_string();
    let env = DockerRosEnv::new(
        &ed,
        Middleware::Zenoh {
            locator: locator.clone(),
            domain_id: domain,
        },
    );
    if !env.available() {
        crate::skip!("{ed} image not built or docker absent — run `just ros_editions image {ed}`");
    }
    // Only editions that SHIP `rmw_zenoh_cpp` can be a zenoh peer (jazzy; iron
    // and humble have no official apt package). Skip loudly otherwise — never a
    // silent pass on an edition that structurally cannot run this lane.
    let has_zenoh = env
        .run_text("ros2 pkg prefix rmw_zenoh_cpp >/dev/null 2>&1 && echo OK")
        .map(|s| s.contains("OK"))
        .unwrap_or(false);
    if !has_zenoh {
        crate::skip!(
            "{ed} ships no rmw_zenoh_cpp (only jazzy does) — zenoh interop lane N/A for this edition"
        );
    }
    (env, bin, domain, locator)
}

/// Locate the host-built `nros` CLI binary, for bind-mounting into a codegen
/// container ([`DockerRosEnv::generate`]). Prefers the in-tree release build,
/// then `PATH`. `None` when neither is present (a codegen lane then `skip!`s).
pub fn host_nros_bin() -> Option<std::path::PathBuf> {
    let in_tree = crate::project_root().join("packages/cli/target/release/nros");
    if in_tree.is_file() {
        return Some(in_tree);
    }
    let out = Command::new("bash")
        .args(["-lc", "command -v nros"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!path.is_empty()).then(|| std::path::PathBuf::from(path))
}

/// Is `docker` on PATH and the daemon reachable?
pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("info")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Is a local docker image present (`docker image inspect <tag>` succeeds)?
pub fn docker_image_present(tag: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", tag])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Resolve the right backend for `distro`: the host install when it IS the host
/// distro and is available, otherwise a docker env. Returns a boxed trait object
/// so callers stay backend-agnostic.
pub fn for_edition(distro: &str, mw: Middleware) -> Box<dyn RosEnv> {
    let host = HostRosEnv::new(distro, mw.clone());
    if host.available() {
        Box::new(host)
    } else {
        Box::new(DockerRosEnv::new(distro, mw))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_shell_sources_distro_and_inner() {
        // No ROS required — we only inspect the composed command program/args.
        let env = HostRosEnv::new("humble", Middleware::FastRtps { domain_id: 7 });
        let cmd = env.shell("ros2 topic list");
        assert_eq!(cmd.get_program(), "bash");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "-c");
        assert!(args[1].contains("/opt/ros/humble/setup.bash"));
        assert!(args[1].contains("rmw_fastrtps_cpp"));
        assert!(args[1].contains("ROS_DOMAIN_ID=7"));
        assert!(args[1].trim_end().ends_with("ros2 topic list"));
    }

    #[test]
    fn docker_shell_runs_network_host_image() {
        let env = DockerRosEnv::new("jazzy", Middleware::Cyclonedds { domain_id: 2 });
        assert_eq!(env.image(), "nano-ros-ros:jazzy");
        let cmd = env.shell("ros2 topic list");
        assert_eq!(cmd.get_program(), "docker");
        let args: Vec<_> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.contains(&"--network".to_string()));
        assert!(args.contains(&"host".to_string()));
        assert!(args.contains(&"nano-ros-ros:jazzy".to_string()));
        let script = args.last().unwrap();
        assert!(script.contains("/opt/ros/jazzy/setup.bash"));
        assert!(script.contains("rmw_cyclonedds_cpp"));
        assert!(script.contains("ROS_DOMAIN_ID=2"));
        assert!(script.trim_end().ends_with("ros2 topic list"));
    }

    #[test]
    fn edition_falls_back_to_docker_when_host_absent() {
        // A distro the host cannot have resolves to the docker backend.
        let env = for_edition("no_such_distro_xyz", Middleware::zenoh_default());
        assert_eq!(env.edition(), "no_such_distro_xyz");
        // available() is false (no image built) — the skip contract, not a panic.
        assert!(!env.available());
    }

    /// Issue 0763 — the LOCATOR half of backend agreement.
    ///
    /// `both_backends_agree_on_middleware_and_domain` guards the middleware and
    /// the domain. The locator was the field it did not cover, and the one the
    /// backends still genuinely differ on: the host writes a session config and
    /// exports `ZENOH_SESSION_CONFIG_URI` (what `rmw_zenoh_cpp` reads), while
    /// docker exports `NROS_LOCATOR` — a nano-ros variable no ROS 2 peer reads.
    ///
    /// The rule asserted here is not "both do the same thing" but "neither
    /// silently does the wrong thing": for the DEFAULT locator both are correct
    /// (the RMW's shipped config already targets it), and for a non-default one
    /// docker REFUSES instead of dropping it.
    #[test]
    fn neither_backend_silently_drops_a_zenoh_locator() {
        let default_mw = Middleware::Zenoh {
            locator: "tcp/127.0.0.1:7447".to_string(),
            domain_id: 0,
        };
        // Host: the locator reaches the peer through a real session config.
        let host = HostRosEnv::new("humble", default_mw.clone())
            .env_snippet()
            .0;
        assert!(
            host.contains("ZENOH_SESSION_CONFIG_URI="),
            "the host backend must point the peer at a session config carrying the \
             locator — `rmw_zenoh_cpp` reads no other variable:\n{host}"
        );
        // Docker: the default is honoured by doing nothing, which is legitimate
        // ONLY because the shipped config already targets it.
        let docker = DockerRosEnv::new("humble", default_mw).env_snippet();
        assert!(
            docker.contains("rmw_zenoh_cpp"),
            "docker must still select the zenoh RMW:\n{docker}"
        );

        // A non-default locator must not be silently ignored.
        let odd = Middleware::Zenoh {
            locator: "tcp/127.0.0.1:7999".to_string(),
            domain_id: 0,
        };
        assert!(
            HostRosEnv::new("humble", odd.clone())
                .env_snippet()
                .0
                .contains("ZENOH_SESSION_CONFIG_URI="),
            "the host backend honours any locator — it writes the config itself"
        );
        let refused = std::panic::catch_unwind(|| DockerRosEnv::new("humble", odd).env_snippet());
        assert!(
            refused.is_err(),
            "DockerRosEnv must REFUSE a locator it cannot honour. Returning a snippet \
             that omits it is the silent-drop this issue is about: the cell asks for one \
             endpoint and the peer connects to another, with nothing said."
        );
    }

    /// Issue 0763 — the two backends must produce the SAME environment for the
    /// same `Middleware`, because that is the whole claim `RosEnv` makes.
    ///
    /// They did not. `Middleware::Zenoh` carries a `domain_id`; the docker
    /// backend exported it and the host backend destructured it away with
    /// `{ locator, .. }`, so a host zenoh peer ran on domain 0 whatever the
    /// cell asked for. Nothing caught it because each backend was only ever
    /// read on its own.
    ///
    /// The assertion is on the env vars that decide whether two peers can SEE
    /// each other — the middleware and the domain — not on the whole snippet:
    /// the backends legitimately differ in how they reach the install (host
    /// sources a distro, docker also sources an overlay) and in how the zenoh
    /// session is pointed at a router.
    #[test]
    fn both_backends_agree_on_middleware_and_domain() {
        for (mw, want_rmw, want_domain) in [
            (
                Middleware::Zenoh {
                    // The DEFAULT locator deliberately: this test is about the
                    // middleware and the domain, and `DockerRosEnv` refuses a
                    // locator it cannot honour rather than dropping it (see
                    // `neither_backend_silently_drops_a_zenoh_locator`). Using a
                    // non-default one here would be asserting the other rule by
                    // accident, and would fail on the refusal.
                    locator: "tcp/127.0.0.1:7447".to_string(),
                    domain_id: 42,
                },
                "rmw_zenoh_cpp",
                42,
            ),
            (Middleware::FastRtps { domain_id: 7 }, "rmw_fastrtps_cpp", 7),
            (
                Middleware::Cyclonedds { domain_id: 13 },
                "rmw_cyclonedds_cpp",
                13,
            ),
        ] {
            let host = HostRosEnv::new("humble", mw.clone()).env_snippet().0;
            let docker = DockerRosEnv::new("humble", mw.clone()).env_snippet();
            for (backend, snippet) in [("host", &host), ("docker", &docker)] {
                assert!(
                    snippet.contains(&format!("RMW_IMPLEMENTATION={want_rmw}")),
                    "{backend} backend does not select {want_rmw} for {mw:?}:\n{snippet}"
                );
                assert!(
                    snippet.contains(&format!("ROS_DOMAIN_ID={want_domain}")),
                    "{backend} backend drops the domain for {mw:?} — a peer and the \
                     nano-ros node must share it or they never discover each other, and \
                     ros2cli keys its daemon on this alone:\n{snippet}"
                );
            }
        }
    }
}
