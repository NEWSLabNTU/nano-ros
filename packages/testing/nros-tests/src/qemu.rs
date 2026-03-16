//! QEMU process fixture for embedded testing
//!
//! Provides managed QEMU processes for testing ARM Cortex-M binaries.

use crate::process::{kill_process_group, set_new_process_group};
use crate::{TestError, TestResult};
use std::io::Read;
#[cfg(unix)]
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// Managed QEMU process for Cortex-M3 emulation
///
/// Starts QEMU with semihosting enabled and captures output.
/// Automatically kills the process on drop.
///
/// # Example
///
/// ```ignore
/// use nros_tests::fixtures::QemuProcess;
/// use std::path::Path;
///
/// let binary = Path::new("target/thumbv7m-none-eabi/release/my-test");
/// let mut qemu = QemuProcess::start_cortex_m3(binary).unwrap();
/// let output = qemu.wait_for_output(Duration::from_secs(30)).unwrap();
/// assert!(output.contains("[PASS]"));
/// ```
pub struct QemuProcess {
    handle: Child,
}

impl QemuProcess {
    /// Start QEMU Cortex-M3 emulator with semihosting
    ///
    /// Uses the LM3S6965EVB machine which supports semihosting output.
    ///
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_cortex_m3(binary: &Path) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = Command::new("qemu-system-arm");
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "lm3s6965evb",
            "-nographic",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Start QEMU with MPS2-AN385 machine (Cortex-M3 + LAN9118 Ethernet)
    ///
    /// Uses the MPS2-AN385 machine which has a LAN9118 Ethernet controller.
    ///
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_mps2_an385(binary: &Path) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = Command::new("qemu-system-arm");
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-nographic",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Start QEMU with MPS2-AN385 machine + LAN9118 TAP networking
    ///
    /// Uses the MPS2-AN385 machine with a LAN9118 Ethernet controller connected
    /// to a TAP device on the host, enabling network communication via the qemu-br bridge.
    ///
    /// The `peer_index` selects the TAP device and MAC address:
    /// - 0: tap-qemu0, MAC 02:00:00:00:00:00 (talker/server)
    /// - 1: tap-qemu1, MAC 02:00:00:00:00:01 (listener/client)
    ///
    /// MAC addresses use the locally-administered range (02:xx) with the last
    /// byte derived from the peer index. These must match the firmware's network
    /// config in `nros-mps2-an385-freertos`.
    ///
    /// Uses `-icount shift=auto` to synchronize QEMU's virtual clock with
    /// wall-clock time. Without this, hardware timers (CMSDK Timer0) race ahead
    /// during WFI, causing zenoh-pico timeouts to expire before TAP network I/O
    /// completes. See `docs/reference/qemu-icount.md`.
    pub fn start_mps2_an385_networked(binary: &Path, peer_index: u8) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let tap_device = format!("tap-qemu{}", peer_index);
        let mac_address = format!("02:00:00:00:00:{:02x}", peer_index);
        let nic_arg = format!(
            "tap,ifname={},script=no,downscript=no,model=lan9118,mac={}",
            tap_device, mac_address
        );

