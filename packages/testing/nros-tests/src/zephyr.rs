//! Zephyr process fixture for embedded testing
//!
//! Provides managed Zephyr processes for testing native_sim and QEMU targets.

use crate::process::{kill_process_group, set_new_process_group};
use crate::{TestError, TestResult, project_root};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Zephyr platform variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZephyrPlatform {
    /// Native simulator (x86_64, runs directly on host)
    NativeSim,
    /// QEMU ARM Cortex-M3 emulation
    QemuArm,
}

impl ZephyrPlatform {
    /// Get the west board specifier for this platform
    pub fn board_spec(&self) -> &'static str {
        match self {
            ZephyrPlatform::NativeSim => "native_sim/native/64",
            ZephyrPlatform::QemuArm => "qemu_cortex_m3",
        }
    }
}

/// Managed Zephyr process for native_sim or QEMU
///
/// Starts a Zephyr application and captures output.
/// Automatically kills the process on drop.
///
/// # Example
///
/// ```ignore
/// use nros_tests::zephyr::{ZephyrProcess, ZephyrPlatform};
/// use std::path::Path;
/// use std::time::Duration;
///
/// let workspace = zephyr_workspace_path().unwrap();
/// let binary = workspace.join("build/zephyr/zephyr.exe");
/// let mut zephyr = ZephyrProcess::start(&binary, ZephyrPlatform::NativeSim).unwrap();
/// let output = zephyr.wait_for_output(Duration::from_secs(15)).unwrap();
/// ```
pub struct ZephyrProcess {
    handle: Child,
    platform: ZephyrPlatform,
}

/// Atomic counter to ensure each Zephyr process gets a unique seed
static SEED_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

