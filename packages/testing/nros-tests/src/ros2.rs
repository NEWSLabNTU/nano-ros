//! ROS 2 process fixtures for integration tests
//!
//! Provides helpers for running ROS 2 commands and processes.

use crate::{
    TestError, TestResult,
    process::{kill_process_group, set_new_process_group},
};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};

/// Default ROS 2 distro to use
pub const DEFAULT_ROS_DISTRO: &str = "humble";

/// Locate the pinned `rmw_zenoh_cpp` overlay built by `just rmw_zenoh setup`.
///
/// Returns the overlay's `setup.bash` path when the ament install is present,
/// allowing tests to source a zenoh RMW whose wire version matches our
/// pinned `zenoh-pico` / `zenohd`. When absent, callers should fall back to
/// the distro install (if any).
pub fn rmw_zenoh_overlay() -> Option<PathBuf> {
    let overlay = crate::project_root().join("build/rmw_zenoh_ws/install/setup.bash");
    overlay.exists().then_some(overlay)
}

/// Check if ROS 2 is available
pub fn is_ros2_available() -> bool {
    // Check if ros2 command exists by trying to get help
    Command::new("bash")
        .args(["-c", "source /opt/ros/humble/setup.bash && ros2 --help"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Require ROS 2 for a test (skips if not available)
///
/// Returns true if ROS 2 is available, false otherwise.
/// Prints a skip message when returning false.
pub fn require_ros2() -> bool {
    if !is_ros2_available() {
        eprintln!("Skipping test: ROS 2 not found");
        return false;
    }
    if !is_rmw_zenoh_available() {
        eprintln!("Skipping test: rmw_zenoh_cpp not found");
        return false;
    }
    true
}

/// Check if rmw_zenoh_cpp is available.
///
/// Prefers the pinned overlay built by `just rmw_zenoh setup`; falls back to
/// a distro-installed `rmw_zenoh_cpp` if the overlay is absent.
pub fn is_rmw_zenoh_available() -> bool {
    if rmw_zenoh_overlay().is_some() {
        return true;
    }
    Command::new("bash")
        .args([
            "-c",
            "source /opt/ros/humble/setup.bash && ros2 pkg list | grep -q rmw_zenoh_cpp",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Get ROS 2 environment setup command with default locator
pub fn ros2_env_setup(distro: &str) -> (String, tempfile::TempDir) {
    ros2_env_setup_with_locator(distro, "tcp/127.0.0.1:7447")
}

/// Write a zenoh session config file for rmw_zenoh_cpp and return a
/// [`tempfile::TempDir`] that keeps the file alive. The config is written to
/// `<tmpdir>/session_config.json5`.
///
/// The caller must hold the returned `TempDir` for the lifetime of the process
/// that reads the config — dropping it deletes the directory and file.
fn write_zenoh_session_config(locator: &str) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("failed to create temp dir for zenoh session config");
    let config_path = dir.path().join("session_config.json5");

    let config = format!(
        r#"{{
  mode: "client",
  connect: {{
    endpoints: ["{locator}"],
    exit_on_failure: {{ client: true }},
    timeout_ms: {{ client: 0 }},
  }},
  scouting: {{
    multicast: {{
      enabled: false,
    }},
  }},
}}"#
    );

    std::fs::write(&config_path, config).expect("failed to write zenoh session config");
    dir
}

/// Get ROS 2 environment setup command with custom locator.
///
/// Returns `(shell_snippet, _config_guard)`. The caller **must** hold the
/// returned [`tempfile::TempDir`] for the lifetime of the process that reads
/// the config — dropping it deletes the config file.
///
/// Stops the ROS 2 daemon first because it maintains its own zenoh session
/// connected to the default `tcp/localhost:7447`. If the daemon is running,
/// `ros2 topic list` queries the graph via XML-RPC through the daemon, which
/// ignores per-process zenoh config. Stopping the daemon forces the CLI to
/// create its own zenoh session using our custom locator.
///
/// Uses `ZENOH_SESSION_CONFIG_URI` to point rmw_zenoh_cpp at a JSON5 config
/// file with `mode: "client"` and the specified locator as the connect endpoint.
pub fn ros2_env_setup_with_locator(distro: &str, locator: &str) -> (String, tempfile::TempDir) {
    let config_dir = write_zenoh_session_config(locator);
    let config_path = config_dir.path().join("session_config.json5");
    // Source the pinned overlay on top of the distro setup so
    // rmw_zenoh_cpp comes from `build/rmw_zenoh_ws/install/` (wire-matched
    // to our zenoh-pico pin). Fall through to the distro install when the
    // overlay is missing.
    let overlay_snippet = match rmw_zenoh_overlay() {
        Some(path) => format!(" && source {}", path.display()),
        None => String::new(),
    };
    let cmd = format!(
        "source /opt/ros/{distro}/setup.bash{overlay_snippet} && \
         ros2 daemon stop >/dev/null 2>&1; \
         export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
         export ZENOH_SESSION_CONFIG_URI={config_path}",
        config_path = config_path.display()
    );
    (cmd, config_dir)
}

/// Managed ROS 2 process
///
/// Wraps a ROS 2 command with proper environment setup.
/// Automatically kills the process on drop.
///
/// Holds a [`tempfile::TempDir`] to keep the zenoh session config file alive
/// for the lifetime of the process.
pub struct Ros2Process {
    handle: Child,
    name: String,
    _config_dir: Option<tempfile::TempDir>,
}

impl Ros2Process {
    /// Spawn a bash command in its own process group.
    fn spawn_bash(
        cmd: &str,
        name: impl Into<String>,
        config_dir: Option<tempfile::TempDir>,
    ) -> TestResult<Self> {
        let name = name.into();
        let mut command = Command::new("bash");
        command
            .args(["-c", cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut command);
        let handle = command
            .spawn()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to start {name}: {e}")))?;
        Ok(Self {
            handle,
            name,
            _config_dir: config_dir,
        })
    }

    /// Start a ROS 2 topic echo subscriber
    ///
    /// # Arguments
    /// * `topic` - Topic name (e.g., "/chatter")
    /// * `msg_type` - Message type (e.g., "std_msgs/msg/Int32")
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn topic_echo(
        topic: &str,
        msg_type: &str,
        locator: &str,
        distro: &str,
    ) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic echo {topic} {msg_type} --qos-reliability best_effort"
        );

        Self::spawn_bash(&cmd, format!("ros2 topic echo {topic}"), Some(config_dir))
    }

    /// Start a ROS 2 action send_goal command
    ///
    /// # Arguments
    /// * `action_name` - Action name (e.g., "/demo/fibonacci")
    /// * `action_type` - Action type (e.g., "example_interfaces/action/Fibonacci")
    /// * `goal` - Goal data as YAML (e.g., "{order: 5}")
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn action_send_goal(
        action_name: &str,
        action_type: &str,
        goal: &str,
        locator: &str,
        distro: &str,
    ) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 15 ros2 action send_goal --feedback {action_name} {action_type} \"{goal}\""
        );

        Self::spawn_bash(
            &cmd,
            format!("ros2 action send_goal {action_name}"),
            Some(config_dir),
        )
    }

    /// Start a ROS 2 Fibonacci action server
    ///
    /// Uses the example_interfaces Fibonacci action server.
    /// Requires ros-humble-example-interfaces package.
    ///
    /// # Arguments
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn action_server_fibonacci(locator: &str, distro: &str) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        // Use ros2 run to start the action server from example_interfaces
        // Note: The standard action server example is in rclpy_action_server or similar
        // For testing, we use a simple Python one-liner that creates a Fibonacci server
        let cmd = format!(
            "{env_setup} && timeout 60 ros2 run action_tutorials_py fibonacci_action_server"
        );

        Self::spawn_bash(&cmd, "ros2 fibonacci_action_server", Some(config_dir))
    }

    /// Start a ROS 2 topic pub publisher
    ///
    /// # Arguments
    /// * `topic` - Topic name (e.g., "/chatter")
    /// * `msg_type` - Message type (e.g., "std_msgs/msg/Int32")
    /// * `data` - Message data as YAML (e.g., "{data: 42}")
    /// * `rate` - Publishing rate in Hz
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn topic_pub(
        topic: &str,
        msg_type: &str,
        data: &str,
        rate: u32,
        locator: &str,
        distro: &str,
    ) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic pub -r {rate} {topic} {msg_type} \"{data}\" --qos-reliability best_effort"
        );

        Self::spawn_bash(&cmd, format!("ros2 topic pub {topic}"), Some(config_dir))
    }

    /// Wait for output and return it
    pub fn wait_for_output(&mut self, timeout: Duration) -> TestResult<String> {
        use std::io::Read;
        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;

        let start = std::time::Instant::now();
        let mut output = String::new();

        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed(format!("No stdout for {}", self.name)))?;

        // Set non-blocking mode on stdout so read() doesn't block forever
        #[cfg(unix)]
        let fd = {
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
            fd
        };

        let mut buffer = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                kill_process_group(&mut self.handle);
                if output.is_empty() {
                    return Err(TestError::Timeout);
                }
                break;
            }

            match self.handle.try_wait() {
                Ok(Some(_)) => {
                    let _ = stdout.read_to_string(&mut output);
                    break;
                }
                Ok(None) => match stdout.read(&mut buffer) {
                    Ok(0) => {
                        Self::wait_for_data(
                            #[cfg(unix)]
                            fd,
                            timeout.saturating_sub(start.elapsed()),
                        );
                    }
                    Ok(n) => {
                        output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        Self::wait_for_data(
                            #[cfg(unix)]
                            fd,
                            timeout.saturating_sub(start.elapsed()),
                        );
                    }
                    Err(_) => break,
                },
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Wait for data on a file descriptor (or sleep on non-Unix).
    #[cfg(unix)]
    fn wait_for_data(fd: std::os::unix::io::RawFd, remaining: Duration) {
        let ms = remaining.as_millis().min(500) as i32;
        let mut fds = [libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        }];
        unsafe {
            libc::poll(fds.as_mut_ptr(), 1, ms);
        }
    }

    #[cfg(not(unix))]
    fn wait_for_data(remaining: Duration) {
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }

    /// Kill the process
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
    }

    /// Check if process is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }
}

