//! Zephyr process fixture for embedded testing
//!
//! Provides managed Zephyr processes for testing native_sim and QEMU targets.

use crate::process::{kill_process_group, set_new_process_group};
use crate::{TestError, TestResult, project_root};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Zephyr platform variants
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ZephyrPlatform {
    /// Native simulator (x86_64, runs directly on host)
    NativeSim,
    /// QEMU ARM Cortex-M3 emulation (Stellaris LM3S6965)
    QemuArm,
    /// QEMU ARM Cortex-A9 emulation (Xilinx Zynq-7000) with the
    /// Xilinx GEM ethernet driver wired into the Zephyr native IP
    /// stack. Phase 92 — the platform real DDS-on-Zephyr deployments
    /// run on (Zynq, STM32-Eth, NXP-MAC all use the same Cortex-A9
    /// + Zephyr stack code path).
    QemuCortexA9,
}

impl ZephyrPlatform {
    /// Get the west board specifier for this platform
    pub fn board_spec(&self) -> &'static str {
        match self {
            ZephyrPlatform::NativeSim => "native_sim/native/64",
            ZephyrPlatform::QemuArm => "qemu_cortex_m3",
            ZephyrPlatform::QemuCortexA9 => "qemu_cortex_a9",
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
    // Accumulated stdout, grown by the background reader spawned in
    // `start()`. `wait_for_pattern()` polls this buffer for a readiness
    // marker (e.g. "Waiting for messages"), replacing the old fixed
    // sleeps that couldn't keep up with parallel-load cold-boot
    // variance. `wait_for_output()` returns the final snapshot and
    // signals the reader to stop.
    output: std::sync::Arc<std::sync::Mutex<String>>,
    reader_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Joined via Drop when the process is killed; held here so the
    // thread outlives any `wait_for_pattern` / `wait_for_output` call.
    #[allow(dead_code)]
    reader_thread: Option<std::thread::JoinHandle<()>>,
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

        let mut handle = match platform {
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
            ZephyrPlatform::QemuCortexA9 => {
                // For Cortex-A9 the test must call `start_qemu_a9_mcast`
                // instead so it can supply a virtual-L2 mcast group.
                // Calling the bare `start` API doesn't make sense here.
                return Err(TestError::ProcessFailed(String::from(
                    "ZephyrPlatform::QemuCortexA9 requires `start_qemu_a9_mcast` (Phase 92)",
                )));
            }
        };

        // Spawn a background reader that accumulates stdout into a
        // shared buffer. Subsequent `wait_for_pattern()` calls poll
        // this buffer; `wait_for_output()` returns its final snapshot.
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let reader_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_thread = {
            let output = output.clone();
            let reader_done = reader_done.clone();
            let mut stdout = handle.stdout.take().ok_or_else(|| {
                TestError::ProcessFailed("No stdout on spawned process".to_string())
            })?;
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = [0u8; 4096];
                loop {
                    match stdout.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let chunk = String::from_utf8_lossy(&buf[..n]);
                            if let Ok(mut guard) = output.lock() {
                                guard.push_str(&chunk);
                            }
                        }
                        Err(_) => break,
                    }
                }
                reader_done.store(true, std::sync::atomic::Ordering::Release);
            })
        };

        Ok(Self {
            handle,
            platform,
            output,
            reader_done,
            reader_thread: Some(reader_thread),
        })
    }

    /// Launch a Zephyr `qemu_cortex_a9` binary with QEMU's
    /// `-netdev socket,mcast=…` networking. Two instances launched
    /// with the same `mcast_addr:port` share a virtual L2 broadcast
    /// domain on the host (no `sudo`, no TAP), so SPDP/SEDP/ARP all
    /// flow between them. Phase 92.5 talker↔listener interop runs
    /// over this.
    ///
    /// `mac` must be unique per instance — Zephyr's GEM driver uses
    /// the DTS `local-mac-address`, but QEMU still wants its `-nic
    /// mac=` to match for ARP/IGMP plumbing on the host side.
    pub fn start_qemu_a9_mcast(
        binary: &Path,
        mcast_addr_port: &str,
        mac: &str,
    ) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Zephyr binary not found: {}",
                binary.display()
            )));
        }

        // Locate the SDK-bundled qemu-system-xilinx-aarch64 — same
        // path the upstream zephyr-lang-rust samples/philosophers
        // build uses via `west build -t run`. Falling back to the
        // PATH version is fine for dev hosts.
        let qemu = std::env::var("QEMU_BIN").unwrap_or_else(|_| {
            let sdk = "/home/aeon/repos/nano-ros/scripts/zephyr/sdk/zephyr-sdk-0.16.8";
            format!("{sdk}/sysroots/x86_64-pokysdk-linux/usr/bin/qemu-system-xilinx-aarch64")
        });
        let dtb = std::env::var("ZEPHYR_FDT_ZYNQ7000S").unwrap_or_else(|_| {
            let ws = zephyr_workspace_path()
                .map(|p| p.join("zephyr/boards/qemu/cortex_a9/fdt-zynq7000s.dtb"))
                .unwrap_or_default();
            ws.to_string_lossy().into_owned()
        });

        let mut cmd = Command::new(&qemu);
        cmd.args([
            "-nographic",
            "-machine",
            "arm-generic-fdt-7series",
            "-dtb",
            dtb.as_str(),
            "-nic",
            &format!("socket,model=cadence_gem,mcast={mcast_addr_port},mac={mac}"),
            "-chardev",
            "stdio,id=con,mux=on",
            "-serial",
            "chardev:con",
            "-mon",
            "chardev=con,mode=readline",
            "-device",
        ])
        .arg(format!(
            "loader,file={},cpu-num=0",
            binary.to_string_lossy()
        ))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let mut handle = cmd.spawn()?;

        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let reader_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_thread = {
            let output = output.clone();
            let reader_done = reader_done.clone();
            let mut stdout = handle.stdout.take().ok_or_else(|| {
                TestError::ProcessFailed("No stdout on spawned process".to_string())
            })?;
            std::thread::spawn(move || {
                use std::io::Read;
                let mut buf = [0u8; 4096];
                loop {
                    match stdout.read(&mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let chunk = String::from_utf8_lossy(&buf[..n]);
                            if let Ok(mut guard) = output.lock() {
                                guard.push_str(&chunk);
                            }
                        }
                        Err(_) => break,
                    }
                }
                reader_done.store(true, std::sync::atomic::Ordering::Release);
            })
        };

        Ok(Self {
            handle,
            platform: ZephyrPlatform::QemuCortexA9,
            output,
            reader_done,
            reader_thread: Some(reader_thread),
        })
    }

    /// Wait until `pattern` appears in the process's accumulated stdout,
    /// or until `timeout` elapses.
    ///
    /// Returns the output seen so far (whether or not the pattern
    /// matched), so callers can inspect it either way.
    ///
    /// Unlike `wait_for_output`, this does NOT stop the reader thread —
    /// subsequent calls to `wait_for_pattern` or `wait_for_output` keep
    /// seeing new output as it arrives.
    pub fn wait_for_pattern(&self, pattern: &str, timeout: Duration) -> String {
        let deadline = Instant::now() + timeout;
        loop {
            {
                let guard = self.output.lock().expect("output mutex poisoned");
                if guard.contains(pattern) {
                    return guard.clone();
                }
                if self.reader_done.load(std::sync::atomic::Ordering::Acquire) {
                    // Process exited; no more output is coming.
                    return guard.clone();
                }
            }
            if Instant::now() >= deadline {
                return self.output.lock().expect("output mutex poisoned").clone();
            }
            std::thread::sleep(Duration::from_millis(50));
        }
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
        let start = Instant::now();
        let deadline = start + timeout;
        let mut markers_seen = false;

        while Instant::now() < deadline {
            // Check for completion/error markers in the accumulated
            // output. This short-circuits the wait when a Zephyr app
            // emits a known terminal string.
            {
                let guard = self.output.lock().expect("output mutex poisoned");
                if guard.contains("Failed to create context")
                    || guard.contains("session error")
                    || guard.contains("SUCCESS")
                    || guard.contains("COMPLETE")
                {
                    markers_seen = true;
                    break;
                }
            }

            // If the reader has signalled EOF the process ended; no
            // more output is coming.
            if self.reader_done.load(std::sync::atomic::Ordering::Acquire) {
                break;
            }
            if let Ok(Some(_)) = self.handle.try_wait() {
                // Process exited — give the reader a moment to drain
                // the last buffered bytes, then return.
                std::thread::sleep(Duration::from_millis(100));
                break;
            }

            std::thread::sleep(Duration::from_millis(50));
        }

        // Kill the process if still running. The reader thread exits
        // on stdout EOF, which the kill will cause.
        kill_process_group(&mut self.handle);
        let _ = markers_seen;

        let guard = self.output.lock().expect("output mutex poisoned");
        if guard.is_empty() {
            Err(TestError::Timeout)
        } else {
            Ok(guard.clone())
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
        // DDS examples (Phase 71.8)
        "zephyr-dds-rs-talker" | "dds-rs-talker" => "build-dds-rs-talker",
        "zephyr-dds-rs-listener" | "dds-rs-listener" => "build-dds-rs-listener",
        // Phase 92 — same examples on qemu_cortex_a9
        "zephyr-dds-rs-talker-a9" => "build-dds-a9-talker",
        "zephyr-dds-rs-listener-a9" => "build-dds-a9-listener",
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
        // DDS examples (Phase 71.8)
        "zephyr-dds-rs-talker" | "dds-rs-talker" => "zephyr/rust/dds/talker".to_string(),
        "zephyr-dds-rs-listener" | "dds-rs-listener" => "zephyr/rust/dds/listener".to_string(),
        // Phase 92 — same source, qemu_cortex_a9 build dir alias
        "zephyr-dds-rs-talker-a9" => "zephyr/rust/dds/talker".to_string(),
        "zephyr-dds-rs-listener-a9" => "zephyr/rust/dds/listener".to_string(),
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
        ZephyrPlatform::QemuArm | ZephyrPlatform::QemuCortexA9 => {
            workspace.join(format!("{}/zephyr/zephyr.elf", build_dir))
        }
    };

    // If binary exists and we're not forcing a rebuild, check that it's
    // fresher than the example's sources before reusing it. Without this
    // staleness check, a `prj.conf` edit (e.g. the 89.Zephyr per-variant
    // port split) leaves tests using a cached binary that still has the
    // old `CONFIG_NROS_ZENOH_LOCATOR` baked in — binary connects to
    // port 7456 while the test starts zenohd on 7466, test reports
    // `Transport(ConnectionFailed)` or `Init failed: -100` with no
    // hint that the culprit is a stale build.
    if !force_build && binary_path.exists() && !is_binary_stale(&binary_path, example_name) {
        eprintln!("Using existing Zephyr binary: {}", binary_path.display());
        return Ok(binary_path);
    }

    if binary_path.exists() {
        eprintln!(
            "Zephyr binary out-of-date vs sources, rebuilding: {}",
            binary_path.display()
        );
    }

    // Otherwise, build it
    build_zephyr_example(example_name, platform)
}

