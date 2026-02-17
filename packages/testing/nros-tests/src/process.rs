//! Managed process utilities for integration tests
//!
//! Provides RAII-based process management with automatic cleanup.
//! All child processes are spawned in their own process group so that
//! `kill_process_group()` can reap the entire tree (bash + children).

use crate::TestError;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// Wait for a file descriptor to become readable, or sleep on non-Unix.
///
/// Uses `poll(2)` on Unix to avoid busy-waiting.
#[cfg(unix)]
fn poll_or_sleep(fd: std::os::unix::io::RawFd, remaining: Duration) {
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
fn poll_or_sleep(remaining: Duration) {
    std::thread::sleep(remaining.min(Duration::from_millis(50)));
}

/// Configure a Command to spawn the child in its own process group.
///
/// This ensures that `kill_process_group()` can kill the child and all
/// its descendants (e.g., bash → timeout → ros2).
///
/// On Linux, also sets `PR_SET_PDEATHSIG(SIGKILL)` so the child is killed
/// when the parent dies — prevents orphans when nextest SIGKILL's the test binary.
#[cfg(unix)]
pub fn set_new_process_group(command: &mut Command) -> &mut Command {
    use std::os::unix::process::CommandExt;
    // SAFETY: setpgid and prctl are async-signal-safe and called before exec
    unsafe {
        command.pre_exec(|| {
            libc::setpgid(0, 0);
            #[cfg(target_os = "linux")]
            {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL);
            }
            Ok(())
        })
    }
}

/// Kill an entire process group given a child handle.
///
/// Sends SIGKILL to the process group (negative PID), then waits for
/// the direct child to be reaped.
#[cfg(unix)]
pub fn kill_process_group(handle: &mut Child) {
    let pid = handle.id() as libc::pid_t;
    // Kill the entire process group
    unsafe {
        libc::kill(-pid, libc::SIGKILL);
    }
    // Reap the direct child to avoid zombies
    let _ = handle.wait();
}

/// Fallback for non-unix: kill just the direct child.
#[cfg(not(unix))]
pub fn kill_process_group(handle: &mut Child) {
    let _ = handle.kill();
    let _ = handle.wait();
}

/// Managed process with automatic cleanup
///
/// Wraps a child process and ensures it is killed on drop.
/// Used for running talker/listener binaries and other test processes.
///
/// # Example
///
/// ```ignore
/// let mut proc = ManagedProcess::spawn(&binary_path, &["--tcp", "127.0.0.1:7447"], "talker")?;
/// std::thread::sleep(Duration::from_secs(5));
/// let output = proc.wait_for_output(Duration::from_secs(2))?;
/// // Process is automatically killed on drop
/// ```
pub struct ManagedProcess {
    handle: Child,
    name: String,
}

impl ManagedProcess {
    /// Spawn a new managed process
    ///
    /// # Arguments
    /// * `binary` - Path to the executable
    /// * `args` - Command line arguments
    /// * `name` - Human-readable name for error messages
    pub fn spawn(
        binary: &std::path::Path,
        args: &[&str],
        name: impl Into<String>,
    ) -> Result<Self, TestError> {
        let name = name.into();
        let mut cmd = Command::new(binary);
        cmd.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd
            .spawn()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to spawn {}: {}", name, e)))?;

        Ok(Self { handle, name })
    }

    /// Spawn a process from a Command builder
    ///
    /// # Arguments
    /// * `command` - Pre-configured Command builder
    /// * `name` - Human-readable name for error messages
    pub fn spawn_command(mut command: Command, name: impl Into<String>) -> Result<Self, TestError> {
        let name = name.into();
        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut command);
        let handle = command
            .spawn()
            .map_err(|e| TestError::ProcessFailed(format!("Failed to spawn {}: {}", name, e)))?;

