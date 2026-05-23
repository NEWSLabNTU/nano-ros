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
    std::env::var_os("NROS_ZEPHYR_BUILD_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|| workspace.to_path_buf())
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
        "zephyr-dds-rs-async-service-client" | "dds-rs-async-service-client" => {
            ("rust", "service-client-async", "cyclonedds", "")
        }
        "zephyr-dds-rs-talker-a9" => ("rust", "talker", "cyclonedds", "-a9"),
        "zephyr-dds-rs-listener-a9" => ("rust", "listener", "cyclonedds", "-a9"),
        "zephyr-dds-rs-service-server-a9" => ("rust", "service-server", "cyclonedds", "-a9"),
        "zephyr-dds-rs-service-client-a9" => ("rust", "service-client", "cyclonedds", "-a9"),
        "zephyr-dds-rs-action-server-a9" => ("rust", "action-server", "cyclonedds", "-a9"),
        "zephyr-dds-rs-action-client-a9" => ("rust", "action-client", "cyclonedds", "-a9"),
        "zephyr-dds-rs-async-service-client-a9" => {
            ("rust", "service-client-async", "cyclonedds", "-a9")
        }
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

/// Get path to Zephyr binary, using an existing fixture by default
///
/// This function checks if a Zephyr binary already exists in the build directory
/// and returns it when it is fresh. Normal tests must not build fixtures in
/// their bodies; stale or missing fixtures report a setup error that points to
/// `just zephyr build-fixtures`. Passing `force_build=true` keeps the explicit
/// build path available for callers that intentionally rebuild.
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

    let build_root = zephyr_build_root(&workspace);
    let build_dir = build_dir_for_example(example_name);

    // Determine binary path based on platform
    let binary_path = match platform {
        ZephyrPlatform::NativeSim => build_root.join(format!("{}/zephyr/zephyr.exe", build_dir)),
        ZephyrPlatform::QemuArm | ZephyrPlatform::QemuCortexA9 => {
            build_root.join(format!("{}/zephyr/zephyr.elf", build_dir))
        }
    };

    if !force_build {
        let binary = crate::fixtures::require_prebuilt_binary(&binary_path)?;
        if is_binary_stale(&binary, example_name) {
            return Err(TestError::BuildFailed(format!(
                "Zephyr fixture binary is stale: {}\n\
                 Run `just zephyr build-fixtures` before running Zephyr tests.",
                binary.display()
            )));
        }
        eprintln!("Using prebuilt Zephyr binary: {}", binary.display());
        return Ok(binary);
    }

    build_zephyr_example(example_name, platform)
}

/// Return true if the built binary is older than the example or shared nros
/// sources that are linked into Zephyr fixtures.
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
        root.join("packages/core"),
        root.join("packages/dds"),
        root.join("packages/xrce"),
        root.join("packages/zpico"),
    ];
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
    if meta.modified().is_ok_and(|mtime| mtime > cutoff) {
        return true;
    }
    if !meta.is_dir() {
        return false;
    }
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

    let build_root = zephyr_build_root(&workspace);
    let build_dir = build_dir_for_example(example_name);
    let actual_build_dir = build_root.join(&build_dir);

    let binary_path = match platform {
        ZephyrPlatform::NativeSim => actual_build_dir.join("zephyr/zephyr.exe"),
        ZephyrPlatform::QemuArm | ZephyrPlatform::QemuCortexA9 => {
            actual_build_dir.join("zephyr/zephyr.elf")
        }
    };

    // Phase 140 — Zephyr examples consume nano-ros via the Phase 139
    // integration shell (`integrations/zephyr/`) which `add_subdirectory`s
    // the root CMake. No CMAKE_PREFIX_PATH override is needed.
    let mut cmd = Command::new("west");
    cmd.current_dir(&workspace)
        .env(
            "SCCACHE_DISABLE",
            std::env::var("NROS_ZEPHYR_SCCACHE_DISABLE").unwrap_or_else(|_| "1".to_string()),
        )
        .env(
            "CMAKE_BUILD_PARALLEL_LEVEL",
            std::env::var("NROS_ZEPHYR_NINJA_JOBS").unwrap_or_else(|_| "1".to_string()),
        )
        .arg("build")
        .arg("-b")
        .arg(platform.board_spec())
        .arg("-d")
        .arg(&actual_build_dir)
        .arg("-p")
        .arg(std::env::var("NROS_ZEPHYR_PRISTINE").unwrap_or_else(|_| "auto".to_string()))
        .arg(&example_path);

    // Phase 168.6.B — collapsed examples select RMW via a Kconfig
    // overlay (prj-<rmw>.conf). Inject CONF_FILE for any alias that
    // resolves to a collapsed cell; legacy/unmapped names keep their
    // pre-collapse single-prj.conf semantics.
    let mut west_extras: Vec<String> = Vec::new();
    if let Some(conf) = conf_files_for_example(example_name) {
        west_extras.push(format!("-DCONF_FILE={conf}"));
    }
    if let Some(port) = xrce_agent_port_for_example(example_name) {
        west_extras.push(format!("-DCONFIG_NROS_XRCE_AGENT_PORT={port}"));
    }
    if !west_extras.is_empty() {
        cmd.arg("--");
        for arg in &west_extras {
            cmd.arg(arg);
        }
    }

    let output = cmd.output().map_err(|e| {
        TestError::BuildFailed(format!("Failed to start west for {example_name}: {e}"))
    })?;
    if !output.status.success() {
        return Err(TestError::BuildFailed(format!(
            "west build failed for {example_name} ({})\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    crate::fixtures::require_prebuilt_binary(&binary_path)
}

fn xrce_agent_port_for_example(example_name: &str) -> Option<u16> {
    match example_name {
        "zephyr-xrce-rs-talker"
        | "xrce-rs-talker"
        | "zephyr-xrce-rs-listener"
        | "xrce-rs-listener" => Some(2018),
        "zephyr-xrce-rs-service-server"
        | "xrce-rs-service-server"
        | "zephyr-xrce-rs-service-client"
        | "xrce-rs-service-client" => Some(2028),
        "zephyr-xrce-rs-action-server"
        | "xrce-rs-action-server"
        | "zephyr-xrce-rs-action-client"
        | "xrce-rs-action-client" => Some(2038),
        "zephyr-xrce-c-talker" | "xrce-c-talker" | "zephyr-xrce-c-listener" | "xrce-c-listener" => {
            Some(2118)
        }
        "zephyr-xrce-cpp-talker"
        | "xrce-cpp-talker"
        | "zephyr-xrce-cpp-listener"
        | "xrce-cpp-listener" => Some(2218),
        "zephyr-xrce-cpp-service-server"
        | "xrce-cpp-service-server"
        | "zephyr-xrce-cpp-service-client"
        | "xrce-cpp-service-client" => Some(2228),
        "zephyr-xrce-cpp-action-server"
        | "xrce-cpp-action-server"
        | "zephyr-xrce-cpp-action-client"
        | "xrce-cpp-action-client" => Some(2238),
        _ => None,
    }
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
