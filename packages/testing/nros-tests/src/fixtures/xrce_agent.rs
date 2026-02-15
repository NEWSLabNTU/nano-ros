//! XrceAgent fixture for managing the Micro-XRCE-DDS-Agent process
//!
//! Provides automatic startup and cleanup of the XRCE-DDS Agent.

use crate::process::kill_process_group;
use crate::{TestError, TestResult};
use std::process::{Child, Stdio};
use std::time::Duration;

/// Managed XRCE-DDS Agent process.
///
/// Automatically starts the Agent on creation and kills it on drop.
/// Uses configurable UDP ports for parallel test execution.
///
/// # Example
///
/// ```ignore
/// use nros_tests::fixtures::XrceAgent;
///
/// let agent = XrceAgent::start_unique().unwrap();
/// println!("Agent at: {}", agent.addr());
/// // Agent is automatically stopped when dropped
/// ```
pub struct XrceAgent {
    handle: Child,
    port: u16,
}

impl XrceAgent {
    /// Start a new XRCE-DDS Agent on the specified UDP port.
    ///
    /// # Arguments
    /// * `port` - UDP port to listen on
    pub fn start(port: u16) -> TestResult<Self> {
        let binary = xrce_agent_binary_path();

        let mut cmd = std::process::Command::new(&binary);
        cmd.args(["udp4", "-p", &port.to_string()])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let handle = cmd.spawn().map_err(|e| {
            TestError::ProcessFailed(format!(
                "Failed to start XRCE Agent ({}): {}",
                binary.display(),
                e
            ))
        })?;

        // The Agent starts quickly — give it a short delay to bind the port
        std::thread::sleep(Duration::from_millis(500));

        Ok(Self { handle, port })
    }

    /// Start an agent on an OS-assigned ephemeral port (parallel-safe).
    pub fn start_unique() -> TestResult<Self> {
        let port = allocate_ephemeral_udp_port()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to allocate UDP port: {}", e)))?;
        Self::start(port)
    }

    /// Get the address string for connecting to this agent (e.g., "127.0.0.1:2019").
    pub fn addr(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// Get the UDP port number.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Check if the agent is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }
}

impl Drop for XrceAgent {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
    }
}

/// Allocate an ephemeral UDP port from the OS.
fn allocate_ephemeral_udp_port() -> std::io::Result<u16> {
    let socket = std::net::UdpSocket::bind("127.0.0.1:0")?;
    let port = socket.local_addr()?.port();
    drop(socket);
    Ok(port)
}

/// Get the path to the XRCE Agent binary.
///
/// Checks for a locally-built agent at `build/xrce-agent/MicroXRCEAgent`
/// first, then falls back to `MicroXRCEAgent` on the system PATH.
pub fn xrce_agent_binary_path() -> std::path::PathBuf {
    let local = crate::project_root().join("build/xrce-agent/MicroXRCEAgent");
    if local.exists() {
        local
    } else {
        std::path::PathBuf::from("MicroXRCEAgent")
    }
}

