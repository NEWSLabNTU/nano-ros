//! Integration test framework for nros
//!
//! This crate provides fixtures and utilities for testing nros components:
//! - Process management (zenohd, QEMU, Zephyr)
//! - Binary building helpers
//! - Output assertion utilities
//!
//! # Example
//!
//! ```ignore
//! use nros_tests::fixtures::zenohd;
//! use rstest::rstest;
//!
//! #[rstest]
//! fn test_pubsub(zenohd: ZenohRouter) {
//!     // zenohd is automatically started and cleaned up
//! }
//! ```

pub mod esp32;
pub mod fixtures;
pub mod platform;
pub mod process;
pub mod qemu;
pub mod ros2;
pub mod zephyr;

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::process::{Child, ChildStdout};
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// Intra-process counter for multiple `unique_domain_id()` calls in one test.
static DOMAIN_SEQ: AtomicU32 = AtomicU32::new(0);

/// Returns a unique ROS domain ID for test isolation.
///
/// Nextest runs each test in a separate process, so the PID is unique across
/// concurrent tests. The low 8 bits hold an intra-process sequence counter
/// for the rare case where one test needs multiple distinct domain IDs.
///
/// This avoids the pitfall of a global `AtomicU32` counter that resets per
/// process — all processes would start at the same value.
pub fn unique_domain_id() -> u32 {
    let pid = std::process::id();
    let seq = DOMAIN_SEQ.fetch_add(1, Ordering::Relaxed);
    (pid << 8) | (seq & 0xFF)
}

/// Poll a file descriptor for readability using poll(2).
///
/// Returns `true` if the fd is readable, `false` on timeout.
#[cfg(unix)]
fn poll_readable(fd: std::os::unix::io::RawFd, timeout_ms: i32) -> bool {
    let mut fds = [libc::pollfd {
        fd,
        events: libc::POLLIN,
        revents: 0,
    }];
    // Safety: valid pollfd struct, single element
    let ret = unsafe { libc::poll(fds.as_mut_ptr(), 1, timeout_ms) };
    ret > 0 && (fds[0].revents & libc::POLLIN) != 0
}

/// Error type for test utilities
#[derive(Debug, thiserror::Error)]
pub enum TestError {
    #[error("Process failed to start: {0}")]
    ProcessStart(#[from] std::io::Error),

    #[error("Process failed: {0}")]
    ProcessFailed(String),

    #[error("Timeout waiting for condition")]
    Timeout,

    #[error("Build failed: {0}")]
    BuildFailed(String),

    #[error("Output parsing error: {0}")]
    OutputParse(String),
}

pub type TestResult<T> = Result<T, TestError>;

/// Wait for a TCP port to become available
///
/// # Arguments
/// * `port` - The port number to check
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// `true` if the port is available within the timeout, `false` otherwise
pub fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    let addr = format!("127.0.0.1:{}", port);

