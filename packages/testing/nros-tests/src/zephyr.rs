//! Zephyr process fixture for embedded testing
//!
//! Provides managed Zephyr processes for testing native_sim and QEMU targets.

use crate::{
    TestError, TestResult,
    process::{kill_process_group, set_new_process_group},
    project_root,
};
use std::{
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

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
    // Accumulated stdout+stderr, grown by the background readers spawned in
    // `start()`. `wait_for_pattern()` polls this buffer for a readiness
    // marker (e.g. "Waiting for messages"), replacing the old fixed
    // sleeps that couldn't keep up with parallel-load cold-boot
    // variance. `wait_for_output()` returns the final snapshot and
    // signals the reader to stop.
    output: std::sync::Arc<std::sync::Mutex<String>>,
    reader_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    // Joined via Drop when the process is killed; held here so the
    // threads outlive any `wait_for_pattern` / `wait_for_output` call.
    #[allow(dead_code)]
    reader_threads: Vec<std::thread::JoinHandle<()>>,
}

/// Atomic counter to ensure each Zephyr process gets a unique seed
static SEED_COUNTER: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

fn spawn_output_readers(
    handle: &mut Child,
    output: std::sync::Arc<std::sync::Mutex<String>>,
    reader_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> TestResult<Vec<std::thread::JoinHandle<()>>> {
    let stdout = handle
        .stdout
        .take()
        .ok_or_else(|| TestError::ProcessFailed("No stdout on spawned process".to_string()))?;
    let stderr = handle
        .stderr
        .take()
        .ok_or_else(|| TestError::ProcessFailed("No stderr on spawned process".to_string()))?;

    let remaining = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(2));
    Ok(vec![
        spawn_stream_reader(
            stdout,
            output.clone(),
            reader_done.clone(),
            remaining.clone(),
        ),
        spawn_stream_reader(stderr, output, reader_done, remaining),
    ])
}

