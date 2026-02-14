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