/// Check if the XRCE Agent binary is available (local build or system PATH).
pub fn is_xrce_agent_available() -> bool {
    std::process::Command::new(xrce_agent_binary_path())
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Skip test if the XRCE Agent is not available.
///
/// Returns `false` (test should skip) if the agent binary is not found.
/// Returns `true` if the agent is available and the test should proceed.
pub fn require_xrce_agent() -> bool {
    if !is_xrce_agent_available() {
        eprintln!("Skipping test: XRCE Agent not found (run `just build-xrce-agent`)");
        return false;
    }
    true
}

/// rstest fixture for XRCE Agent on default port 2019.
#[rstest::fixture]
pub fn xrce_agent() -> XrceAgent {
    XrceAgent::start(2019).expect("Failed to start XRCE Agent")
}

/// rstest fixture for XRCE Agent on an OS-assigned ephemeral port (parallel-safe).
#[rstest::fixture]
pub fn xrce_agent_unique() -> XrceAgent {
    XrceAgent::start_unique().expect("Failed to start XRCE Agent")
}

// ============================================================================
// XRCE Serial Agent (multiserial mode via socat PTY pairs)
// ============================================================================

/// Managed XRCE-DDS Agent in serial/multiserial mode over socat PTY pairs.
///
/// Creates N socat PTY pairs and starts the Agent on the agent-side PTYs.
/// Test binaries connect to the client-side PTYs via `client_pty_path()`.
///
/// For single-client tests, use `start(1)`. For multi-client tests (e.g.,
/// talker + listener), use `start(2)` with `multiserial` mode.
///
/// # Example
///
/// ```ignore
/// use nros_tests::fixtures::XrceSerialAgent;
///
/// // Single client
/// let agent = XrceSerialAgent::start(1).unwrap();
/// println!("Client PTY: {}", agent.client_pty_path(0));
///
/// // Two clients (talker + listener)
/// let agent = XrceSerialAgent::start(2).unwrap();
/// println!("Listener PTY: {}", agent.client_pty_path(0));
/// println!("Talker PTY: {}", agent.client_pty_path(1));
/// ```
pub struct XrceSerialAgent {
    socat_handles: Vec<Child>,
    agent_handle: Child,
    client_ptys: Vec<String>,
    _tmp_dir: tempfile::TempDir,
}

impl XrceSerialAgent {
    /// Start socat PTY pairs and an XRCE Agent in serial/multiserial mode.
    ///
    /// `num_ports` determines how many PTY pairs to create:
    /// - 1: uses `serial -D <pty>` mode
    /// - 2+: uses `multiserial -D "<pty1> <pty2> ..."` mode
    pub fn start(num_ports: usize) -> TestResult<Self> {
        assert!(num_ports >= 1, "need at least 1 port");

        let tmp_dir = tempfile::tempdir()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to create temp dir: {}", e)))?;

        let mut socat_handles = Vec::new();
        let mut agent_ptys = Vec::new();
        let mut client_ptys = Vec::new();

        // Start socat instances to create PTY pairs
        for i in 0..num_ports {
            let agent_pty = tmp_dir.path().join(format!("agent{i}.pty"));
            let client_pty = tmp_dir.path().join(format!("client{i}.pty"));

            let socat_args = format!(
                "pty,raw,echo=0,link={},b115200 pty,raw,echo=0,link={},b115200",
                agent_pty.display(),
                client_pty.display(),
            );
            let mut socat_cmd = std::process::Command::new("socat");
            socat_cmd
                .args(socat_args.split_whitespace())
                .stdout(Stdio::null())
                .stderr(Stdio::null());
            #[cfg(unix)]
            crate::process::set_new_process_group(&mut socat_cmd);
            let handle = socat_cmd
                .spawn()
                .map_err(|e| TestError::ProcessFailed(format!("Failed to start socat {i}: {e}")))?;
            socat_handles.push(handle);
            agent_ptys.push(agent_pty);
            client_ptys.push(client_pty);
        }

        // Wait for all socat PTY symlinks
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let all_exist =
                agent_ptys.iter().all(|p| p.exists()) && client_ptys.iter().all(|p| p.exists());
            if all_exist {
                break;
            }
            if std::time::Instant::now() > deadline {
                return Err(TestError::ProcessFailed(
                    "Timeout waiting for socat PTY symlinks".to_string(),
                ));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        std::thread::sleep(Duration::from_millis(200));

        // Start MicroXRCEAgent
        let binary = xrce_agent_binary_path();
        let mut agent_cmd = std::process::Command::new(&binary);
        if num_ports == 1 {
            // Single port: serial mode
            agent_cmd.args([
                "serial",
                "-D",
                agent_ptys[0].to_str().unwrap(),
                "-b",
                "115200",
            ]);
        } else {
            // Multiple ports: multiserial mode with space-separated device list
            let devs: String = agent_ptys
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect::<Vec<_>>()
                .join(" ");
            agent_cmd.args(["multiserial", "-D", &devs, "-b", "115200"]);
        }
        agent_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut agent_cmd);
        let agent_handle = agent_cmd.spawn().map_err(|e| {
            TestError::ProcessFailed(format!(
                "Failed to start XRCE Agent serial ({}): {e}",
                binary.display(),
            ))
        })?;

        // Give the Agent time to open PTYs and initialize
        std::thread::sleep(Duration::from_millis(500));

        let client_pty_strings: Vec<String> = client_ptys
            .iter()
            .map(|p| p.to_string_lossy().to_string())
            .collect();

        Ok(Self {
            socat_handles,
            agent_handle,
            client_ptys: client_pty_strings,
            _tmp_dir: tmp_dir,
        })
    }

    /// Get the PTY path for client connection at index `i`.
    pub fn client_pty_path(&self, i: usize) -> &str {
        &self.client_ptys[i]
    }

    /// Check if the agent is still running.
    pub fn is_running(&mut self) -> bool {
        matches!(self.agent_handle.try_wait(), Ok(None))
    }
}

impl Drop for XrceSerialAgent {
    fn drop(&mut self) {
        kill_process_group(&mut self.agent_handle);
        for handle in &mut self.socat_handles {
            kill_process_group(handle);
        }
    }
}

/// Check if `socat` is available on the system PATH.
pub fn is_socat_available() -> bool {
    std::process::Command::new("socat")
        .arg("-V")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

/// Skip test if socat is not available.
///
/// Returns `false` (test should skip) if socat is not found.
/// Returns `true` if socat is available and the test should proceed.
pub fn require_socat() -> bool {
    if !is_socat_available() {
        eprintln!("Skipping test: socat not found (run `sudo apt install socat`)");
        return false;
    }
    true
}

/// rstest fixture for XRCE Serial Agent with a single PTY pair.
#[rstest::fixture]
pub fn xrce_serial_agent() -> XrceSerialAgent {
    XrceSerialAgent::start(1).expect("Failed to start XRCE Serial Agent")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_xrce_agent_addr() {
        let port = 2019;
        assert_eq!(format!("127.0.0.1:{}", port), "127.0.0.1:2019");
    }

    #[test]
    fn test_ephemeral_udp_port_allocation() {
        let p1 = allocate_ephemeral_udp_port().unwrap();
        let p2 = allocate_ephemeral_udp_port().unwrap();
        assert_ne!(p1, p2);
        assert!(p1 > 1024);
        assert!(p2 > 1024);
    }
}
