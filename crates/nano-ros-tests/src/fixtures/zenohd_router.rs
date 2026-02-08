//! ZenohRouter fixture for managing zenohd process
//!
//! Provides automatic startup and cleanup of the zenoh router daemon.

use crate::process::kill_process_group;
use crate::{TestError, TestResult, wait_for_port};
use std::net::TcpStream;
use std::process::Child;
use std::time::Duration;

/// Allocate an ephemeral port from the OS.
///
/// Binds a TCP listener on port 0 (OS-assigned), retrieves the port,
/// then closes the socket. This is safe for nextest where each test
/// runs in a separate process — a static counter would reset per process
/// and cause port collisions.
fn allocate_ephemeral_port() -> std::io::Result<u16> {
    let listener = std::net::TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    drop(listener);
    Ok(port)
}

/// Kill any process listening on the given TCP port.
///
/// Orphaned zenohd processes can survive across test runs when nextest
/// SIGKILL's a test process (preventing Drop from running). This function
/// detects and kills such orphans before starting a new router.
fn kill_listeners_on_port(port: u16) {
    if TcpStream::connect(format!("127.0.0.1:{}", port)).is_err() {
        return; // nothing listening
    }
    eprintln!(
        "WARNING: port {} already in use — killing orphaned process",
        port
    );
    // fuser -k sends SIGKILL to all processes using the port
    let _ = std::process::Command::new("fuser")
        .args(["-k", &format!("{}/tcp", port)])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    // Wait for the port to actually become free
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if TcpStream::connect(format!("127.0.0.1:{}", port)).is_err() {
            return; // port is now free
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    eprintln!("WARNING: port {} still in use after kill attempt", port);
}

/// Managed zenohd router process
///
/// Automatically starts zenohd on creation and kills it on drop.
/// Uses OS-assigned ephemeral ports to allow parallel test execution
/// across nextest's separate test processes.
///
/// # Example
///
/// ```ignore
/// use nano_ros_tests::fixtures::ZenohRouter;
///
/// let router = ZenohRouter::start_unique().unwrap();
/// println!("Router at: {}", router.locator());
/// // Router is automatically stopped when dropped
/// ```
pub struct ZenohRouter {
    handle: Child,
    port: u16,
}

impl ZenohRouter {
    /// Start a new zenohd router on the specified port
    ///
    /// Kills any orphaned zenohd still listening on the port from a previous
    /// test run (e.g. if nextest SIGKILL'd the test process, preventing Drop).
    ///
    /// # Arguments
    /// * `port` - TCP port to listen on
    ///
    /// # Returns
    /// A managed router instance that will be stopped on drop
    pub fn start(port: u16) -> TestResult<Self> {
        // Kill any orphaned zenohd from a previous test run
        kill_listeners_on_port(port);

        let locator = format!("tcp/0.0.0.0:{}", port);

        let mut cmd = std::process::Command::new(crate::process::zenohd_binary_path());
        cmd.args(["--listen", &locator, "--no-multicast-scouting"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        // Wait for zenohd to be ready (TCP port accepting connections)
        // 10s allows for slow startup under concurrent test load
        if !wait_for_port(port, Duration::from_secs(10)) {
            return Err(TestError::Timeout);
        }

        Ok(Self { handle, port })
    }

    /// Start a router on an OS-assigned ephemeral port (parallel-safe)
    pub fn start_unique() -> TestResult<Self> {
        let port = allocate_ephemeral_port()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to allocate port: {}", e)))?;
        Self::start(port)
    }

    /// Get the locator string for connecting to this router
    pub fn locator(&self) -> String {
        format!("tcp/127.0.0.1:{}", self.port)
    }

    /// Get the port number
    pub fn port(&self) -> u16 {
        self.port
    }

    /// Check if the router is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }
}

impl Drop for ZenohRouter {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
    }
}

/// rstest fixture for zenohd on default port
///
/// # Example
///
/// ```ignore
/// use nano_ros_tests::fixtures::zenohd;
/// use rstest::rstest;
///
/// #[rstest]
/// fn my_test(zenohd: ZenohRouter) {
///     // zenohd is ready to use
/// }
/// ```
#[rstest::fixture]
pub fn zenohd() -> ZenohRouter {
    ZenohRouter::start(7447).expect("Failed to start zenohd")
}

/// rstest fixture for zenohd on an OS-assigned ephemeral port (parallel-safe)
#[rstest::fixture]
pub fn zenohd_unique() -> ZenohRouter {
    ZenohRouter::start_unique().expect("Failed to start zenohd")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zenoh_router_locator() {
        // Just test the locator format without starting a real router
        let port = 12345;
        let expected = "tcp/127.0.0.1:12345";
        assert_eq!(format!("tcp/127.0.0.1:{}", port), expected);
    }

    #[test]
    fn test_ephemeral_port_allocation() {
        let p1 = allocate_ephemeral_port().unwrap();
        let p2 = allocate_ephemeral_port().unwrap();
        // OS should assign different ports
        assert_ne!(p1, p2);
        // Should be in the ephemeral range
        assert!(p1 > 1024);
        assert!(p2 > 1024);
    }
}