        let mut cmd = Command::new("qemu-system-arm");
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-nographic",
            // Synchronize virtual clock with wall-clock time. With sleep=on
            // (default), WFI advances virtual time at wall-clock speed via
            // QEMU_CLOCK_VIRTUAL_RT instead of jumping instantly. This keeps
            // hardware timer-backed clocks (CMSDK Timer0) aligned with TAP
            // network I/O timing.
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .args(["-nic", &nic_arg])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Start QEMU with MPS2-AN385 machine + external serial device
    ///
    /// Connects UART0 to the given serial device path (e.g., a socat PTY).
    /// Uses `-display none -monitor none` to avoid `-nographic`'s implicit
    /// `-serial mon:stdio` which would hijack UART0 for the monitor.
    /// Semihosting output goes to stdout via the ARM semihosting interface.
    ///
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    /// * `serial_path` - Host serial device path (e.g., `/tmp/serial-a`)
    pub fn start_mps2_an385_with_serial(binary: &Path, serial_path: &str) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let chardev_arg = format!("serial,id=ser0,path={}", serial_path);
        let mut cmd = Command::new("qemu-system-arm");
        cmd.args([
            "-cpu",
            "cortex-m3",
            "-machine",
            "mps2-an385",
            "-display",
            "none",
            "-monitor",
            "none",
            // Synchronize virtual clock with wall-clock time. The firmware
            // uses CMSDK Timer0 (hardware timer) for zenoh-pico timeouts.
            // Without icount, WFI advances virtual time instantly, causing
            // timeouts to fire before serial I/O through zenohd completes.
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-chardev",
        ])
        .arg(&chardev_arg)
        .args(["-serial", "chardev:ser0", "-kernel"])
        .arg(binary)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Wait for QEMU to produce output and exit
    ///
    /// # Arguments
    /// * `timeout` - Maximum time to wait
    ///
    /// # Returns
    /// The combined stdout/stderr output
    pub fn wait_for_output(&mut self, timeout: Duration) -> TestResult<String> {
        let start = Instant::now();
        let mut output = String::new();

        // Take ownership of stdout
        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed("No stdout".to_string()))?;

        // Set non-blocking mode so read() doesn't block indefinitely when the
        // QEMU process pauses output (e.g., listener waiting for messages).
        // Without this, the timeout loop never fires because read() blocks.
        #[cfg(unix)]
        {
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        // Read with timeout
        let mut buffer = [0u8; 4096];
        loop {
            if start.elapsed() > timeout {
                // Kill on timeout
                kill_process_group(&mut self.handle);
                if output.is_empty() {
                    return Err(TestError::Timeout);
                }
                break;
            }

            // Check if process exited
            match self.handle.try_wait() {
                Ok(Some(_status)) => {
                    // Process exited, read remaining output
                    let _ = stdout.read_to_string(&mut output);
                    break;
                }
                Ok(None) => {
                    // Still running, try to read
                    match stdout.read(&mut buffer) {
                        Ok(0) => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Ok(n) => {
                            output.push_str(&String::from_utf8_lossy(&buffer[..n]));

                            // Check for test completion markers
                            if output.contains("All tests passed")
                                || output.contains("Benchmark complete")
                                || output.contains("TEST COMPLETE")
                                || output.contains("QEMU: Terminated")
                                // FreeRTOS/NuttX E2E completion markers
                                || output.contains("Done publishing")
                                || output.contains("Received 10 messages")
                                || output.contains("All service calls completed")
                                || output.contains("Action completed successfully")
                                || output.contains("Server shutting down")
                                || output.contains("Timeout waiting for")
                            {
                                // Give it a moment to finish cleanly
                                std::thread::sleep(Duration::from_millis(100));
                                kill_process_group(&mut self.handle);
                                break;
                            }
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                            std::thread::sleep(Duration::from_millis(50));
                        }
                        Err(_) => break,
                    }
                }
                Err(_) => break,
            }
        }

        Ok(output)
    }

    /// Kill the QEMU process
    pub fn kill(&mut self) {
        kill_process_group(&mut self.handle);
    }

    /// Check if QEMU is still running
    pub fn is_running(&mut self) -> bool {
        matches!(self.handle.try_wait(), Ok(None))
    }

    /// Start QEMU with ARM virt machine (Cortex-A7 + virtio-net + TAP networking)
    ///
    /// Used for NuttX QEMU tests. The virt machine provides a virtio-net interface
    /// connected to a TAP device on the host, enabling network communication via
    /// the qemu-br bridge.
    ///
    /// # Arguments
    /// * `binary` - Path to the NuttX ELF binary (kernel + app)
    /// * `tap_iface` - TAP interface name (e.g., "tap-qemu0")
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_nuttx_virt(binary: &Path, tap_iface: &str) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = Command::new("qemu-system-arm");
        cmd.args(["-M", "virt", "-cpu", "cortex-a7", "-nographic", "-kernel"])
            .arg(binary);