/// Return true if the built binary is older than any of the example's
/// source inputs (`prj.conf`, `CMakeLists.txt`, every file under `src/`).
fn is_binary_stale(binary_path: &Path, example_name: &str) -> bool {
    let Ok(binary_mtime) = binary_path.metadata().and_then(|m| m.modified()) else {
        // Can't stat the binary — assume stale so we rebuild and get a
        // real error instead of reusing something mysterious.
        return true;
    };

    let example_dir = project_root()
        .join("examples")
        .join(example_path_for_name(example_name));

    // `prj.conf` and `CMakeLists.txt` are the common edit points; src/
    // covers main.c / main.cpp / Cargo.toml drift.
    let candidates = [
        example_dir.join("prj.conf"),
        example_dir.join("CMakeLists.txt"),
    ];
    for p in &candidates {
        if let Ok(src_mtime) = p.metadata().and_then(|m| m.modified())
            && src_mtime > binary_mtime
        {
            return true;
        }
    }

    // Walk src/ one level deep — sufficient for every example we ship.
    if let Ok(iter) = std::fs::read_dir(example_dir.join("src")) {
        for entry in iter.flatten() {
            if let Ok(src_mtime) = entry.metadata().and_then(|m| m.modified())
                && src_mtime > binary_mtime
            {
                return true;
            }
        }
    }

    false
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

    let binary_path = match platform {
        ZephyrPlatform::NativeSim => workspace.join(format!("{}/zephyr/zephyr.exe", build_dir)),
        ZephyrPlatform::QemuArm | ZephyrPlatform::QemuCortexA9 => {
            workspace.join(format!("{}/zephyr/zephyr.elf", build_dir))
        }
    };

    crate::fixtures::require_prebuilt_binary(&binary_path)
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
