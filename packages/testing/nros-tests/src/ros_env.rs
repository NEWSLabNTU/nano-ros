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
    /// `rmw_zenoh_cpp` with a client-mode session pointed at `locator` (the
    /// pinned overlay in `build/rmw_zenoh_ws/` is sourced when present).
    Zenoh { locator: String },
    /// `rmw_fastrtps_cpp` on an explicit ROS domain (multicast discovery).
    FastRtps { domain_id: u8 },
    /// `rmw_cyclonedds_cpp` on an explicit ROS domain (RTPS/SPDP).
    Cyclonedds { domain_id: u8 },
}

impl Middleware {
    /// The default zenoh locator used across the interop suite.
    pub fn zenoh_default() -> Self {
        Middleware::Zenoh {
            locator: "tcp/127.0.0.1:7447".to_string(),
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
            Middleware::Zenoh { locator } => {
                let (snip, dir) = ros2::ros2_env_setup_with_locator(&self.distro, locator);
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
            Middleware::Zenoh { locator } => format!(
                "source /opt/ros/{distro}/setup.bash && \
                 [ -f /opt/nros-overlay/install/setup.bash ] && \
                 source /opt/nros-overlay/install/setup.bash; \
                 export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
                 export ZENOH_ROUTER_CONFIG_URI= && \
                 export NROS_LOCATOR={locator}"
            ),
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
        // The bridge sets domains itself; don't pin one via the env snippet.
        let cname = format!(
            "nros-bridge-{}-{}",
            std::process::id(),
            PEER_SEQ.fetch_add(1, Ordering::Relaxed)
        );
        let source = format!("source /opt/ros/{}/setup.bash", self.distro);
        let mut cmd = Command::new("docker");
        cmd.args([
            "run",
            "--rm",
            "--network",
            "host",
            "--init",
            "--name",
            &cname,
        ]);
        cmd.args([
            self.image().as_str(),
            "bash",
            "-lc",
            &format!("{source} && {inner}"),
        ]);
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
/// `examples/native/rust/<name>/target-ros-edition-<edition>/debug/<name>` —
/// produced by `just ros_editions build-e2e-fixtures <edition>`.
pub fn example_bin(name: &str, edition: &str) -> std::path::PathBuf {
    crate::project_root()
        .join("examples/native/rust")
        .join(name)
        .join(format!("target-ros-edition-{edition}/debug/{name}"))
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
        .env("RUST_LOG", "info");
    c
}

/// The ROS 2 edition the gated harness lanes target, from `NROS_ROS_EDITION`
/// (default `"jazzy"`). `just ros_editions ci <distro>` exports it so the same
/// tests run against any built edition image (jazzy, iron, …).
pub fn test_edition() -> String {
    std::env::var("NROS_ROS_EDITION").unwrap_or_else(|_| "jazzy".to_string())
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
}