    while start.elapsed() < timeout {
        if TcpStream::connect(&addr).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Wait for a TCP port to become available on a specific address
///
/// Like [`wait_for_port`] but checks a specific IP instead of localhost.
/// Useful for verifying zenohd is reachable on a specific address
/// (e.g., a host-forwarded port or a veth bridge IP).
pub fn wait_for_port_on(addr: &str, port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    let target = format!("{}:{}", addr, port);

    while start.elapsed() < timeout {
        if TcpStream::connect(&target).is_ok() {
            return true;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    false
}

/// Wait for a specific pattern in process output
///
/// # Arguments
/// * `reader` - A buffered reader from the process stdout
/// * `pattern` - The pattern to search for
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// The matching line if found within timeout
pub fn wait_for_pattern(
    reader: &mut BufReader<ChildStdout>,
    pattern: &str,
    timeout: Duration,
) -> TestResult<String> {
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;

    let start = Instant::now();
    let mut line = String::new();

    #[cfg(unix)]
    let fd = reader.get_ref().as_raw_fd();

    while start.elapsed() < timeout {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => {
                // EOF — wait for more data via poll(2)
                let remaining = timeout.saturating_sub(start.elapsed());
                #[cfg(unix)]
                {
                    let ms = remaining.as_millis().min(500) as i32;
                    poll_readable(fd, ms);
                }
                #[cfg(not(unix))]
                std::thread::sleep(remaining.min(Duration::from_millis(50)));
                continue;
            }
            Ok(_) => {
                if line.contains(pattern) {
                    return Ok(line);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                let remaining = timeout.saturating_sub(start.elapsed());
                #[cfg(unix)]
                {
                    let ms = remaining.as_millis().min(500) as i32;
                    poll_readable(fd, ms);
                }
                #[cfg(not(unix))]
                std::thread::sleep(remaining.min(Duration::from_millis(50)));
                continue;
            }
            Err(e) => return Err(TestError::ProcessStart(e)),
        }
    }
    Err(TestError::Timeout)
}

/// Collect all output from a process until it exits or timeout
///
/// # Arguments
/// * `child` - The child process
/// * `timeout` - Maximum time to wait
///
/// # Returns
/// The collected stdout as a string
pub fn collect_output(mut child: Child, timeout: Duration) -> TestResult<String> {
    use std::io::Read;
    #[cfg(unix)]
    use std::os::unix::io::AsRawFd;

    let start = Instant::now();
    let mut output = String::new();

    if let Some(mut stdout) = child.stdout.take() {
        #[cfg(unix)]
        let fd = stdout.as_raw_fd();

        // Set up non-blocking read with timeout
        let mut buffer = [0u8; 4096];
        while start.elapsed() < timeout {
            match stdout.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    let remaining = timeout.saturating_sub(start.elapsed());
                    #[cfg(unix)]
                    {
                        let ms = remaining.as_millis().min(500) as i32;
                        poll_readable(fd, ms);
                    }
                    #[cfg(not(unix))]
                    std::thread::sleep(remaining.min(Duration::from_millis(50)));
                }
                Err(_) => break,
            }

            // Check if process exited
            if let Ok(Some(_)) = child.try_wait() {
                // Read any remaining output
                let _ = stdout.read_to_string(&mut output);
                break;
            }
        }
    }

    // Ensure process is terminated
    process::kill_process_group(&mut child);

    Ok(output)
}

/// Assert that output contains all specified patterns
///
/// # Arguments
/// * `output` - The output string to check
/// * `patterns` - Patterns that must all be present
///
/// # Panics
/// If any pattern is not found in the output
pub fn assert_output_contains(output: &str, patterns: &[&str]) {
    for pattern in patterns {
        assert!(
            output.contains(pattern),
            "Expected output to contain '{}', but it was not found.\nOutput:\n{}",
            pattern,
            output
        );
    }
}

/// Assert that output contains none of the specified patterns
///
/// # Arguments
/// * `output` - The output string to check
/// * `patterns` - Patterns that must not be present
///
/// # Panics
/// If any pattern is found in the output
pub fn assert_output_excludes(output: &str, patterns: &[&str]) {
    for pattern in patterns {
        assert!(
            !output.contains(pattern),
            "Expected output to NOT contain '{}', but it was found.\nOutput:\n{}",
            pattern,
            output
        );
    }
}

/// Count occurrences of a pattern in output
pub fn count_pattern(output: &str, pattern: &str) -> usize {
    output.matches(pattern).count()
}

/// Get the project root directory
pub fn project_root() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_root() {
        let root = project_root();
        assert!(root.join("Cargo.toml").exists());
        assert!(root.join("packages").exists());
    }

    #[test]
    fn test_count_pattern() {
        let output = "[PASS] test1\n[PASS] test2\n[FAIL] test3\n[PASS] test4";
        assert_eq!(count_pattern(output, "[PASS]"), 3);
        assert_eq!(count_pattern(output, "[FAIL]"), 1);
    }

    #[test]
    fn test_assert_output_contains() {
        let output = "Hello world\nTest passed";
        assert_output_contains(output, &["Hello", "passed"]);
    }

    #[test]
    #[should_panic(expected = "Expected output to contain")]
    fn test_assert_output_contains_fails() {
        let output = "Hello world";
        assert_output_contains(output, &["missing"]);
    }

    #[test]
    fn test_unique_domain_id() {
        let id1 = unique_domain_id();
        let id2 = unique_domain_id();
        // PID-based, so non-zero
        assert!(id1 > 0);
        // Sequential calls differ in the low 8 bits (intra-process counter)
        assert_ne!(id1, id2);
        assert_eq!(id2 - id1, 1);
    }
}
