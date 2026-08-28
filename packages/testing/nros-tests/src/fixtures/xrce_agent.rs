//! XrceAgent fixture for managing the Micro-XRCE-DDS-Agent process
//!
//! Provides automatic startup and cleanup of the XRCE-DDS Agent.

use crate::{TestError, TestResult, process::kill_process_group};
use std::{
    process::{Child, Stdio},
    time::Duration,
};

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
    /// Issue 0470 — held for the agent's lifetime so no other fixture can be
    /// handed this port. `None` when the caller named the port itself
    /// (`start(port)`), which is a deliberate choice the caller owns.
    _lease: Option<crate::port_lease::PortLease>,
}

impl XrceAgent {
    /// Start a new XRCE-DDS Agent on the specified UDP port.
    ///
    /// # Arguments
    /// * `port` - UDP port to listen on
    pub fn start(port: u16) -> TestResult<Self> {
        // Issue 0741 — say WHICH agent this run used, once, before it matters.
        // Two can be installed at a time and the resolution order picks one
        // silently; the failure mode it produces (a 15-byte reader history for
        // a 28-byte reply) names neither the agent nor its Fast-DDS, so the
        // provenance has been reconstructed by hand on every host that hit it.
        // `eprintln!` rather than `nros_log`: this must reach a test log the
        // reader already has (nextest captures stderr and prints it on
        // failure), and the sibling fixture diagnostics in this crate print the
        // same way. A `nros_log` record needs an initialised logger that a bare
        // test binary does not have, so it went nowhere when tried.
        let (binary, provenance) = xrce_agent_binary_with_provenance();
        eprintln!(
            "xrce agent: {} — {}",
            binary.display(),
            provenance.describe()
        );

        let mut cmd = std::process::Command::new(&binary);
        // Phase 160.H.1.2 — `-v6` enables Agent verbose logging
        // (`UXR_VERBOSE_LEVEL_TRACE`) when `NROS_XRCE_AGENT_VERBOSE` is set,
        // capturing every inbound/outbound message at the Agent.
        let mut agent_args: Vec<String> = vec!["udp4".into(), "-p".into(), port.to_string()];
        let verbose = std::env::var_os("NROS_XRCE_AGENT_VERBOSE").is_some();
        if verbose {
            agent_args.push("-v6".into());
        }
        cmd.args(&agent_args);
        // Opt-in log capture into the unified dir (test-logs/fixtures/) — enabled
        // by NROS_XRCE_AGENT_VERBOSE or NROS_TEST_LOGS. Default: null sink, so a
        // normal run leaves no xrce-agent-*.log behind (was: always written to
        // the repo root).
        if verbose || crate::fixtures::fixture_logs_enabled() {
            let log_path = crate::fixtures::fixture_log_path(&format!("xrce-agent-{port}"));
            let log_file = std::fs::File::create(&log_path).expect("failed to create log file");
            cmd.stdout(log_file.try_clone().expect("failed to clone log file"))
                .stderr(log_file);
        } else {
            cmd.stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
        }
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let handle = cmd.spawn().map_err(|e| {
            TestError::ProcessFailed(format!(
                "Failed to start XRCE Agent ({}): {}",
                binary.display(),
                e
            ))
        })?;

        let mut agent = Self {
            handle,
            port,
            _lease: None,
        };
        agent.wait_until_listening(Duration::from_secs(15))?;
        Ok(agent)
    }