impl Drop for Ros2Process {
    fn drop(&mut self) {
        self.kill();
    }
}

/// Helper to collect output from a process with timeout
pub fn collect_ros2_output(process: &mut Ros2Process, timeout: Duration) -> String {
    process.wait_for_output(timeout).unwrap_or_default()
}

// =============================================================================
// Discovery Helpers
// =============================================================================

/// Run `ros2 node list` and return the output
pub fn ros2_node_list(locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 node list 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 node list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 topic list` and return the output
pub fn ros2_topic_list(locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 topic list 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 topic list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 service list` and return the output
pub fn ros2_service_list(locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 service list 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 service list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 node info` for a specific node
pub fn ros2_node_info(node_name: &str, locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 node info {node_name} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 node info: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 param list` for a specific node
pub fn ros2_param_list(node_name: &str, locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 15 ros2 param list {node_name} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 param list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 param get` for a specific parameter on a node
pub fn ros2_param_get(
    node_name: &str,
    param_name: &str,
    locator: &str,
    distro: &str,
) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 15 ros2 param get {node_name} {param_name} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 param get: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 param set` to set a parameter on a node
pub fn ros2_param_set(
    node_name: &str,
    param_name: &str,
    value: &str,
    locator: &str,
    distro: &str,
) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd =
        format!("{env_setup} && timeout 15 ros2 param set {node_name} {param_name} {value} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 param set: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 param describe` for a specific parameter on a node
pub fn ros2_param_describe(
    node_name: &str,
    param_name: &str,
    locator: &str,
    distro: &str,
) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd =
        format!("{env_setup} && timeout 15 ros2 param describe {node_name} {param_name} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 param describe: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 topic info` for a specific topic
pub fn ros2_topic_info(topic: &str, locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 topic info {topic} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 topic info: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 topic hz <topic>` for a measurement window and return the captured
/// output. `ros2 topic hz` streams "average rate: X.YYY" lines roughly every
/// second; the helper times-out after `secs` seconds (caller picks 5–10 s for a
/// stable reading) and returns whatever the command printed so far.
///
/// Used by Phase 211.C to close the `<topic_list, topic_echo, topic_hz>` host
/// CLI interop trio — the first two were already covered, only `topic hz` was
/// missing.
pub fn ros2_topic_hz(topic: &str, secs: u64, locator: &str, distro: &str) -> TestResult<String> {
    let (env_setup, _config_dir) = ros2_env_setup_with_locator(distro, locator);
    // `ros2 topic hz` (Humble) only takes `--window / --filter / --wall-time /
    // --spin-time / -s`. `--no-daemon` + `--qos-reliability` are NOT valid here
    // (they belong on `lifecycle` and `topic echo` respectively); passing them
    // makes argparse hard-fail. `--spin-time` extends the discovery window so
    // the subscriber matches the rmw_zenoh talker before the timeout fires.
    let spin = (secs / 3).max(2);
    // Python's stdout is block-buffered when piped; `ros2 topic hz` prints
    // "average rate: …" lines that never reach our capture buffer before
    // `timeout` SIGTERMs the process. `stdbuf -oL` line-buffers stdout so each
    // averaged line is emitted as it's produced. `--wall-time` measures
    // against wall-clock (no /clock subscription needed for rmw_zenoh).
    let cmd = format!(
        "{env_setup} && timeout {secs} stdbuf -oL \
             ros2 topic hz --spin-time {spin} --wall-time {topic} 2>&1"
    );

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 topic hz: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

// =============================================================================
// Service Helpers
// =============================================================================

impl Ros2Process {
    /// Start a ROS 2 service call
    ///
    /// # Arguments
    /// * `service_name` - Service name (e.g., "/add_two_ints")
    /// * `service_type` - Service type (e.g., "example_interfaces/srv/AddTwoInts")
    /// * `request` - Request data as YAML (e.g., "{a: 5, b: 3}")
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn service_call(
        service_name: &str,
        service_type: &str,
        request: &str,
        locator: &str,
        distro: &str,
    ) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 service call {service_name} {service_type} \"{request}\""
        );

        Self::spawn_bash(
            &cmd,
            format!("ros2 service call {service_name}"),
            Some(config_dir),
        )
    }

    /// Start a ROS 2 service server (example_interfaces AddTwoInts)
    ///
    /// Uses a Python script to create a simple service server.
    /// The server responds with a + b for the AddTwoInts service.
    ///
    /// # Arguments
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn add_two_ints_server(locator: &str, distro: &str) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        // Use a Python one-liner to create a simple service server
        let python_script = r#"
import rclpy
from rclpy.node import Node
from example_interfaces.srv import AddTwoInts

class Server(Node):
    def __init__(self):
        super().__init__('add_two_ints_server')
        self.srv = self.create_service(AddTwoInts, '/add_two_ints', self.callback)
        self.get_logger().info('Service server ready')
    def callback(self, request, response):
        response.sum = request.a + request.b
        self.get_logger().info(f'Request: {request.a} + {request.b} = {response.sum}')
        return response

rclpy.init()
node = Server()
rclpy.spin(node)
"#;

        // Feed the script via a quoted heredoc so its real newlines reach
        // python. `python3 -c '<one line with \n literals>'` is a SyntaxError —
        // the `\n` is not a newline outside a string — so the server never
        // started and the reverse-direction interop tests timed out.
        let cmd = format!(
            "{env_setup} && timeout 60 python3 - <<'NROS_PYEOF'\n{python_script}\nNROS_PYEOF"
        );

        Self::spawn_bash(&cmd, "ros2 add_two_ints_server", Some(config_dir))
    }

    /// Start a ROS 2 topic echo subscriber with custom QoS
    ///
    /// # Arguments
    /// * `topic` - Topic name (e.g., "/chatter")
    /// * `msg_type` - Message type (e.g., "std_msgs/msg/Int32")
    /// * `reliability` - QoS reliability ("reliable" or "best_effort")
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn topic_echo_with_qos(
        topic: &str,
        msg_type: &str,
        reliability: &str,
        locator: &str,
        distro: &str,
    ) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic echo {topic} {msg_type} --qos-reliability {reliability}"
        );

        Self::spawn_bash(
            &cmd,
            format!("ros2 topic echo {topic} ({reliability})"),
            Some(config_dir),
        )
    }

    /// Start a ROS 2 topic pub publisher with custom QoS
    ///
    /// # Arguments
    /// * `topic` - Topic name (e.g., "/chatter")
    /// * `msg_type` - Message type (e.g., "std_msgs/msg/Int32")
    /// * `data` - Message data as YAML (e.g., "{data: 42}")
    /// * `rate` - Publishing rate in Hz
    /// * `reliability` - QoS reliability ("reliable" or "best_effort")
    /// * `locator` - Zenoh locator (e.g., "tcp/127.0.0.1:7447")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn topic_pub_with_qos(
        topic: &str,
        msg_type: &str,
        data: &str,
        rate: u32,
        reliability: &str,
        locator: &str,
        distro: &str,
    ) -> TestResult<Self> {
        let (env_setup, config_dir) = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic pub -r {rate} {topic} {msg_type} \"{data}\" --qos-reliability {reliability}"
        );

        Self::spawn_bash(
            &cmd,
            format!("ros2 topic pub {topic} ({reliability})"),
            Some(config_dir),
        )
    }
}