        // "none" means no networking (boot test); otherwise use TAP
        if tap_iface == "none" {
            cmd.args(["-nic", "none"]);
        } else {
            let nic_arg = format!("tap,ifname={},script=no,downscript=no", tap_iface);
            cmd.args(["-nic", &nic_arg]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Start QEMU with RISC-V 64-bit virt machine + virtio-net TAP networking
    ///
    /// Used for ThreadX QEMU RISC-V tests. The virt machine provides a virtio-net
    /// MMIO interface connected to a TAP device on the host, enabling network
    /// communication via the qemu-br bridge.
    ///
    /// The `peer_index` selects the TAP device and MAC address:
    /// - 0: tap-qemu0, MAC 52:54:00:12:34:56 (talker/server)
    /// - 1: tap-qemu1, MAC 52:54:00:12:34:57 (listener/client)
    ///
    /// MAC addresses use the QEMU OUI range (52:54:00) with the last byte
    /// derived from the peer index (0x56 + index). These must match the
    /// firmware's `Config::default()` / `Config::listener()` presets in
    /// `nros-threadx-qemu-riscv64`.
    pub fn start_riscv64_virt(binary: &Path, peer_index: u8) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let tap_iface = format!("tap-qemu{}", peer_index);
        let mac = format!("52:54:00:12:34:{:02x}", 0x56u8 + peer_index);

        let mut cmd = Command::new("qemu-system-riscv64");
        cmd.args([
            "-M",
            "virt",
            "-m",
            "256M",
            "-bios",
            "none",
            "-nographic",
            "-global",
            "virtio-mmio.force-legacy=false",
            "-kernel",
        ])
        .arg(binary);

        let netdev_arg = format!("tap,id=net0,ifname={},script=no,downscript=no", tap_iface);
        cmd.args(["-netdev", &netdev_arg]);
        let device_arg = format!(
            "virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac={}",
            mac
        );
        cmd.args(["-device", &device_arg]);

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }
}

impl Drop for QemuProcess {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
    }
}

/// Parse test results from QEMU semihosting output
///
/// Looks for `[PASS]` and `[FAIL]` markers.
///
/// # Returns
/// (passed_count, failed_count)
pub fn parse_test_results(output: &str) -> (usize, usize) {
    let passed = output.matches("[PASS]").count();
    let failed = output.matches("[FAIL]").count();
    (passed, failed)
}