    /// Block until the agent actually holds `self.port`, or fail saying why.
    ///
    /// Issue 0869. This used to be `sleep(500ms)` with the comment "the Agent
    /// starts quickly" — which is true on an idle host and a guess on a loaded
    /// one. When the guess is wrong nothing reports it: the client sends its
    /// session-open to a port nobody is listening on, gets no reply, and the
    /// test fails much later as a missing RESULT. The C++ action client even
    /// prints that as `Goal was rejected by server` (issue 0868), so a fixture
    /// timing bug arrives dressed as a server decision.
    ///
    /// The probe is a bind attempt, which works because the port LEASE is a
    /// lockfile and never holds the socket (issue 0470): while the agent owns
    /// the port a second UDP bind fails, and before it does one succeeds. So
    /// "bind failed" is exactly "the agent is listening", with no sleep and no
    /// dependency on how loaded the host is.
    ///
    /// A dead agent is reported as a dead agent rather than waited out — that
    /// is the case the old sleep turned into a 15-second silence followed by an
    /// unrelated assertion.
    fn wait_until_listening(&mut self, timeout: Duration) -> TestResult<()> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            match self.handle.try_wait() {
                Ok(Some(status)) => {
                    return Err(TestError::ProcessFailed(format!(
                        "XRCE Agent exited before binding udp4 port {} ({status})",
                        self.port
                    )));
                }
                Ok(None) => {}
                Err(e) => {
                    return Err(TestError::ProcessFailed(format!(
                        "could not poll the XRCE Agent on port {}: {e}",
                        self.port
                    )));
                }
            }
            if std::net::UdpSocket::bind(("127.0.0.1", self.port)).is_err() {
                return Ok(());
            }
            if std::time::Instant::now() >= deadline {
                return Err(TestError::ProcessFailed(format!(
                    "XRCE Agent did not bind udp4 port {} within {:?} — it is running but not \
                     listening, so every client on this port would time out",
                    self.port, timeout
                )));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Start an agent on an OS-assigned ephemeral port (parallel-safe).
    ///
    /// Issue 0470 — the port is LEASED, not merely suggested. The previous
    /// implementation bound port 0, read the number and closed the socket, so
    /// the port was free again before the agent bound it and the kernel handed
    /// the same number to a concurrent caller (measured: 87 collisions in 2400
    /// allocations across 12 processes). Two agents on one port put a
    /// neighbour's samples into this test's subscription, which surfaced as a
    /// payload-integrity failure rather than as a port conflict.
    pub fn start_unique() -> TestResult<Self> {
        let lease = crate::port_lease::lease_port(crate::port_lease::Transport::Udp)
            .map_err(|e| TestError::ProcessFailed(format!("Failed to lease UDP port: {}", e)))?;
        let mut agent = Self::start(lease.port())?;
        agent._lease = Some(lease);
        Ok(agent)
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

#[cfg(test)]
mod readiness_tests {
    use super::*;

    /// The probe must TIME OUT on a live process that never binds the port —
    /// the case the old `sleep(500ms)` could not distinguish from success.
    ///
    /// A stand-in child (`sleep`) stands for an agent that is running and not
    /// listening. Without a negative control this fix is a claim: the probe
    /// returning `Ok` proves nothing unless it can also return `Err`.
    #[test]
    fn a_process_that_never_binds_times_out() {
        let mut cmd = std::process::Command::new("sleep");
        // Its own process group, so the Drop below reaps it immediately instead
        // of waiting the child out — a 30-second unit test was the first
        // version of this.
        #[cfg(unix)]
        crate::process::set_new_process_group(&mut cmd);
        let handle = cmd
            .arg("5")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn the stand-in child");
        // A leased port so no concurrent fixture can bind it under us and make
        // this pass for the wrong reason.
        let lease = crate::port_lease::lease_port(crate::port_lease::Transport::Udp)
            .expect("lease a udp port");
        let mut agent = XrceAgent {
            handle,
            port: lease.port(),
            _lease: Some(lease),
        };

        let err = agent
            .wait_until_listening(Duration::from_millis(300))
            .expect_err("a process that never binds must not report ready");
        let _ = agent.handle.kill();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("did not bind"),
            "the timeout must say the port was never bound, got: {msg}"
        );
    }

    /// And it must NOTICE a dead agent rather than waiting out the timeout.
    #[test]
    fn a_child_that_exited_is_reported_as_exited() {
        let handle = std::process::Command::new("true")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn a child that exits immediately");
        let lease = crate::port_lease::lease_port(crate::port_lease::Transport::Udp)
            .expect("lease a udp port");
        let mut agent = XrceAgent {
            handle,
            port: lease.port(),
            _lease: Some(lease),
        };
        // Let it actually exit, so this tests the exited branch and not the
        // still-running one.
        let _ = agent.handle.wait();

        let err = agent
            .wait_until_listening(Duration::from_secs(5))
            .expect_err("an exited agent must be an error");
        let msg = format!("{err:?}");
        assert!(
            msg.contains("exited before binding"),
            "a dead agent must be named as dead, got: {msg}"
        );
    }
}

impl Drop for XrceAgent {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
    }
}

/// Which agent got resolved, and whether it is paired with the sourced ROS.
///
/// Issue 0741. Two agents can be installed at once and the resolution order
/// below silently prefers one: `build/xrce-agent/` (built by `just xrce setup`
/// against the sourced ROS's own Fast-DDS — "zero skew") over the `nros setup`
/// store, whose SDK pin BUNDLES its own Fast-DDS and loads it through a
/// relocatable launcher. Measured on one host: the store agent bundles Fast-DDS
/// **2.14.6** while the ROS peer on the same machine is **2.6.11** — a
/// Jazzy-era library registering the DDS type a Humble reader sizes itself
/// from.
///
/// Whether that skew is fatal is exactly what issue 0741 is still deciding: it
/// is fatal on one host and harmless on three others, and both agents pass here.
/// What is NOT in doubt is that a run should SAY which one it used. Five axes
/// were compared by hand across hosts before the libraries themselves were
/// reached, and every one of those comparisons started by asking someone to
/// work out which agent their machine had picked.
///
/// Same shape as issue 0774 one component over: a resolver that finds A binary
/// is not a resolver that found the RIGHT one, and the failure surfaces layers
/// away (there, a SEGV; here, a 15-byte history on a 28-byte reply).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XrceAgentProvenance {
    /// `build/xrce-agent/` — built against the sourced ROS, no Fast-DDS skew.
    RosPaired,
    /// The `nros setup` SDK store — bundles its own Fast-DDS.
    SdkStore,
    /// Bare `MicroXRCEAgent` from `PATH`; provenance unknown.
    SystemPath,
}