        Ok(Self { handle, name })
    }

    /// Get the process name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Check if process is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }

    /// Get mutable access to the underlying Child handle
    ///
    /// Use with caution - modifications may affect cleanup behavior.
    pub fn handle_mut(&mut self) -> &mut Child {
        &mut self.handle
    }

    /// Wait for output with timeout
    ///
    /// Collects stdout from the process until:
    /// - The timeout is reached
    /// - The process exits
    /// - An error occurs
    ///
    /// The process is killed when the timeout is reached.
    pub fn wait_for_output(&mut self, timeout: Duration) -> Result<String, TestError> {
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

        // Set non-blocking mode on stdout
        #[cfg(unix)]
        {
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let mut buffer = [0u8; 4096];

        #[cfg(unix)]
        let fd = stdout.as_raw_fd();

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
                    // Process exited, read remaining output
                    let _ = stdout.read_to_string(&mut output);
                    break;
                }
                Ok(None) => match stdout.read(&mut buffer) {
                    Ok(0) => {
                        poll_or_sleep(fd, timeout.saturating_sub(start.elapsed()));
                    }
                    Ok(n) => {
                        output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        poll_or_sleep(fd, timeout.saturating_sub(start.elapsed()));
                    }
                    Err(_) => break,
                },
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Wait until a pattern appears in stdout+stderr, then return all output so far.
    ///
    /// Useful for waiting until a process prints a readiness marker or until
    /// enough messages have been received, without a fixed sleep.
    pub fn wait_for_output_pattern(
        &mut self,
        pattern: &str,
        timeout: Duration,
    ) -> Result<String, TestError> {
        use std::io::Read;
        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;

        let start = std::time::Instant::now();
        let mut output = String::new();

        let mut stdout = self.handle.stdout.take();
        let mut stderr = self.handle.stderr.take();

        // Set non-blocking mode
        #[cfg(unix)]
        {
            if let Some(ref out) = stdout {
                let fd = out.as_raw_fd();
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
            if let Some(ref err) = stderr {
                let fd = err.as_raw_fd();
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
        }

        let mut buf = [0u8; 4096];

        loop {
            if start.elapsed() > timeout {
                // Put handles back so drop can still kill the process
                self.handle.stdout = stdout;
                self.handle.stderr = stderr;
                if output.is_empty() {
                    return Err(TestError::Timeout);
                }
                return Ok(output);
            }

            if let Ok(Some(_)) = self.handle.try_wait() {
                if let Some(ref mut out) = stdout {
                    let _ = out.read_to_string(&mut output);
                }
                if let Some(ref mut err) = stderr {
                    let _ = err.read_to_string(&mut output);
                }
                break;
            }

            let mut got_data = false;
            if let Some(ref mut out) = stdout
                && let Ok(n) = out.read(&mut buf)
                && n > 0
            {
                output.push_str(&String::from_utf8_lossy(&buf[..n]));
                got_data = true;
            }
            if let Some(ref mut err) = stderr
                && let Ok(n) = err.read(&mut buf)
                && n > 0
            {
                output.push_str(&String::from_utf8_lossy(&buf[..n]));
                got_data = true;
            }

            if output.contains(pattern) {
                // Put handles back for further use
                self.handle.stdout = stdout;
                self.handle.stderr = stderr;
                return Ok(output);
            }

            if !got_data {
                #[cfg(unix)]
                {
                    let remaining = timeout.saturating_sub(start.elapsed());
                    let ms = remaining.as_millis().min(500) as i32;
                    let mut fds = Vec::new();
                    if let Some(ref out) = stdout {
                        fds.push(libc::pollfd {
                            fd: out.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        });
                    }
                    if let Some(ref err) = stderr {
                        fds.push(libc::pollfd {
                            fd: err.as_raw_fd(),
                            events: libc::POLLIN,
                            revents: 0,
                        });
                    }
                    if !fds.is_empty() {
                        unsafe {
                            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, ms);
                        }
                    }
                }
                #[cfg(not(unix))]
                std::thread::sleep(Duration::from_millis(50));
            }
        }

        // Put handles back
        self.handle.stdout = stdout;
        self.handle.stderr = stderr;
        Ok(output)
    }

    /// Kill the process group and wait for it to exit
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
    }

    /// Wait for output with timeout, capturing both stdout and stderr
    ///
    /// Similar to wait_for_output but also captures stderr (useful for env_logger output).
    /// The process is killed when the timeout is reached.
    pub fn wait_for_all_output(&mut self, timeout: Duration) -> Result<String, TestError> {
        use std::io::Read;
        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;

        let start = std::time::Instant::now();
        let mut output = String::new();

        // Take both stdout and stderr
        let mut stdout = self.handle.stdout.take();
        let mut stderr = self.handle.stderr.take();

        // Set non-blocking mode on stdout and stderr
        #[cfg(unix)]
        {
            if let Some(ref out) = stdout {
                let fd = out.as_raw_fd();
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
            if let Some(ref err) = stderr {
                let fd = err.as_raw_fd();
                unsafe {
                    let flags = libc::fcntl(fd, libc::F_GETFL);
                    libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
                }
            }
        }

        let mut stdout_buf = [0u8; 4096];
        let mut stderr_buf = [0u8; 4096];

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
                    // Process exited, read remaining output
                    if let Some(ref mut out) = stdout {
                        let _ = out.read_to_string(&mut output);
                    }
                    if let Some(ref mut err) = stderr {
                        let _ = err.read_to_string(&mut output);
                    }
                    break;
                }
                Ok(None) => {
                    // Read from stdout
                    if let Some(ref mut out) = stdout {
                        match out.read(&mut stdout_buf) {
                            Ok(0) => {}
                            Ok(n) => {
                                output.push_str(&String::from_utf8_lossy(&stdout_buf[..n]));
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => {}
                        }
                    }
                    // Read from stderr
                    if let Some(ref mut err) = stderr {
                        match err.read(&mut stderr_buf) {
                            Ok(0) => {}
                            Ok(n) => {
                                output.push_str(&String::from_utf8_lossy(&stderr_buf[..n]));
                            }
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(_) => {}
                        }
                    }
                    // Wait for data on either fd via poll(2)
                    #[cfg(unix)]
                    {
                        let remaining = timeout.saturating_sub(start.elapsed());
                        let ms = remaining.as_millis().min(500) as i32;
                        let mut fds = Vec::new();
                        if let Some(ref out) = stdout {
                            fds.push(libc::pollfd {
                                fd: out.as_raw_fd(),
                                events: libc::POLLIN,
                                revents: 0,
                            });
                        }
                        if let Some(ref err) = stderr {
                            fds.push(libc::pollfd {
                                fd: err.as_raw_fd(),
                                events: libc::POLLIN,
                                revents: 0,
                            });
                        }
                        if !fds.is_empty() {
                            unsafe {
                                libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, ms);
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(_) => break,
            }
        }

        Ok(output)
    }
}

impl Drop for ManagedProcess {
    fn drop(&mut self) {
        self.kill();
    }
}

// =============================================================================
// Zenoh Availability Check
// =============================================================================

/// Get the path to the locally-built zenohd binary.
///
/// Returns `build/zenohd/zenohd` within the project root.
/// Build it with `just build-zenohd`.
pub fn zenohd_binary_path() -> std::path::PathBuf {
    crate::project_root().join("build/zenohd/zenohd")
}

/// Check if the locally-built zenohd is available.
pub fn is_zenohd_available() -> bool {
    let path = zenohd_binary_path();
    path.exists()
        && Command::new(&path)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// Skip test if zenohd is not available.
///
/// Returns `false` if zenohd is not available, printing a skip message.
/// Returns `true` if zenohd is available and the test should proceed.
///
/// # Example
///
/// ```ignore
/// #[test]
/// fn test_something() {
///     if !require_zenohd() {
///         return;
///     }
///     // ... test code
/// }
/// ```
pub fn require_zenohd() -> bool {
    if !is_zenohd_available() {
        eprintln!("Skipping test: zenohd not found (run `just build-zenohd`)");
        return false;
    }
    true
}

/// Check if cmake is available in PATH
pub fn is_cmake_available() -> bool {
    Command::new("cmake")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Skip test if cmake is not available
///
/// Returns `false` if cmake is not found, printing a skip message.
/// Returns `true` if cmake is available and the test should proceed.
pub fn require_cmake() -> bool {
    if !is_cmake_available() {
        eprintln!("Skipping test: cmake not found");
        return false;
    }
    true
}

/// Check if `docker compose` is available and the Docker daemon is running
pub fn is_docker_compose_available() -> bool {
    Command::new("docker")
        .args(["compose", "version"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
        && Command::new("docker")
            .args(["info"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
}

/// Skip test if Docker Compose is not available
///
/// Returns `false` if Docker Compose or the Docker daemon is unavailable.
/// Returns `true` if Docker is available and the test should proceed.
pub fn require_docker_compose() -> bool {
    if !is_docker_compose_available() {
        eprintln!("Skipping test: docker compose not available");
        return false;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zenohd_detection() {
        let available = is_zenohd_available();
        eprintln!("zenohd available: {}", available);
    }

    #[test]
    fn test_cmake_detection() {
        let available = is_cmake_available();
        eprintln!("cmake available: {}", available);
    }
}