fn spawn_stream_reader<R>(
    mut stream: R,
    output: std::sync::Arc<std::sync::Mutex<String>>,
    reader_done: std::sync::Arc<std::sync::atomic::AtomicBool>,
    remaining: std::sync::Arc<std::sync::atomic::AtomicUsize>,
) -> std::thread::JoinHandle<()>
where
    R: std::io::Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut buf = [0u8; 4096];
        loop {
            match stream.read(&mut buf) {
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
        if remaining.fetch_sub(1, std::sync::atomic::Ordering::AcqRel) == 1 {
            reader_done.store(true, std::sync::atomic::Ordering::Release);
        }
    })
}

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
        Self::start_inner(binary, platform, &[])
    }

    /// #166 / phase-286 W1 — start a native_sim image with a per-test router
    /// locator override, passed as `-testargs --nros-locator=<locator>`. The
    /// image's `nros_runtime_locator_override()` reads it (via
    /// `nsi_get_test_cmd_line_args`) and dials THIS locator instead of the
    /// build-time-baked port, so each test can run its own ephemeral zenohd and
    /// the zenoh e2e lanes need not serialize on a shared baked port.
    ///
    /// native_sim only: `-testargs` is a native-simulator feature; QEMU /
    /// hardware images ignore the override and keep their baked locator.
    pub fn start_with_locator(
        binary: &Path,
        platform: ZephyrPlatform,
        locator: &str,
    ) -> TestResult<Self> {
        Self::start_inner(binary, platform, &[format!("--nros-locator={locator}")])
    }

    fn start_inner(
        binary: &Path,
        platform: ZephyrPlatform,
        testargs: &[String],
    ) -> TestResult<Self> {
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
                // #166 — everything after `-testargs` goes to the native-sim
                // "test args" argv (bypassing native_sim's own option parser,
                // which would abort on an unregistered `--nros-locator`).
                if !testargs.is_empty() {
                    cmd.arg("-testargs");
                    cmd.args(testargs);
                }
                #[cfg(unix)]
                set_new_process_group(&mut cmd);
                cmd.spawn()?
            }
            ZephyrPlatform::QemuArm => {
                // QEMU ARM requires qemu-system-arm; Phase 143 routes
                // this through the patched build under `build/qemu/`
                // when present.
                let mut cmd = crate::qemu::qemu_system_arm_cmd();
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

        // Spawn background readers that accumulate stdout and stderr into
        // a shared buffer. Subsequent `wait_for_pattern()` calls poll
        // this buffer; `wait_for_output()` returns its final snapshot.
        let output = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
        let reader_done = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let reader_threads =
            spawn_output_readers(&mut handle, output.clone(), reader_done.clone())?;

        Ok(Self {
            handle,
            platform,
            output,
            reader_done,
            reader_threads,
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
        let reader_threads =
            spawn_output_readers(&mut handle, output.clone(), reader_done.clone())?;

        Ok(Self {
            handle,
            platform: ZephyrPlatform::QemuCortexA9,
            output,
            reader_done,
            reader_threads,
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
        for thread in std::mem::take(&mut self.reader_threads) {
            let _ = thread.join();
        }
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

fn zephyr_build_root(workspace: &Path) -> PathBuf {
    if let Some(path) = std::env::var_os("NROS_ZEPHYR_BUILD_ROOT") {
        return PathBuf::from(path);
    }
    if workspace_is_writable(workspace) {
        workspace.to_path_buf()
    } else {
        project_root().join("build/zephyr-workspace-builds")
    }
}

fn workspace_is_writable(path: &Path) -> bool {
    path.metadata()
        .map(|m| !m.permissions().readonly())
        .unwrap_or(false)
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
// Zephyr Fixture Helpers
// =============================================================================

/// Get the build directory name for an example
///
/// Returns a unique build directory to allow simultaneous builds of talker and listener.
/// Phase 168.6.B — alias → (lang, case, rmw, board-suffix) decoder.
///
/// Legacy alias names (kept for caller-source stability) are mapped
/// to the collapsed Phase 168 shape. The build directory and
/// example path both follow `build-<lang>-<case>-<rmw>[<-board>]`
/// / `examples/zephyr/<lang>/<case>` respectively.
fn decode_alias(
    example_name: &str,
) -> Option<(&'static str, &'static str, &'static str, &'static str)> {
    // (lang, case, rmw, board_suffix)
    Some(match example_name {
        // Rust zenoh
        "zephyr-rs-talker" | "rs-talker" => ("rust", "talker", "zenoh", ""),
        "zephyr-rs-listener" | "rs-listener" => ("rust", "listener", "zenoh", ""),
        "zephyr-rs-action-server" | "rs-action-server" => ("rust", "action-server", "zenoh", ""),
        "zephyr-rs-action-client" | "rs-action-client" => ("rust", "action-client", "zenoh", ""),
        "zephyr-rs-service-server" | "rs-service-server" => ("rust", "service-server", "zenoh", ""),
        "zephyr-rs-service-client" | "rs-service-client" => ("rust", "service-client", "zenoh", ""),
        // C++ zenoh
        "zephyr-cpp-talker" | "cpp-talker" => ("cpp", "talker", "zenoh", ""),
        "zephyr-cpp-listener" | "cpp-listener" => ("cpp", "listener", "zenoh", ""),
        "zephyr-cpp-service-server" | "cpp-service-server" => {
            ("cpp", "service-server", "zenoh", "")
        }
        "zephyr-cpp-service-client" | "cpp-service-client" => {
            ("cpp", "service-client", "zenoh", "")
        }
        "zephyr-cpp-action-server" | "cpp-action-server" => ("cpp", "action-server", "zenoh", ""),
        "zephyr-cpp-action-client" | "cpp-action-client" => ("cpp", "action-client", "zenoh", ""),
        // C zenoh
        "zephyr-c-talker" | "c-talker" => ("c", "talker", "zenoh", ""),
        "zephyr-c-listener" | "c-listener" => ("c", "listener", "zenoh", ""),
        // XRCE Rust
        "zephyr-xrce-rs-talker" | "xrce-rs-talker" => ("rust", "talker", "xrce", ""),
        "zephyr-xrce-rs-listener" | "xrce-rs-listener" => ("rust", "listener", "xrce", ""),
        "zephyr-xrce-rs-service-server" | "xrce-rs-service-server" => {
            ("rust", "service-server", "xrce", "")
        }
        "zephyr-xrce-rs-service-client" | "xrce-rs-service-client" => {
            ("rust", "service-client", "xrce", "")
        }
        "zephyr-xrce-rs-action-server" | "xrce-rs-action-server" => {
            ("rust", "action-server", "xrce", "")
        }
        "zephyr-xrce-rs-action-client" | "xrce-rs-action-client" => {
            ("rust", "action-client", "xrce", "")
        }
        // XRCE C
        "zephyr-xrce-c-talker" | "xrce-c-talker" => ("c", "talker", "xrce", ""),
        "zephyr-xrce-c-listener" | "xrce-c-listener" => ("c", "listener", "xrce", ""),
        // XRCE C++
        "zephyr-xrce-cpp-talker" | "xrce-cpp-talker" => ("cpp", "talker", "xrce", ""),
        "zephyr-xrce-cpp-listener" | "xrce-cpp-listener" => ("cpp", "listener", "xrce", ""),
        "zephyr-xrce-cpp-service-server" | "xrce-cpp-service-server" => {
            ("cpp", "service-server", "xrce", "")
        }
        "zephyr-xrce-cpp-service-client" | "xrce-cpp-service-client" => {
            ("cpp", "service-client", "xrce", "")
        }
        "zephyr-xrce-cpp-action-server" | "xrce-cpp-action-server" => {
            ("cpp", "action-server", "xrce", "")
        }
        "zephyr-xrce-cpp-action-client" | "xrce-cpp-action-client" => {
            ("cpp", "action-client", "xrce", "")
        }
        // Cyclone DDS — C / C++ today; Rust path lands once Phase 169.5
        // ships `nros-rmw-cyclonedds-sys`. Legacy `zephyr-dds-*` aliases
        // map to cyclonedds for source compatibility after Phase 169.4.
        "zephyr-dds-cpp-talker" => ("cpp", "talker", "cyclonedds", ""),
        "zephyr-dds-cpp-listener" => ("cpp", "listener", "cyclonedds", ""),
        "zephyr-dds-cpp-service-server" => ("cpp", "service-server", "cyclonedds", ""),
        "zephyr-dds-cpp-service-client" => ("cpp", "service-client", "cyclonedds", ""),
        "zephyr-dds-cpp-action-server" => ("cpp", "action-server", "cyclonedds", ""),
        "zephyr-dds-cpp-action-client" => ("cpp", "action-client", "cyclonedds", ""),
        "zephyr-dds-cpp-talker-a9" => ("cpp", "talker", "cyclonedds", "-a9"),
        "zephyr-dds-cpp-listener-a9" => ("cpp", "listener", "cyclonedds", "-a9"),
        "zephyr-dds-cpp-service-server-a9" => ("cpp", "service-server", "cyclonedds", "-a9"),
        "zephyr-dds-cpp-service-client-a9" => ("cpp", "service-client", "cyclonedds", "-a9"),
        "zephyr-dds-cpp-action-server-a9" => ("cpp", "action-server", "cyclonedds", "-a9"),
        "zephyr-dds-cpp-action-client-a9" => ("cpp", "action-client", "cyclonedds", "-a9"),
        "zephyr-dds-c-talker" => ("c", "talker", "cyclonedds", ""),
        "zephyr-dds-c-listener" => ("c", "listener", "cyclonedds", ""),
        "zephyr-dds-c-service-server" => ("c", "service-server", "cyclonedds", ""),
        "zephyr-dds-c-service-client" => ("c", "service-client", "cyclonedds", ""),
        "zephyr-dds-c-action-server" => ("c", "action-server", "cyclonedds", ""),
        "zephyr-dds-c-action-client" => ("c", "action-client", "cyclonedds", ""),
        "zephyr-dds-c-talker-a9" => ("c", "talker", "cyclonedds", "-a9"),
        "zephyr-dds-c-listener-a9" => ("c", "listener", "cyclonedds", "-a9"),
        "zephyr-dds-c-service-server-a9" => ("c", "service-server", "cyclonedds", "-a9"),
        "zephyr-dds-c-service-client-a9" => ("c", "service-client", "cyclonedds", "-a9"),
        "zephyr-dds-c-action-server-a9" => ("c", "action-server", "cyclonedds", "-a9"),
        "zephyr-dds-c-action-client-a9" => ("c", "action-client", "cyclonedds", "-a9"),
        // DDS Rust legacy aliases — Phase 169.4 retired the old Rust DDS
        // backend. These
        // map to cyclonedds for now; the build dir + example path
        // resolve correctly only once Phase 169.5's `nros-rmw-cyclonedds-sys`
        // lands. Tests that invoke these aliases without the shim get
        // a clean "example not found" failure.
        "zephyr-dds-rs-talker" | "dds-rs-talker" => ("rust", "talker", "cyclonedds", ""),
        "zephyr-dds-rs-listener" | "dds-rs-listener" => ("rust", "listener", "cyclonedds", ""),
        "zephyr-dds-rs-service-server" | "dds-rs-service-server" => {
            ("rust", "service-server", "cyclonedds", "")
        }
        "zephyr-dds-rs-service-client" | "dds-rs-service-client" => {
            ("rust", "service-client", "cyclonedds", "")
        }
        "zephyr-dds-rs-action-server" | "dds-rs-action-server" => {
            ("rust", "action-server", "cyclonedds", "")
        }
        "zephyr-dds-rs-action-client" | "dds-rs-action-client" => {
            ("rust", "action-client", "cyclonedds", "")
        }
        // `service-client-async` zephyr/rust example dropped 2026-06-02 per
        // Phase 212.M-F.5 — pending async-`Node` trait decision.
        "zephyr-dds-rs-talker-a9" => ("rust", "talker", "cyclonedds", "-a9"),
        "zephyr-dds-rs-listener-a9" => ("rust", "listener", "cyclonedds", "-a9"),
        "zephyr-dds-rs-service-server-a9" => ("rust", "service-server", "cyclonedds", "-a9"),
        "zephyr-dds-rs-service-client-a9" => ("rust", "service-client", "cyclonedds", "-a9"),
        "zephyr-dds-rs-action-server-a9" => ("rust", "action-server", "cyclonedds", "-a9"),
        "zephyr-dds-rs-action-client-a9" => ("rust", "action-client", "cyclonedds", "-a9"),
        _ => return None,
    })
}

/// Build-dir slot for the alias. Collapsed shape:
/// `build-<lang_tag>-<case>-<rmw>[-a9]` where `lang_tag` is
/// `rs` / `c` / `cpp`.
fn build_dir_for_example(example_name: &str) -> String {
    if let Some((lang, case, rmw, suffix)) = decode_alias(example_name) {
        let lang_tag = match lang {
            "rust" => "rs",
            other => other,
        };
        format!("build-{lang_tag}-{case}-{rmw}{suffix}")
    } else {
        "build".to_string()
    }
}

/// Convert example name to the actual path under examples/
///
/// Handles both legacy names (zephyr-rs-talker) and new names (rs-talker).
/// Returns path relative to examples/ directory.
fn example_path_for_name(example_name: &str) -> String {
    if let Some((lang, case, _rmw, _suffix)) = decode_alias(example_name) {
        return format!("zephyr/{lang}/{case}");
    }
    example_name.to_string()
}

/// Phase 168.6.B — `-DCONF_FILE="..."` argument value for a
/// collapsed alias. Returns `None` for non-collapsed names so
/// callers leave the west default (single `prj.conf`) alone.
fn conf_files_for_example(example_name: &str) -> Option<String> {
    decode_alias(example_name)
        .map(|(_lang, _case, rmw, _suffix)| format!("prj.conf;prj-{rmw}.conf"))
}

/// Get path to a prebuilt Zephyr fixture binary.
///
/// This function checks if a Zephyr binary already exists in the build directory
/// and returns it when it is fresh. Tests must not build fixtures in their
/// bodies; stale or missing fixtures report a setup error that points to
/// `just zephyr build-fixtures`.
///
/// # Arguments
/// * `example_name` - Name of the example directory (e.g., "zephyr-rs-talker")
/// * `platform` - Target platform
/// # Returns
/// Path to the binary
pub fn get_prebuilt_zephyr_example(
    example_name: &str,
    platform: ZephyrPlatform,
) -> TestResult<PathBuf> {
    let workspace = zephyr_workspace_path()
        .ok_or_else(|| TestError::BuildFailed("Zephyr workspace not found".to_string()))?;

    let build_root = zephyr_build_root(&workspace);
    let build_dir = build_dir_for_example(example_name);

    // Determine binary path based on platform
    let binary_path = match platform {
        ZephyrPlatform::NativeSim => build_root.join(format!("{}/zephyr/zephyr.exe", build_dir)),
        ZephyrPlatform::QemuArm | ZephyrPlatform::QemuCortexA9 => {
            build_root.join(format!("{}/zephyr/zephyr.elf", build_dir))
        }
    };

    let binary = crate::fixtures::require_prebuilt_binary(&binary_path)?;
    // Honor the same bypass the sibling fixture guards do (native/cmake/rust
    // paths all check this) — an mtime-heuristic false-positive (#147) shouldn't
    // block a run the caller knows was built another way. Previously this guard
    // omitted the check, so a content-current image with a newer-mtime source
    // (e.g. an inert edit) wrongly aborted.
    if std::env::var_os("NROS_SKIP_FIXTURE_CHECK").is_none()
        && is_binary_stale(&binary, example_name)
    {
        return Err(TestError::BuildFailed(format!(
            "Zephyr fixture binary is stale: {}\n\
             Run `just zephyr build-fixtures` before running Zephyr tests \
             (or set NROS_SKIP_FIXTURE_CHECK=1 if you built it another way).",
            binary.display()
        )));
    }
    eprintln!("Using prebuilt Zephyr binary: {}", binary.display());
    Ok(binary)
}

/// Build directory the 225.P workspace-Entry leaf is emitted into by
/// `scripts/build/zephyr-fixture-leaves.sh` (Approach A — a single
/// post-matrix leaf, not a role/RMW-decoded alias). The Zephyr Entry
/// (`examples/workspaces/rust/src/zephyr_entry`) defaults to the zenoh
/// RMW (`prj.conf;prj-zenoh.conf`), so the native_sim ELF lands at
/// `<build_root>/build-ws-rs-entry-zenoh/zephyr/zephyr.exe`.
const ZEPHYR_WORKSPACE_ENTRY_BUILD_DIR: &str = "build-ws-rs-entry-zenoh";

/// Source-tree key handed to [`is_binary_stale`] for the workspace Entry.
/// It is not a `decode_alias` name (the Entry has no role/RMW alias), so
/// the decoder falls through to `None` — which makes staleness watch the
/// whole `examples/workspaces/rust` tree plus every shared core/rmw crate
/// (never under-watches).
const ZEPHYR_WORKSPACE_ENTRY_SRC_KEY: &str = "workspaces/rust";

/// Get path to the prebuilt Zephyr **workspace Entry** binary (Phase 225.P).
///
/// The workspace Entry is a single Zephyr application that hosts the whole
/// launch-defined node set — talker *and* listener — in one process
/// (`nros::main!(launch = "demo_bringup:system.launch.xml")`). It is the
/// Zephyr sibling of the native / FreeRTOS / ThreadX workspace Entries.
///
/// Mirrors [`get_prebuilt_zephyr_example`] but resolves the fixed
/// workspace-Entry build directory directly — there is no role/RMW alias to
/// decode. Tests must not build fixtures in their bodies; a missing or stale
/// binary surfaces a setup error pointing at `just zephyr build-fixtures`.
///
/// Only `native_sim` is resolved (the E2E lane runs the host build); other
/// boards flash the same Entry source but are out of scope for the host test
/// harness.
///
/// # Returns
/// Path to `build-ws-rs-entry-zenoh/zephyr/zephyr.exe`.
pub fn get_prebuilt_zephyr_workspace_entry() -> TestResult<PathBuf> {
    let workspace = zephyr_workspace_path()
        .ok_or_else(|| TestError::BuildFailed("Zephyr workspace not found".to_string()))?;

    let build_root = zephyr_build_root(&workspace);
    let binary_path = build_root.join(format!(
        "{ZEPHYR_WORKSPACE_ENTRY_BUILD_DIR}/zephyr/zephyr.exe"
    ));

    let binary = crate::fixtures::require_prebuilt_binary(&binary_path).map_err(|_| {
        TestError::BuildFailed(format!(
            "Zephyr workspace Entry binary not found: {}\n\
             Build the workspace fixtures first: `just zephyr build-fixtures`.",
            binary_path.display()
        ))
    })?;
    if is_binary_stale(&binary, ZEPHYR_WORKSPACE_ENTRY_SRC_KEY) {
        return Err(TestError::BuildFailed(format!(
            "Zephyr workspace Entry binary is stale: {}\n\
             Run `just zephyr build-fixtures` before running Zephyr tests.",
            binary.display()
        )));
    }
    eprintln!(
        "Using prebuilt Zephyr workspace Entry binary: {}",
        binary.display()
    );
    Ok(binary)
}

/// Return true if the built binary is older than the example or shared nros
/// sources that are linked into Zephyr fixtures.
/// Whether a `packages/core/<crate>` subdir should be watched for staleness
/// of a fixture whose language API crate is `lang_api_crate` (`Some("nros-c")`
/// for C, `Some("nros-cpp")` for C++, `Some("nros")` for Rust, `None` if the
/// language is unknown). Drops only the *other* languages' API crates; every
/// shared/platform/rmw crate stays watched (Phase 177.8). Unknown language →
/// watch everything (never under-watch).
fn core_crate_is_watched(crate_name: &str, lang_api_crate: Option<&str>) -> bool {
    let is_lang_api = matches!(crate_name, "nros" | "nros-c" | "nros-cpp");
    let is_other_lang_api = is_lang_api && lang_api_crate.is_some_and(|c| c != crate_name);
    !is_other_lang_api
}

fn is_binary_stale(binary_path: &Path, example_name: &str) -> bool {
    let Ok(binary_mtime) = binary_path.metadata().and_then(|m| m.modified()) else {
        // Can't stat the binary — assume stale so we rebuild and get a
        // real error instead of reusing something mysterious.
        return true;
    };

    let root = project_root();
    let example_dir = root
        .join("examples")
        .join(example_path_for_name(example_name));

    // The example-local set catches app source, Kconfig overlays, and Rust
    // dependency changes. The package set catches shared nros backend/platform
    // edits; otherwise tests can report stale Zephyr runtime failures after a
    // library fix has already landed.
    let mut candidates = vec![
        example_dir.join("prj.conf"),
        example_dir.join("CMakeLists.txt"),
        example_dir.join("Cargo.toml"),
        example_dir.join("Cargo.lock"),
        example_dir.join("boards"),
        example_dir.join("src"),
        root.join("zephyr"),
    ];

    // Watch `packages/core`, but skip the *other* languages' API crates so a
    // single-language core edit (e.g. an `nros-cpp` change) doesn't falsely
    // mark unrelated C/Rust fixtures stale — cmake correctly leaves them
    // un-rebuilt (a C fixture links `nros-c`, not `nros-cpp`), yet the gate
    // would report a spurious runtime "is stale" failure (Phase 177.8). Every
    // shared/platform/rmw crate (nros-core, nros-node, nros-rmw, nros-serdes,
    // nros-platform-*, …) stays watched, and new crates are picked up
    // automatically; only the two non-matching language API crates are
    // dropped. `nros` = Rust API, `nros-c` = C API, `nros-cpp` = C++ API.
    let lang_api_crate = match decode_alias(example_name).map(|(lang, _, _, _)| lang) {
        Some("c") => Some("nros-c"),
        Some("cpp") => Some("nros-cpp"),
        Some("rust") => Some("nros"),
        _ => None,
    };
    let core_dir = root.join("packages/core");
    match std::fs::read_dir(&core_dir) {
        Ok(entries) => {
            for entry in entries.flatten() {
                let name = entry.file_name();
                if core_crate_is_watched(&name.to_string_lossy(), lang_api_crate) {
                    candidates.push(entry.path());
                }
            }
        }
        // Can't enumerate — fall back to watching the whole tree (safe: never
        // under-watches, at worst keeps the old over-broad behaviour).
        Err(_) => candidates.push(core_dir),
    }
    match decode_alias(example_name).map(|(_, _, rmw, _)| rmw) {
        Some("cyclonedds") => candidates.push(root.join("packages/dds")),
        Some("xrce") => candidates.push(root.join("packages/xrce")),
        Some("zenoh") => candidates.push(root.join("packages/zpico")),
        _ => {
            candidates.push(root.join("packages/dds"));
            candidates.push(root.join("packages/xrce"));
            candidates.push(root.join("packages/zpico"));
        }
    }
    if let Some(conf_files) = conf_files_for_example(example_name) {
        for conf_file in conf_files.split(';') {
            candidates.push(example_dir.join(conf_file));
        }
    }
    for p in &candidates {
        if path_newer_than(p, binary_mtime) {
            return true;
        }
    }
    false
}

fn path_newer_than(path: &Path, cutoff: std::time::SystemTime) -> bool {
    let Ok(meta) = path.metadata() else {
        return false;
    };
    if !meta.is_dir() {
        // Files: a newer mtime is a real content change.
        return meta.modified().is_ok_and(|mtime| mtime > cutoff);
    }
    // Directories: do NOT trust the directory's OWN mtime — it bumps on any
    // transient entry add/remove (e.g. a codegen step writing then deleting a
    // temp file inside a watched `include/` dir), which is not a source change
    // and would falsely mark EVERY fixture stale right after a clean rebuild.
    // (Observed: `_test-c-codegen` churns `packages/core/nros-{c,cpp}/include/
    // nros/`, bumping the dir mtime while every header inside stays unchanged
    // — git-clean — yet the old `meta.modified() > cutoff` early-return tripped
    // here and reported all zephyr fixtures stale.) Recurse into entries
    // instead; only real file mtimes count. Pure deletions are not detected by
    // mtime anyway — the build-side content `.nros-zephyr-fixture.sig` is the
    // safety net for those.
    let Ok(entries) = std::fs::read_dir(path) else {
        return false;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(name.as_ref(), "target" | "build" | ".git") {
            continue;
        }
        if path_newer_than(&p, cutoff) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_crate_is_watched_per_language() {
        // Shared crates: always watched, regardless of fixture language.
        for lang in [Some("nros-c"), Some("nros-cpp"), Some("nros"), None] {
            for shared in ["nros-core", "nros-node", "nros-rmw", "nros-platform-zephyr"] {
                assert!(
                    core_crate_is_watched(shared, lang),
                    "shared crate {shared} must stay watched for lang {lang:?}"
                );
            }
        }
        // C fixture: watches nros-c, drops the other two language API crates.
        assert!(core_crate_is_watched("nros-c", Some("nros-c")));
        assert!(!core_crate_is_watched("nros-cpp", Some("nros-c")));
        assert!(!core_crate_is_watched("nros", Some("nros-c")));
        // C++ fixture.
        assert!(core_crate_is_watched("nros-cpp", Some("nros-cpp")));
        assert!(!core_crate_is_watched("nros-c", Some("nros-cpp")));
        assert!(!core_crate_is_watched("nros", Some("nros-cpp")));
        // Rust fixture.
        assert!(core_crate_is_watched("nros", Some("nros")));
        assert!(!core_crate_is_watched("nros-c", Some("nros")));
        assert!(!core_crate_is_watched("nros-cpp", Some("nros")));
        // Unknown language: never under-watch — all language crates kept.
        assert!(core_crate_is_watched("nros-c", None));
        assert!(core_crate_is_watched("nros-cpp", None));
        assert!(core_crate_is_watched("nros", None));
    }

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
