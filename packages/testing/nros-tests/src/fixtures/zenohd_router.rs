//! ZenohRouter fixture for managing zenohd process
//!
//! Provides automatic startup and cleanup of the zenoh router daemon.

use crate::{TestError, TestResult, process::graceful_kill_process_group};
use std::{
    io::Read,
    net::TcpStream,
    process::{Child, Stdio},
    time::{Duration, Instant},
};

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

fn wait_for_router_ready(handle: &mut Child, locator: &str, port: u16) -> TestResult<()> {
    let start = Instant::now();
    let timeout = Duration::from_secs(10);
    let addr = format!("127.0.0.1:{port}");

    while start.elapsed() < timeout {
        if TcpStream::connect(&addr).is_ok() {
            return Ok(());
        }

        if let Some(status) = handle.try_wait()? {
            let mut stderr = String::new();
            if let Some(mut pipe) = handle.stderr.take() {
                let _ = pipe.read_to_string(&mut stderr);
            }
            let stderr = stderr.trim();
            let detail = if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            };
            return Err(TestError::ProcessFailed(format!(
                "zenohd exited before listening on {locator} with {status}{detail}"
            )));
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    graceful_kill_process_group(handle);
    Err(TestError::Timeout)
}

/// Managed zenohd router process
///
/// Automatically starts zenohd on creation and kills it on drop.
/// Uses OS-assigned ephemeral ports to allow parallel test execution
/// across nextest's separate test processes.
///
/// Supports both TCP and TLS listeners.
///
/// # Example
///
/// ```ignore
/// use nros_tests::fixtures::ZenohRouter;
///
/// let router = ZenohRouter::start_unique().unwrap();
/// println!("Router at: {}", router.locator());
/// // Router is automatically stopped when dropped
/// ```
pub struct ZenohRouter {
    handle: Child,
    port: u16,
    tls: bool,
}

impl ZenohRouter {
    /// Start a new zenohd router on the specified port, bound to `127.0.0.1`.
    ///
    /// Kills any orphaned zenohd still listening on the port from a previous
    /// test run (e.g. if nextest SIGKILL'd the test process, preventing Drop).
    ///
    /// Binding to loopback prevents cross-platform interference and is
    /// sufficient for native/POSIX tests.
    ///
    /// For bridge-networked tests (ThreadX Linux sim) that connect via
    /// a non-loopback IP, use [`start_on`](Self::start_on) with `"0.0.0.0"`.
    pub fn start(port: u16) -> TestResult<Self> {
        Self::start_on("127.0.0.1", port)
    }

    /// Start a router for QEMU user-mode networking guests.
    ///
    /// Slirp guests connect to the host through gateway `10.0.2.2`; binding
    /// only to loopback can leave those guest SYNs unreachable on some hosts.
    pub fn start_slirp(port: u16) -> TestResult<Self> {
        Self::start_on("0.0.0.0", port)
    }

    /// Start a new zenohd router on the specified bind address and port.
    ///
    /// # Arguments
    /// * `bind_addr` - IP address to bind to (`"127.0.0.1"` or `"0.0.0.0"`)
    /// * `port` - TCP port to listen on
    ///
    /// # Returns
    /// A managed router instance that will be stopped on drop
    pub fn start_on(bind_addr: &str, port: u16) -> TestResult<Self> {
        if !crate::process::is_local_tcp_listener_available() {
            return Err(TestError::ProcessFailed(
                "local TCP listeners unavailable in this environment".to_string(),
            ));
        }

        // Kill any orphaned zenohd from a previous test run
        kill_listeners_on_port(port);

        let locator = format!("tcp/{}:{}", bind_addr, port);

        let mut cmd = std::process::Command::new(crate::process::zenohd_binary_path());
        cmd.args(["--listen", &locator, "--no-multicast-scouting"]);
        // Diagnostic log capture per port — opt-in, unified dir. Enabled by
        // ZENOHD_LOG=trace|debug (also sets RUST_LOG level) or NROS_TEST_LOGS;
        // the file lands in test-logs/fixtures/ (see fixtures::fixture_log_path).
        // Defaults to null sinks so a normal run leaves nothing behind.
        let zenohd_log = std::env::var("ZENOHD_LOG").ok();
        if zenohd_log.is_some() || crate::fixtures::fixture_logs_enabled() {
            let log_path = crate::fixtures::fixture_log_path(&format!("zenohd-{port}"));
            let log = std::fs::File::create(&log_path).map_err(TestError::ProcessStart)?;
            let log_stdout = log.try_clone().map_err(TestError::ProcessStart)?;
            cmd.env("RUST_LOG", zenohd_log.as_deref().unwrap_or("info"))
                .stdout(Stdio::from(log_stdout))
                .stderr(Stdio::from(log));
        } else {
            cmd.stdout(Stdio::null()).stderr(Stdio::piped());
        }
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let mut handle = cmd.spawn()?;

        // Wait for zenohd to be ready (TCP port accepting connections)
        // 10s allows for slow startup under concurrent test load
        wait_for_router_ready(&mut handle, &locator, port)?;

        Ok(Self {
            handle,
            port,
            tls: false,
        })
    }

    /// Start a zenohd router with serial listeners on the given PTY paths
    ///
    /// Each PTY path is added as a `serial/<path>#baudrate=115200` listener.
    /// No TCP listener is created — the router is only reachable via serial.
    ///
    /// # Arguments
    /// * `pty_paths` - Host PTY device paths (e.g., `/dev/pts/5`)
    pub fn start_serial(pty_paths: &[&str]) -> TestResult<Self> {
        let mut cmd = std::process::Command::new(crate::process::zenohd_binary_path());
        cmd.arg("--no-multicast-scouting");

        for pty in pty_paths {
            let locator = format!("serial/{}#baudrate=115200", pty);
            cmd.args(["--listen", &locator]);
        }

        let zenohd_log = std::env::var("ZENOHD_LOG").ok();
        if zenohd_log.is_some() || crate::fixtures::fixture_logs_enabled() {
            let log_path = crate::fixtures::fixture_log_path("zenohd-serial");
            let log = std::fs::File::create(&log_path).map_err(TestError::ProcessStart)?;
            let log_stdout = log.try_clone().map_err(TestError::ProcessStart)?;
            cmd.env("RUST_LOG", zenohd_log.as_deref().unwrap_or("info"))
                .stdout(Stdio::from(log_stdout))
                .stderr(Stdio::from(log));
        } else {
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::piped());
        }
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let mut handle = cmd.spawn()?;

        // Serial listeners don't have a TCP port to probe, so wait a bit
        // for zenohd to initialize and open the serial devices.
        std::thread::sleep(Duration::from_secs(2));

        // #189 — FAIL LOUD if the router already died. A zenohd built without
        // `zenoh/transport_serial` (the pre-nros2 SDK binaries) refuses the
        // serial listener ("Unicast not supported for serial protocol") and
        // exits within the sleep above; the old code returned the corpse and
        // every guest hung silently at its serial handshake until the test
        // timeout.
        if let Ok(Some(status)) = handle.try_wait() {
            return Err(TestError::ProcessFailed(format!(
                "zenohd exited ({status}) right after starting with serial \
                 listener(s) — the provisioned zenohd likely lacks the \
                 `zenoh/transport_serial` feature (needs the 1.7.2-nros2 \
                 build; re-run `just zenohd setup` after `git pull`)."
            )));
        }

        Ok(Self {
            handle,
            port: 0,
            tls: false,
        })
    }

    /// Start a router with TLS listener on the specified port
    ///
    /// # Arguments
    /// * `port` - TCP port to listen on
    /// * `cert_path` - Path to PEM certificate file
    /// * `key_path` - Path to PEM private key file
    pub fn start_tls(
        port: u16,
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
    ) -> TestResult<Self> {
        kill_listeners_on_port(port);

        let locator = format!("tls/127.0.0.1:{}", port);
        let cert_cfg = format!(
            "transport/link/tls/listen_certificate:\"{}\"",
            cert_path.display()
        );
        let key_cfg = format!(
            "transport/link/tls/listen_private_key:\"{}\"",
            key_path.display()
        );

        let mut cmd = std::process::Command::new(crate::process::zenohd_binary_path());
        cmd.args([
            "--listen",
            &locator,
            "--no-multicast-scouting",
            "--cfg",
            &cert_cfg,
            "--cfg",
            &key_cfg,
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let mut handle = cmd.spawn()?;

        // Wait for zenohd to be ready (TLS port accepting connections)
        wait_for_router_ready(&mut handle, &locator, port)?;

        Ok(Self {
            handle,
            port,
            tls: true,
        })
    }

    /// Start a TLS router on an OS-assigned ephemeral port (parallel-safe)
    pub fn start_tls_unique(
        cert_path: &std::path::Path,
        key_path: &std::path::Path,
    ) -> TestResult<Self> {
        let port = allocate_ephemeral_port()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to allocate port: {}", e)))?;
        Self::start_tls(port, cert_path, key_path)
    }

    /// Start a router on an OS-assigned ephemeral port (parallel-safe)
    pub fn start_unique() -> TestResult<Self> {
        let port = allocate_ephemeral_port()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to allocate port: {}", e)))?;
        Self::start(port)
    }

    /// Get the locator string for connecting to this router
    ///
    /// TLS connections use `localhost` (not `127.0.0.1`) to match
    /// the CN=localhost in our self-signed test certificates, which
    /// avoids hostname verification failures.
    pub fn locator(&self) -> String {
        if self.tls {
            format!("tls/localhost:{}", self.port)
        } else {
            format!("tcp/127.0.0.1:{}", self.port)
        }
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
        graceful_kill_process_group(&mut self.handle);
    }
}

/// rstest fixture for zenohd on port 7447 (native/POSIX integration tests only).
///
/// QEMU slirp platform tests use `ZenohRouter::start_slirp(platform::*.zenohd_port)`
/// with per-platform ports (7450–7456) for parallel execution.
///
/// # Example
///
/// ```ignore
/// use nros_tests::fixtures::zenohd;
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