// =============================================================================
// DDS (rmw_fastrtps_cpp) Helpers — for XRCE-DDS ↔ ROS 2 interop tests
// =============================================================================

/// Check if rmw_fastrtps_cpp is available (default RMW in Humble)
pub fn is_rmw_fastrtps_available() -> bool {
    Command::new("bash")
        .args([
            "-c",
            "source /opt/ros/humble/setup.bash && ros2 pkg list | grep -q rmw_fastrtps_cpp",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Require ROS 2 with DDS (rmw_fastrtps_cpp) for a test.
///
/// Returns true if both ROS 2 and rmw_fastrtps_cpp are available.
/// Prints a skip message and returns false otherwise.
pub fn require_ros2_dds() -> bool {
    if !is_ros2_available() {
        eprintln!("Skipping test: ROS 2 not found");
        return false;
    }
    if !is_rmw_fastrtps_available() {
        eprintln!("Skipping test: rmw_fastrtps_cpp not found");
        return false;
    }
    true
}

/// Get ROS 2 environment setup command for DDS (rmw_fastrtps_cpp).
///
/// Unlike the zenoh variant, no locator or zenoh config is needed —
/// DDS uses multicast discovery on the local network.
pub fn ros2_env_setup_dds(distro: &str) -> String {
    ros2_env_setup_dds_with_domain(distro, 0)
}

/// Get ROS 2 environment setup command for DDS with an explicit domain
/// (defaults the middleware to `rmw_fastrtps_cpp`).
pub fn ros2_env_setup_dds_with_domain(distro: &str, domain_id: u8) -> String {
    ros2_env_setup_rmw_with_domain(distro, "rmw_fastrtps_cpp", domain_id)
}

/// Get ROS 2 environment setup for an explicit RMW + domain. No zenoh locator —
/// DDS RMWs use multicast discovery on the local network. The `rmw` string
/// selects the ROS 2 middleware (`rmw_fastrtps_cpp`, `rmw_cyclonedds_cpp`, …).
pub fn ros2_env_setup_rmw_with_domain(distro: &str, rmw: &str, domain_id: u8) -> String {
    format!(
        "source /opt/ros/{distro}/setup.bash && \
         export RMW_IMPLEMENTATION={rmw} && \
         export ROS_DOMAIN_ID={domain_id}"
    )
}

/// Get ROS 2 environment setup for CycloneDDS (`rmw_cyclonedds_cpp`) + domain.
/// Used by the CycloneDDS ↔ ROS 2 interop suite (Phase 183.5) — nano-ros's
/// Cyclone backend and a stock `rmw_cyclonedds_cpp` ROS 2 node share a
/// `ROS_DOMAIN_ID` and discover over RTPS/SPDP.
pub fn ros2_env_setup_cyclonedds_with_domain(distro: &str, domain_id: u8) -> String {
    ros2_env_setup_rmw_with_domain(distro, "rmw_cyclonedds_cpp", domain_id)
}

/// Check if `rmw_cyclonedds_cpp` is available in the ROS 2 install.
pub fn is_rmw_cyclonedds_available() -> bool {
    Command::new("bash")
        .args([
            "-c",
            "source /opt/ros/humble/setup.bash && ros2 pkg list | grep -q rmw_cyclonedds_cpp",
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Require ROS 2 with CycloneDDS (`rmw_cyclonedds_cpp`) for a test.
pub fn require_ros2_cyclonedds() -> bool {
    if !is_ros2_available() {
        eprintln!("Skipping test: ROS 2 not found");
        return false;
    }
    if !is_rmw_cyclonedds_available() {
        eprintln!("Skipping test: rmw_cyclonedds_cpp not found");
        return false;
    }
    true
}

/// Managed ROS 2 process using DDS (rmw_fastrtps_cpp).
///
/// Same pattern as `Ros2Process` but uses DDS multicast discovery
/// instead of zenoh. No locator parameter needed.
pub struct Ros2DdsProcess {
    handle: Child,
    name: String,
}

impl Ros2DdsProcess {
    /// Spawn a bash command in its own process group.
    fn spawn_bash(cmd: &str, name: impl Into<String>) -> TestResult<Self> {
        let name = name.into();
        let mut command = Command::new("bash");
        command
            .args(["-c", cmd])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut command);
        let handle = command
            .spawn()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to start {name}: {e}")))?;
        Ok(Self { handle, name })
    }

    /// Start a ROS 2 DDS topic echo subscriber
    ///
    /// # Arguments
    /// * `topic` - Topic name (e.g., "/chatter")
    /// * `msg_type` - Message type (e.g., "std_msgs/msg/Int32")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn topic_echo(topic: &str, msg_type: &str, distro: &str) -> TestResult<Self> {
        Self::topic_echo_with_domain(topic, msg_type, distro, 0)
    }

    /// Start a ROS 2 DDS topic echo subscriber on a specific ROS domain.
    pub fn topic_echo_with_domain(
        topic: &str,
        msg_type: &str,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_dds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic echo {topic} {msg_type} --qos-reliability reliable"
        );
        Self::spawn_bash(&cmd, format!("ros2-dds topic echo {topic}"))
    }

    /// Start a ROS 2 DDS topic pub publisher
    ///
    /// # Arguments
    /// * `topic` - Topic name (e.g., "/chatter")
    /// * `msg_type` - Message type (e.g., "std_msgs/msg/Int32")
    /// * `data` - Message data as YAML (e.g., "{data: 42}")
    /// * `rate` - Publishing rate in Hz
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn topic_pub(
        topic: &str,
        msg_type: &str,
        data: &str,
        rate: u32,
        distro: &str,
    ) -> TestResult<Self> {
        Self::topic_pub_with_domain(topic, msg_type, data, rate, distro, 0)
    }

    /// Start a ROS 2 DDS topic publisher on a specific ROS domain.
    pub fn topic_pub_with_domain(
        topic: &str,
        msg_type: &str,
        data: &str,
        rate: u32,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_dds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic pub -r {rate} {topic} {msg_type} \"{data}\" --qos-reliability reliable"
        );
        Self::spawn_bash(&cmd, format!("ros2-dds topic pub {topic}"))
    }

    /// Start a ROS 2 DDS service call
    ///
    /// # Arguments
    /// * `service_name` - Service name (e.g., "/add_two_ints")
    /// * `service_type` - Service type (e.g., "example_interfaces/srv/AddTwoInts")
    /// * `request` - Request data as YAML (e.g., "{a: 5, b: 3}")
    /// * `distro` - ROS distro (e.g., "humble")
    pub fn service_call(
        service_name: &str,
        service_type: &str,
        request: &str,
        distro: &str,
    ) -> TestResult<Self> {
        Self::service_call_with_domain(service_name, service_type, request, distro, 0)
    }

    /// Start a ROS 2 DDS service call on a specific ROS domain.
    pub fn service_call_with_domain(
        service_name: &str,
        service_type: &str,
        request: &str,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_dds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 service call {service_name} {service_type} \"{request}\""
        );
        Self::spawn_bash(&cmd, format!("ros2-dds service call {service_name}"))
    }

    // --- CycloneDDS variants (Phase 183.5) — same `ros2` CLI, but with
    // RMW_IMPLEMENTATION=rmw_cyclonedds_cpp so the ROS 2 node speaks Cyclone
    // RTPS to nano-ros's CycloneDDS backend on a shared ROS_DOMAIN_ID. ---

    /// CycloneDDS topic echo subscriber on a specific ROS domain.
    pub fn topic_echo_cyclonedds_with_domain(
        topic: &str,
        msg_type: &str,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_cyclonedds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic echo {topic} {msg_type} --qos-reliability reliable"
        );
        Self::spawn_bash(&cmd, format!("ros2-cyclone topic echo {topic}"))
    }

    /// CycloneDDS topic publisher on a specific ROS domain.
    pub fn topic_pub_cyclonedds_with_domain(
        topic: &str,
        msg_type: &str,
        data: &str,
        rate: u32,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_cyclonedds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic pub -r {rate} {topic} {msg_type} \"{data}\" --qos-reliability reliable"
        );
        Self::spawn_bash(&cmd, format!("ros2-cyclone topic pub {topic}"))
    }

    /// CycloneDDS service call on a specific ROS domain.
    pub fn service_call_cyclonedds_with_domain(
        service_name: &str,
        service_type: &str,
        request: &str,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_cyclonedds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 service call {service_name} {service_type} \"{request}\""
        );
        Self::spawn_bash(&cmd, format!("ros2-cyclone service call {service_name}"))
    }

    /// CycloneDDS action `send_goal --feedback` on a specific ROS domain.
    pub fn action_send_goal_cyclonedds_with_domain(
        action_name: &str,
        action_type: &str,
        goal: &str,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_cyclonedds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 20 ros2 action send_goal --feedback {action_name} {action_type} \"{goal}\""
        );
        Self::spawn_bash(&cmd, format!("ros2-cyclone action send_goal {action_name}"))
    }

    // --- DDS server / action side (Phase 183.6) — the reverse interop
    // directions: a ROS 2 (rmw_fastrtps_cpp) service/action SERVER + an action
    // goal CLIENT, on an explicit ROS_DOMAIN_ID, for nano-XRCE ↔ ROS 2. ---

    /// ROS 2 DDS `add_two_ints` service server (rclpy one-liner) on a domain.
    pub fn add_two_ints_server_with_domain(distro: &str, domain_id: u8) -> TestResult<Self> {
        let env_setup = ros2_env_setup_dds_with_domain(distro, domain_id);
        let python_script = r#"
import rclpy
from rclpy.node import Node
from example_interfaces.srv import AddTwoInts

class Server(Node):
    def __init__(self):
        super().__init__('add_two_ints_server')
        self.srv = self.create_service(AddTwoInts, '/add_two_ints', self.callback)
        self.get_logger().info('Service server ready')
    def callback(self, request, response):
        response.sum = request.a + request.b
        self.get_logger().info(f'Request: {request.a} + {request.b} = {response.sum}')
        return response

rclpy.init()
node = Server()
rclpy.spin(node)
"#;
        // Feed the script via a quoted heredoc so its real newlines reach
        // python. `python3 -c '<one line with \n literals>'` is a SyntaxError —
        // the `\n` is not a newline outside a string — so the server never
        // started and the reverse-direction interop tests timed out.
        let cmd = format!(
            "{env_setup} && timeout 60 python3 - <<'NROS_PYEOF'\n{python_script}\nNROS_PYEOF"
        );
        Self::spawn_bash(&cmd, "ros2-dds add_two_ints_server")
    }

    /// ROS 2 DDS Fibonacci action server on a domain.
    ///
    /// Serves `example_interfaces/action/Fibonacci` on `/fibonacci` — the SAME
    /// type+name the nano-ros action client/server examples use. The stock
    /// `action_tutorials_py fibonacci_action_server` serves
    /// `action_tutorials_interfaces/action/Fibonacci`, a DIFFERENT type, so DDS
    /// type matching never succeeds against our client → goal-acceptance timeout
    /// (233.6). A small rclpy server pinned to `example_interfaces` fixes the
    /// type alignment without depending on `action_tutorials_py` being present.
    pub fn action_server_fibonacci_with_domain(distro: &str, domain_id: u8) -> TestResult<Self> {
        let env_setup = ros2_env_setup_dds_with_domain(distro, domain_id);
        let python_script = r#"
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
        print(f'SERVER GOAL order={order}', flush=True)
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
        // Quoted heredoc so the script's real newlines reach python —
        // `python3 -c '<\n literals>'` is a SyntaxError (see add_two_ints_server).
        let cmd = format!(
            "{env_setup} && timeout 60 python3 - <<'NROS_PYEOF'\n{python_script}\nNROS_PYEOF"
        );
        Self::spawn_bash(&cmd, "ros2-dds fibonacci_action_server")
    }

    /// ROS 2 DDS `ros2 action send_goal --feedback` on a domain.
    pub fn action_send_goal_with_domain(
        action_name: &str,
        action_type: &str,
        goal: &str,
        distro: &str,
        domain_id: u8,
    ) -> TestResult<Self> {
        let env_setup = ros2_env_setup_dds_with_domain(distro, domain_id);
        let cmd = format!(
            "{env_setup} && timeout 20 ros2 action send_goal --feedback {action_name} {action_type} \"{goal}\""
        );
        Self::spawn_bash(&cmd, format!("ros2-dds action send_goal {action_name}"))
    }

    /// Wait for output and return it
    pub fn wait_for_output(&mut self, timeout: Duration) -> TestResult<String> {
        use std::io::Read;
        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;

        let start = std::time::Instant::now();
        let mut output = String::new();

        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed(format!("No stdout for {}", self.name)))?;

        // Set non-blocking mode on stdout so read() doesn't block forever
        #[cfg(unix)]
        let fd = {
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
            fd
        };

        let mut buffer = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                kill_process_group(&mut self.handle);
                if output.is_empty() {
                    return Err(TestError::Timeout);
                }
                break;
            }

            match self.handle.try_wait() {
                Ok(Some(_)) => {
                    let _ = stdout.read_to_string(&mut output);
                    break;
                }
                Ok(None) => match stdout.read(&mut buffer) {
                    Ok(0) => {
                        Self::wait_for_data(
                            #[cfg(unix)]
                            fd,
                            timeout.saturating_sub(start.elapsed()),
                        );
                    }
                    Ok(n) => {
                        output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        Self::wait_for_data(
                            #[cfg(unix)]
                            fd,
                            timeout.saturating_sub(start.elapsed()),
                        );
                    }
                    Err(_) => break,
                },
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Wait for data on a file descriptor (or sleep on non-Unix).
    #[cfg(unix)]
    fn wait_for_data(fd: std::os::unix::io::RawFd, remaining: Duration) {
        let ms = remaining.as_millis().min(500) as i32;
        let mut fds = [libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        }];
        unsafe {
            libc::poll(fds.as_mut_ptr(), 1, ms);
        }
    }

    #[cfg(not(unix))]
    fn wait_for_data(remaining: Duration) {
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }

    /// Kill the process
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
    }

    /// Check if process is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }
}

impl Drop for Ros2DdsProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ros2_env_setup() {
        let (setup, _config_dir) = ros2_env_setup("humble");
        assert!(setup.contains("/opt/ros/humble"));
        assert!(setup.contains("rmw_zenoh_cpp"));
        assert!(setup.contains("ZENOH_SESSION_CONFIG_URI"));
    }

    #[test]
    fn test_ros2_env_setup_dds_format() {
        let setup = ros2_env_setup_dds("humble");
        assert!(setup.contains("/opt/ros/humble"));
        assert!(setup.contains("rmw_fastrtps_cpp"));
        // DDS setup should NOT contain zenoh config
        assert!(!setup.contains("ZENOH"));
    }

    #[test]
    fn test_ros2_detection() {
        // Just verify detection works, don't require ROS 2
        let available = is_ros2_available();
        eprintln!("ROS 2 available: {}", available);
    }

    #[test]
    fn test_rmw_zenoh_detection() {
        let available = is_rmw_zenoh_available();
        eprintln!("rmw_zenoh_cpp available: {}", available);
    }

    #[test]
    fn test_rmw_fastrtps_detection() {
        let available = is_rmw_fastrtps_available();
        eprintln!("rmw_fastrtps_cpp available: {}", available);
    }
}