/// Clean up stale network state from previous QEMU test runs.
///
/// Two sources of stale state cause TCP connection failures when QEMU
/// instances are restarted between E2E tests:
///
/// 1. **Stale TCP sockets** — SIGKILL'd QEMU leaves host-side connections in
///    FIN-WAIT-1 (FIN can't be ACK'd). These persist for minutes and collide
///    with new connections on the same 4-tuple. Fixed via `ss -K`.
///
/// 2. **Stale ARP cache** — The bridge remembers old MAC→IP mappings. New QEMU
///    instances have the same MAC but the kernel may route to a stale entry.
///    Fixed via `ip neigh del`.
///
/// Stale packets in the TAP `pfifo` qdisc are harmless because the firmware
/// seeds smoltcp's ephemeral port from the host's wall clock via ARM
/// semihosting `SYS_TIME`. Each QEMU run uses a different source port, so
/// stale packets with old port numbers are silently ignored by smoltcp (no
/// matching socket).
///
/// Note: `ss -K` and `ip neigh del` require `CAP_NET_ADMIN`. When running
/// without privileges, these fail silently. Nextest retries (configured in
/// `.config/nextest.toml`) handle residual flakiness.
///
/// Orphaned processes are handled separately by `PR_SET_PDEATHSIG(SIGKILL)`.
pub fn cleanup_tap_network() {
    // Read QEMU peer IPs from example config.toml files (fallback to defaults).
    let root = crate::project_root();
    let peer_ips: Vec<String> = ["talker", "listener"]
        .iter()
        .enumerate()
        .map(|(i, name)| {
            let config = root.join(format!(
                "examples/qemu-arm-baremetal/rust/zenoh/{name}/config.toml"
            ));
            crate::read_config_ip(&config).unwrap_or_else(|| format!("192.0.3.{}", 10 + i))
        })
        .collect();

    // Kill stale TCP connections to QEMU IPs (best-effort, needs CAP_NET_ADMIN)
    for ip in &peer_ips {
        let _ = std::process::Command::new("ss")
            .args(["-K", "dst", ip.as_str()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    // Flush ARP cache for QEMU IPs (best-effort, needs CAP_NET_ADMIN)
    for ip in &peer_ips {
        let _ = std::process::Command::new("ip")
            .args(["neigh", "del", ip.as_str(), "dev", "qemu-br"])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    }

    // Settle time for kernel state cleanup.
    // Back-to-back QEMU E2E tests need enough time for the kernel to
    // process FIN-WAIT-1 retransmits and bridge FDB updates. Without
    // this, zenoh-pico service replies can be lost due to residual
    // bridge/ARP state from the previous test.
    std::thread::sleep(Duration::from_secs(2));
}

/// Check if the QEMU TAP bridge network is available
///
/// Verifies that the `qemu-br` bridge and at least `tap-qemu0` + `tap-qemu1`
/// interfaces exist. These are created by `sudo ./scripts/qemu/setup-network.sh`.
pub fn is_tap_bridge_available() -> bool {
    // Check if qemu-br bridge exists
    let bridge_exists = std::path::Path::new("/sys/class/net/qemu-br").exists();
    let tap0_exists = std::path::Path::new("/sys/class/net/tap-qemu0").exists();
    let tap1_exists = std::path::Path::new("/sys/class/net/tap-qemu1").exists();
    bridge_exists && tap0_exists && tap1_exists
}

/// Skip test if TAP bridge is not available
pub fn require_tap_bridge() -> bool {
    if !is_tap_bridge_available() {
        eprintln!("Skipping test: QEMU TAP bridge not available");
        eprintln!("Setup: sudo ./scripts/qemu/setup-network.sh");
        return false;
    }
    true
}

/// Check if the veth bridge network is available for ThreadX Linux simulation.
///
/// Verifies that the `qemu-br` bridge and at least `veth-tx0` + `veth-tx1`
/// interfaces exist. These are created by `sudo ./scripts/qemu/setup-network.sh`.
///
/// ThreadX Linux uses veth pairs (not TAP devices) because the NetX Duo Linux
/// network driver uses AF_PACKET/SOCK_RAW, which doesn't work correctly on TAP
/// devices with a bridge. veth pairs are purely kernel-side and route traffic
/// through the bridge correctly.
pub fn is_veth_bridge_available() -> bool {
    let bridge_exists = std::path::Path::new("/sys/class/net/qemu-br").exists();
    let veth0_exists = std::path::Path::new("/sys/class/net/veth-tx0").exists();
    let veth1_exists = std::path::Path::new("/sys/class/net/veth-tx1").exists();
    bridge_exists && veth0_exists && veth1_exists
}

/// Skip test if veth bridge is not available for ThreadX Linux simulation
pub fn require_veth_bridge() -> bool {
    if !is_veth_bridge_available() {
        eprintln!("Skipping test: veth bridge not available for ThreadX Linux");
        eprintln!("Setup: sudo ./scripts/qemu/setup-network.sh");
        return false;
    }
    true
}

/// Check if QEMU RISC-V 64-bit is available
pub fn is_qemu_riscv64_available() -> bool {
    Command::new("qemu-system-riscv64")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if QEMU ARM is available
pub fn is_qemu_available() -> bool {
    Command::new("qemu-system-arm")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if ARM toolchain is available
pub fn is_arm_toolchain_available() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("thumbv7m-none-eabi"))
        .unwrap_or(false)
}

/// Check if zenoh-pico ARM library is available
///
/// The BSP examples require the zenoh-pico library to be cross-compiled for
/// ARM Cortex-M3. This library is built with `just build-zenoh-pico-arm`.
pub fn is_zenoh_pico_arm_available() -> bool {
    // Check if the library exists at the expected location relative to project root
    let lib_path = crate::project_root().join("build/qemu-zenoh-pico/libzenohpico.a");
    lib_path.exists()
}

/// Skip test if zenoh-pico ARM library is not available
pub fn require_zenoh_pico_arm() -> bool {
    if !is_zenoh_pico_arm_available() {
        eprintln!("Skipping test: libzenohpico.a not found");
        eprintln!("Build with: just build-zenoh-pico-arm");
        return false;
    }
    true
}

/// A socat-managed virtual serial pair (two linked PTYs).
///
/// Creates a bidirectional PTY pair via `socat`. Data written to one
/// end appears on the other. Both ends are exposed as symlinks for
/// stable paths.
///
/// Use this to wire QEMU's UART0 to zenohd's serial listener without
/// timing races: the PTY pair exists before either side starts, so
/// data is kernel-buffered.
///
/// # Example
///
/// ```ignore
/// let pair = SocatPtyPair::create("/tmp/serial-qemu", "/tmp/serial-zenohd")?;
/// // QEMU connects to /tmp/serial-qemu
/// // zenohd listens on /tmp/serial-zenohd
/// // pair is killed on drop
/// ```
pub struct SocatPtyPair {
    handle: Child,
    /// Path for the QEMU side
    pub qemu_path: String,
    /// Path for the zenohd side
    pub zenohd_path: String,
}

impl SocatPtyPair {
    /// Create a new PTY pair with symlinks at the given paths.
    ///
    /// Blocks until both symlinks exist (up to 5 seconds).
    pub fn create(qemu_link: &str, zenohd_link: &str) -> TestResult<Self> {
        // Remove stale symlinks from previous runs
        let _ = std::fs::remove_file(qemu_link);
        let _ = std::fs::remove_file(zenohd_link);

        let pty_a = format!("PTY,raw,echo=0,link={}", qemu_link);
        let pty_b = format!("PTY,raw,echo=0,link={}", zenohd_link);

        let mut cmd = Command::new("socat");
        cmd.args([&pty_a, &pty_b])
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        // Wait for symlinks to appear
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(5) {
            if Path::new(qemu_link).exists() && Path::new(zenohd_link).exists() {
                // Small delay for socat to finish setting up the PTY
                std::thread::sleep(Duration::from_millis(100));
                return Ok(Self {
                    handle,
                    qemu_path: qemu_link.to_string(),
                    zenohd_path: zenohd_link.to_string(),
                });
            }
            std::thread::sleep(Duration::from_millis(50));
        }

        Err(TestError::ProcessFailed(
            "Timeout waiting for socat PTY symlinks".to_string(),
        ))
    }
}

impl Drop for SocatPtyPair {
    fn drop(&mut self) {
        kill_process_group(&mut self.handle);
        let _ = std::fs::remove_file(&self.qemu_path);
        let _ = std::fs::remove_file(&self.zenohd_path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_results() {
        let output = "[PASS] test1\n[PASS] test2\n[FAIL] test3\n[PASS] test4";
        let (passed, failed) = parse_test_results(output);
        assert_eq!(passed, 3);
        assert_eq!(failed, 1);
    }

    #[test]
    fn test_parse_results_empty() {
        let (passed, failed) = parse_test_results("");
        assert_eq!(passed, 0);
        assert_eq!(failed, 0);
    }
}