impl XrceAgentProvenance {
    /// One line, safe to print on every run.
    pub fn describe(self) -> &'static str {
        match self {
            Self::RosPaired => "built against the sourced ROS (no Fast-DDS skew)",
            Self::SdkStore => {
                "the `nros setup` SDK pin, which BUNDLES its own Fast-DDS (a version skew against the ROS peer is possible — issue 0741)"
            }
            Self::SystemPath => "found on PATH; its Fast-DDS pairing is unknown",
        }
    }
}

/// [`xrce_agent_binary_path`], plus WHERE it came from.
pub fn xrce_agent_binary_with_provenance() -> (std::path::PathBuf, XrceAgentProvenance) {
    let local = crate::build_dir(crate::kind::XRCE_AGENT, &[]).join("MicroXRCEAgent");
    if local.exists() {
        // issue 0741 — the PATH does not decide the provenance, the CONTENT
        // does. `scripts/xrce-agent/build.sh` publishes to this one path two
        // different ways: a real ROS-paired build, OR an 85-byte forwarding
        // wrapper (`#!/bin/sh\nexec "<store>/MicroXRCEAgent" "$@"`) around the
        // SDK store agent, which BUNDLES its own Fast-DDS. Classifying by path
        // alone reported the wrapper as "no Fast-DDS skew" while running the
        // very skew it claims to exclude — and 0741's central measurement
        // ("ROS-paired 8/8, SDK store 8/8, so skew alone is not the cause")
        // compared two arms that may have been the same binary.
        if let Some(target) = forwarding_wrapper_target(&local) {
            return (target, XrceAgentProvenance::SdkStore);
        }
        return (local, XrceAgentProvenance::RosPaired);
    }
    if let Some(store) = crate::nros_store_bin("xrce-agent", "MicroXRCEAgent") {
        return (store, XrceAgentProvenance::SdkStore);
    }
    (
        std::path::PathBuf::from("MicroXRCEAgent"),
        XrceAgentProvenance::SystemPath,
    )
}