impl ZephyrProcess {
    /// Start a Zephyr application
    ///
    /// # Arguments
    /// * `binary` - Path to the Zephyr executable (zephyr.exe for native_sim, zephyr.elf for QEMU)
    /// * `platform` - Target platform
    ///
    /// # Returns
    /// A managed Zephyr process
    pub fn start(binary: &Path, platform: ZephyrPlatform) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Zephyr binary not found: {}",
                binary.display()
            )));
        }

        let handle = match platform {
            ZephyrPlatform::NativeSim => {
                // native_sim runs directly
                // Each process needs a unique --seed to prevent ephemeral port conflicts
                // (the test entropy source produces identical random numbers without different seeds).
                // Use current time nanos as a random base; the atomic counter ensures
                // two processes spawned in the same test get different seeds.
                use std::time::{SystemTime, UNIX_EPOCH};
                let base = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .subsec_nanos();
                let offset = SEED_COUNTER.fetch_add(10000, std::sync::atomic::Ordering::Relaxed);
                let seed = base.wrapping_add(offset);
                let mut cmd = Command::new(binary);
                cmd.arg(format!("--seed={}", seed))
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped());
                #[cfg(unix)]
                set_new_process_group(&mut cmd);
                cmd.spawn()?
            }
            ZephyrPlatform::QemuArm => {
                // QEMU ARM requires qemu-system-arm
                let mut cmd = Command::new("qemu-system-arm");
                cmd.args([
                    "-cpu",
                    "cortex-m3",
                    "-machine",
                    "lm3s6965evb",
                    "-nographic",
                    "-kernel",
                ])
                .arg(binary)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());
                #[cfg(unix)]
                set_new_process_group(&mut cmd);
                cmd.spawn()?
            }
        };

        Ok(Self { handle, platform })
    }

    /// Get the platform this process is running on
    pub fn platform(&self) -> ZephyrPlatform {
        self.platform
    }

    /// Wait for output with timeout
    ///
    /// Collects stdout from the process. Since Zephyr native_sim processes
    /// typically output everything quickly and then wait indefinitely,
    /// this uses a thread to avoid blocking on read().
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    /// The collected stdout as a string
    pub fn wait_for_output(&mut self, timeout: Duration) -> TestResult<String> {
        use std::sync::mpsc;
        use std::thread;

        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed("No stdout".to_string()))?;

        // Spawn a thread to read stdout (avoids blocking)
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let mut output = String::new();
            let mut buffer = [0u8; 4096];
            loop {
                match stdout.read(&mut buffer) {
                    Ok(0) => break, // EOF
                    Ok(n) => {
                        output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                        // Send partial output
                        let _ = tx.send(output.clone());
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for output with timeout
        let start = Instant::now();
        let mut last_output = String::new();

        while start.elapsed() < timeout {
            // Check if process exited
            if let Ok(Some(_)) = self.handle.try_wait() {
                // Process exited, collect any remaining output
                while let Ok(output) = rx.recv_timeout(Duration::from_millis(50)) {
                    last_output = output;
                }
                break;
            }

            // Wait for new output with bounded timeout
            let remaining = timeout.saturating_sub(start.elapsed());
            let wait = remaining.min(Duration::from_millis(500));
            match rx.recv_timeout(wait) {
                Ok(output) => {
                    last_output = output;

                    // Check for completion/error markers (Zephyr outputs error and stops)
                    if last_output.contains("Failed to create context")
                        || last_output.contains("session error")
                        || last_output.contains("SUCCESS")
                        || last_output.contains("COMPLETE")
                    {
                        // Drain any trailing output
                        while let Ok(output) = rx.recv_timeout(Duration::from_millis(50)) {
                            last_output = output;
                        }
                        break;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => continue,
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
            }
        }

        // Kill the process if still running
        kill_process_group(&mut self.handle);

        if last_output.is_empty() {
            Err(TestError::Timeout)
        } else {
            Ok(last_output)
        }
    }

    /// Kill the Zephyr process
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
    }

    /// Check if process is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }
}

impl Drop for ZephyrProcess {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
    }
}

// =============================================================================
// Zephyr Availability Checks
// =============================================================================

/// Get the path to the Zephyr workspace
///
/// Checks in order:
/// 1. `ZEPHYR_NANO_ROS` environment variable
/// 2. `zephyr-workspace` symlink in project root
/// 3. Sibling workspace `../nano-ros-workspace/`
///
/// # Returns
/// Path to the workspace, or None if not found
pub fn zephyr_workspace_path() -> Option<PathBuf> {
    // 1. Environment variable
    if let Ok(path) = std::env::var("ZEPHYR_NANO_ROS") {
        let path = PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    let root = project_root();

    // 2. zephyr-workspace symlink
    let symlink = root.join("zephyr-workspace");
    if (symlink.is_symlink() || symlink.is_dir())
        && let Ok(resolved) = std::fs::canonicalize(&symlink)
        && resolved.exists()
    {
        return Some(resolved);
    }

    // 3. Sibling workspace
    let sibling = root.parent()?.join("nano-ros-workspace");
    if sibling.exists() {
        return Some(sibling);
    }

    None
}

/// Check if west command is available
pub fn is_west_available() -> bool {
    Command::new("west")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if Zephyr workspace is configured
pub fn is_zephyr_workspace_available() -> bool {
    zephyr_workspace_path()
        .map(|p| p.join("zephyr").exists())
        .unwrap_or(false)
}

/// Check if all Zephyr prerequisites are available
///
/// Checks:
/// - west command available
/// - Zephyr workspace configured
///
/// Networking on native_sim uses NSOS (host loopback), so no TAP/bridge setup
/// is required.
pub fn is_zephyr_available() -> bool {
    is_west_available() && is_zephyr_workspace_available()
}

/// Skip test if Zephyr is not available
///
/// Returns `false` if Zephyr prerequisites are not met, printing a skip message.
/// Returns `true` if Zephyr is available and the test should proceed.
pub fn require_zephyr() -> bool {
    if !is_west_available() {
        eprintln!("Skipping test: west not found");
        return false;
    }
    if !is_zephyr_workspace_available() {
        eprintln!("Skipping test: Zephyr workspace not found");
        eprintln!("  Run: ./scripts/zephyr/setup.sh");
        return false;
    }
    true
}

// =============================================================================
// Zephyr Build Helpers
// =============================================================================

use once_cell::sync::OnceCell;

/// Cached path to built zephyr-rs-talker binary
static ZEPHYR_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Cached path to built zephyr-rs-listener binary
static ZEPHYR_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();

/// Get the build directory name for an example
///
/// Returns a unique build directory to allow simultaneous builds of talker and listener.
fn build_dir_for_example(example_name: &str) -> &'static str {
    match example_name {
        "zephyr-rs-talker" | "rs-talker" => "build-talker",
        "zephyr-rs-listener" | "rs-listener" => "build-listener",
        "zephyr-rs-action-server" | "rs-action-server" => "build-action-server",
        "zephyr-rs-action-client" | "rs-action-client" => "build-action-client",
        "zephyr-rs-service-server" | "rs-service-server" => "build-service-server",
        "zephyr-rs-service-client" | "rs-service-client" => "build-service-client",
        // C++ examples
        "zephyr-cpp-talker" | "cpp-talker" => "build-cpp-talker",
        "zephyr-cpp-listener" | "cpp-listener" => "build-cpp-listener",
        "zephyr-cpp-service-server" | "cpp-service-server" => "build-cpp-service-server",
        "zephyr-cpp-service-client" | "cpp-service-client" => "build-cpp-service-client",
        "zephyr-cpp-action-server" | "cpp-action-server" => "build-cpp-action-server",
        "zephyr-cpp-action-client" | "cpp-action-client" => "build-cpp-action-client",
        // XRCE examples
        "zephyr-xrce-rs-talker" | "xrce-rs-talker" => "build-xrce-rs-talker",
        "zephyr-xrce-rs-listener" | "xrce-rs-listener" => "build-xrce-rs-listener",
        "zephyr-xrce-c-talker" | "xrce-c-talker" => "build-xrce-c-talker",
        "zephyr-xrce-c-listener" | "xrce-c-listener" => "build-xrce-c-listener",
        _ => "build",
    }
}

/// Convert example name to the actual path under examples/
///
/// Handles both legacy names (zephyr-rs-talker) and new names (rs-talker).
/// Returns path relative to examples/ directory.
fn example_path_for_name(example_name: &str) -> String {
    match example_name {
        "zephyr-rs-talker" | "rs-talker" => "zephyr/rust/zenoh/talker".to_string(),
        "zephyr-rs-listener" | "rs-listener" => "zephyr/rust/zenoh/listener".to_string(),
        "zephyr-rs-action-server" | "rs-action-server" => {
            "zephyr/rust/zenoh/action-server".to_string()
        }
        "zephyr-rs-action-client" | "rs-action-client" => {
            "zephyr/rust/zenoh/action-client".to_string()
        }
        "zephyr-rs-service-server" | "rs-service-server" => {
            "zephyr/rust/zenoh/service-server".to_string()
        }
        "zephyr-rs-service-client" | "rs-service-client" => {
            "zephyr/rust/zenoh/service-client".to_string()
        }
        "zephyr-c-talker" | "c-talker" => "zephyr/c/zenoh/talker".to_string(),
        "zephyr-c-listener" | "c-listener" => "zephyr/c/zenoh/listener".to_string(),
        // C++ examples
        "zephyr-cpp-talker" | "cpp-talker" => "zephyr/cpp/zenoh/talker".to_string(),
        "zephyr-cpp-listener" | "cpp-listener" => "zephyr/cpp/zenoh/listener".to_string(),
        "zephyr-cpp-service-server" | "cpp-service-server" => {
            "zephyr/cpp/zenoh/service-server".to_string()
        }
        "zephyr-cpp-service-client" | "cpp-service-client" => {
            "zephyr/cpp/zenoh/service-client".to_string()
        }
        "zephyr-cpp-action-server" | "cpp-action-server" => {
            "zephyr/cpp/zenoh/action-server".to_string()
        }
        "zephyr-cpp-action-client" | "cpp-action-client" => {
            "zephyr/cpp/zenoh/action-client".to_string()
        }
        // XRCE examples
        "zephyr-xrce-rs-talker" | "xrce-rs-talker" => "zephyr/rust/xrce/talker".to_string(),
        "zephyr-xrce-rs-listener" | "xrce-rs-listener" => "zephyr/rust/xrce/listener".to_string(),
        "zephyr-xrce-c-talker" | "xrce-c-talker" => "zephyr/c/xrce/talker".to_string(),
        "zephyr-xrce-c-listener" | "xrce-c-listener" => "zephyr/c/xrce/listener".to_string(),
        // For any other name, assume it's a path relative to examples/
        _ => example_name.to_string(),
    }
}

/// Get path to Zephyr binary, using existing build if available
///
/// This function checks if a Zephyr binary already exists in the build directory
/// and returns it without rebuilding. Only builds if forced or binary doesn't exist.
///
/// # Arguments
/// * `example_name` - Name of the example directory (e.g., "zephyr-rs-talker")
/// * `platform` - Target platform
/// * `force_build` - If true, always rebuild even if binary exists
///
/// # Returns
/// Path to the binary
pub fn get_or_build_zephyr_example(
    example_name: &str,
    platform: ZephyrPlatform,
    force_build: bool,
) -> TestResult<PathBuf> {
    let workspace = zephyr_workspace_path()
        .ok_or_else(|| TestError::BuildFailed("Zephyr workspace not found".to_string()))?;

    let build_dir = build_dir_for_example(example_name);

    // Determine binary path based on platform
    let binary_path = match platform {
        ZephyrPlatform::NativeSim => workspace.join(format!("{}/zephyr/zephyr.exe", build_dir)),
        ZephyrPlatform::QemuArm => workspace.join(format!("{}/zephyr/zephyr.elf", build_dir)),
    };

    // If binary exists and we're not forcing a rebuild, use it
    if !force_build && binary_path.exists() {
        eprintln!("Using existing Zephyr binary: {}", binary_path.display());
        return Ok(binary_path);
    }

    // Otherwise, build it
    build_zephyr_example(example_name, platform)
}

/// Build a Zephyr example using west (cached)
///
/// For zephyr-rs-talker and zephyr-rs-listener, results are cached to avoid
/// repeated builds within the same test run.
pub fn build_zephyr_example_cached(
    example_name: &str,
    platform: ZephyrPlatform,
) -> TestResult<&'static Path> {
    match example_name {
        "zephyr-rs-talker" => ZEPHYR_TALKER_BINARY
            .get_or_try_init(|| build_zephyr_example(example_name, platform))
            .map(|p| p.as_path()),
        "zephyr-rs-listener" => ZEPHYR_LISTENER_BINARY
            .get_or_try_init(|| build_zephyr_example(example_name, platform))
            .map(|p| p.as_path()),
        _ => build_zephyr_example(example_name, platform)
            .map(|p| Box::leak(Box::new(p)) as &'static Path),
    }
}

/// Build a Zephyr example using west
///
/// Each example is built to its own directory (build-talker/, build-listener/)
/// to allow both to exist simultaneously.
///
/// # Arguments
/// * `example_name` - Name of the example directory (e.g., "zephyr-rs-talker")
/// * `platform` - Target platform
///
/// # Returns
/// Path to the built binary
pub fn build_zephyr_example(example_name: &str, platform: ZephyrPlatform) -> TestResult<PathBuf> {
    let workspace = zephyr_workspace_path()
        .ok_or_else(|| TestError::BuildFailed("Zephyr workspace not found".to_string()))?;

    let root = project_root();
    let example_rel_path = example_path_for_name(example_name);
    let example_path = root.join("examples").join(&example_rel_path);

    if !example_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Example not found: {}",
            example_path.display()
        )));
    }

    let build_dir = build_dir_for_example(example_name);
    eprintln!(
        "Building {} for {} (build dir: {})...",
        example_name,
        platform.board_spec(),
        build_dir
    );

    // Build with separate build directory for each example
    // This allows talker and listener to coexist
    //
    // Add build/install/bin to PATH so nros-codegen is found by
    // nros_generate_interfaces() in C API examples.
    let install_bin = root.join("build/install/bin");
    let path_env = match std::env::var("PATH") {
        Ok(existing) => format!("{}:{}", install_bin.display(), existing),
        Err(_) => install_bin.display().to_string(),
    };

    // Pass CMAKE_PREFIX_PATH so nros_generate_interfaces() finds nros-codegen
    let cmake_prefix = format!(
        "-DCMAKE_PREFIX_PATH={}",
        root.join("build/install").display()
    );
    let output = Command::new("west")
        .args([
            "build",
            "-b",
            platform.board_spec(),
            "-d",
            build_dir,
            "-p",
            "auto",
        ])
        .arg(&example_path)
        .args(["--", &cmake_prefix])
        .current_dir(&workspace)
        .env("ZEPHYR_BASE", workspace.join("zephyr"))
        .env("PATH", &path_env)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| TestError::BuildFailed(format!("Failed to run west: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(TestError::BuildFailed(format!(
            "west build failed:\nstdout: {}\nstderr: {}",
            stdout, stderr
        )));
    }

    // Determine binary path based on platform
    let binary_path = match platform {
        ZephyrPlatform::NativeSim => workspace.join(format!("{}/zephyr/zephyr.exe", build_dir)),
        ZephyrPlatform::QemuArm => workspace.join(format!("{}/zephyr/zephyr.elf", build_dir)),
    };

    if !binary_path.exists() {
        return Err(TestError::BuildFailed(format!(
            "Binary not found after build: {}",
            binary_path.display()
        )));
    }

    Ok(binary_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_board_spec() {
        assert_eq!(
            ZephyrPlatform::NativeSim.board_spec(),
            "native_sim/native/64"
        );
        assert_eq!(ZephyrPlatform::QemuArm.board_spec(), "qemu_cortex_m3");
    }

    #[test]
    fn test_west_detection() {
        let available = is_west_available();
        eprintln!("west available: {}", available);
    }

    #[test]
    fn test_workspace_detection() {
        if let Some(path) = zephyr_workspace_path() {
            eprintln!("Zephyr workspace: {}", path.display());
            assert!(path.exists());
        } else {
            eprintln!("Zephyr workspace not found");
        }
    }
}
