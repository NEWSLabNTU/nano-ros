//! ROS 2 process fixtures for integration tests
//!
//! Provides helpers for running ROS 2 commands and processes.

use crate::process::{kill_process_group, set_new_process_group};
use crate::{TestError, TestResult};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Default ROS 2 distro to use
pub const DEFAULT_ROS_DISTRO: &str = "humble";

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

/// Check if rmw_zenoh_cpp is available
pub fn is_rmw_zenoh_available() -> bool {
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
pub fn ros2_env_setup(distro: &str) -> String {
    ros2_env_setup_with_locator(distro, "tcp/127.0.0.1:7447")
}

/// Get ROS 2 environment setup command with custom locator
pub fn ros2_env_setup_with_locator(distro: &str, locator: &str) -> String {
    format!(
        "source /opt/ros/{distro}/setup.bash && \
         export RMW_IMPLEMENTATION=rmw_zenoh_cpp && \
         export ZENOH_CONFIG_OVERRIDE='mode=\"client\";connect/endpoints=[\"{locator}\"]'"
    )
}

/// Managed ROS 2 process
///
/// Wraps a ROS 2 command with proper environment setup.
/// Automatically kills the process on drop.
pub struct Ros2Process {
    handle: Child,
    name: String,
}

impl Ros2Process {
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic echo {topic} {msg_type} --qos-reliability best_effort"
        );

        Self::spawn_bash(&cmd, format!("ros2 topic echo {topic}"))
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 15 ros2 action send_goal --feedback {action_name} {action_type} \"{goal}\""
        );

        Self::spawn_bash(&cmd, format!("ros2 action send_goal {action_name}"))
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        // Use ros2 run to start the action server from example_interfaces
        // Note: The standard action server example is in rclpy_action_server or similar
        // For testing, we use a simple Python one-liner that creates a Fibonacci server
        let cmd = format!(
            "{env_setup} && timeout 60 ros2 run action_tutorials_py fibonacci_action_server"
        );

        Self::spawn_bash(&cmd, "ros2 fibonacci_action_server")
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic pub -r {rate} {topic} {msg_type} \"{data}\" --qos-reliability best_effort"
        );

        Self::spawn_bash(&cmd, format!("ros2 topic pub {topic}"))
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
    let env_setup = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 node list 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 node list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 topic list` and return the output
pub fn ros2_topic_list(locator: &str, distro: &str) -> TestResult<String> {
    let env_setup = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 topic list 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 topic list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 service list` and return the output
pub fn ros2_service_list(locator: &str, distro: &str) -> TestResult<String> {
    let env_setup = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 service list 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 service list: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 node info` for a specific node
pub fn ros2_node_info(node_name: &str, locator: &str, distro: &str) -> TestResult<String> {
    let env_setup = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 node info {node_name} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 node info: {e}")))?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Run `ros2 param list` for a specific node
pub fn ros2_param_list(node_name: &str, locator: &str, distro: &str) -> TestResult<String> {
    let env_setup = ros2_env_setup_with_locator(distro, locator);
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
    let env_setup = ros2_env_setup_with_locator(distro, locator);
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
    let env_setup = ros2_env_setup_with_locator(distro, locator);
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
    let env_setup = ros2_env_setup_with_locator(distro, locator);
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
    let env_setup = ros2_env_setup_with_locator(distro, locator);
    let cmd = format!("{env_setup} && timeout 10 ros2 topic info {topic} 2>&1");

    let output = Command::new("bash")
        .args(["-c", &cmd])
        .output()
        .map_err(|e| TestError::ProcessFailed(format!("Failed to run ros2 topic info: {e}")))?;

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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 service call {service_name} {service_type} \"{request}\""
        );

        Self::spawn_bash(&cmd, format!("ros2 service call {service_name}"))
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
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

        let cmd = format!(
            "{env_setup} && timeout 60 python3 -c '{}'",
            python_script.replace('\n', "\\n").replace('\'', "\\'")
        );

        Self::spawn_bash(&cmd, "ros2 add_two_ints_server")
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic echo {topic} {msg_type} --qos-reliability {reliability}"
        );

        Self::spawn_bash(&cmd, format!("ros2 topic echo {topic} ({reliability})"))
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
        let env_setup = ros2_env_setup_with_locator(distro, locator);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 topic pub -r {rate} {topic} {msg_type} \"{data}\" --qos-reliability {reliability}"
        );

        Self::spawn_bash(&cmd, format!("ros2 topic pub {topic} ({reliability})"))
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
    format!(
        "source /opt/ros/{distro}/setup.bash && \
         export RMW_IMPLEMENTATION=rmw_fastrtps_cpp"
    )
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
        let env_setup = ros2_env_setup_dds(distro);
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
        let env_setup = ros2_env_setup_dds(distro);
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
        let env_setup = ros2_env_setup_dds(distro);
        let cmd = format!(
            "{env_setup} && timeout 10 ros2 service call {service_name} {service_type} \"{request}\""
        );
        Self::spawn_bash(&cmd, format!("ros2-dds service call {service_name}"))
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
        let setup = ros2_env_setup("humble");
        assert!(setup.contains("/opt/ros/humble"));
        assert!(setup.contains("rmw_zenoh_cpp"));
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