/// If `path` is the `/bin/sh` forwarding wrapper `build.sh` writes for a store
/// agent, return what it execs. `None` for a real binary.
///
/// Deliberately narrow: it matches the exact two-line shape that script emits,
/// so an unrelated executable shell script is not silently reinterpreted.
fn forwarding_wrapper_target(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let text = std::fs::read_to_string(path).ok()?;
    if !text.starts_with("#!/bin/sh") {
        return None;
    }
    let exec_line = text.lines().find(|l| l.trim_start().starts_with("exec "))?;
    let start = exec_line.find('"')? + 1;
    let end = exec_line[start..].find('"')? + start;
    Some(std::path::PathBuf::from(&exec_line[start..end]))
}

/// Get the path to the XRCE Agent binary.
///
/// Checks for a locally-built agent at `build/xrce-agent/MicroXRCEAgent`
/// first, then the `nros setup` store (`xrce-agent` tool), then falls back to
/// `MicroXRCEAgent` on the system PATH.
///
/// Derived from [`xrce_agent_binary_with_provenance`] so the two orders cannot
/// drift — the resolution rule has one spelling.
pub fn xrce_agent_binary_path() -> std::path::PathBuf {
    // phase-334 W2.b step 2 — the Rust mirror of `nros_build_dir`; the shell
    // half moved in the same commit.
    xrce_agent_binary_with_provenance().0
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
        eprintln!("Skipping test: XRCE Agent not found (run `just xrce setup`)");
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
        let verbose = std::env::var_os("NROS_XRCE_AGENT_VERBOSE").is_some();
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
        if verbose {
            agent_cmd.arg("-v6");
        }
        if verbose || crate::fixtures::fixture_logs_enabled() {
            let log_path = crate::fixtures::fixture_log_path("xrce-agent-serial");
            let log_file = std::fs::File::create(&log_path).map_err(|e| {
                TestError::ProcessFailed(format!(
                    "Failed to open xrce-agent serial log {}: {e}",
                    log_path.display()
                ))
            })?;
            agent_cmd
                .stdout(
                    log_file
                        .try_clone()
                        .map_err(|e| TestError::ProcessFailed(format!("clone log fd: {e}")))?,
                )
                .stderr(log_file);
        } else {
            agent_cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
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
    #[test]
    fn test_xrce_agent_addr() {
        let port = 2019;
        assert_eq!(format!("127.0.0.1:{}", port), "127.0.0.1:2019");
    }

    /// Issue 0470 — leases are distinct while HELD, which is the property the
    /// agent needs. The test this replaces allocated two ports SEQUENTIALLY and
    /// asserted they differed; that holds even for the racy allocator it was
    /// guarding, because the kernel only re-hands a port once the first one is
    /// released. It could not have failed on the defect it existed to catch.
    #[test]
    fn leased_udp_ports_are_distinct_while_held() {
        use crate::port_lease::{Transport, lease_port};
        let held: Vec<_> = (0..16)
            .map(|_| lease_port(Transport::Udp).unwrap())
            .collect();
        let mut ports: Vec<u16> = held.iter().map(|l| l.port()).collect();
        assert!(ports.iter().all(|p| *p > 1024));
        let total = ports.len();
        ports.sort_unstable();
        ports.dedup();
        assert_eq!(
            ports.len(),
            total,
            "two concurrently-held leases shared a port"
        );
    }
}
