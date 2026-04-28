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

    /// Start QEMU with MPS2-AN385 machine + LAN9118 slirp networking
    ///
    /// Uses the MPS2-AN385 machine with a LAN9118 Ethernet controller in QEMU's
    /// user-mode (slirp) networking. Each QEMU instance gets its own fully
    /// isolated NAT stack — no TAP devices, no bridge, no sudo needed.
    ///
    /// The firmware connects to the host via slirp gateway `10.0.2.2`. The
    /// firmware's `config.toml` must use the `10.0.2.0/24` subnet with gateway
    /// `10.0.2.2` and a unique guest IP per instance.
    ///
    /// Uses `-icount shift=auto` to synchronize QEMU's virtual clock with
    /// wall-clock time. Without this, hardware timers (CMSDK Timer0) race ahead
    /// during WFI, causing zenoh-pico timeouts to expire before network I/O
    /// completes. See `docs/reference/qemu-icount.md`.
    pub fn start_mps2_an385_networked(binary: &Path) -> TestResult<Self> {
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
            // Synchronize virtual clock with wall-clock time. With sleep=on
            // (default), WFI advances virtual time at wall-clock speed via
            // QEMU_CLOCK_VIRTUAL_RT instead of jumping instantly. This keeps
            // hardware timer-backed clocks (CMSDK Timer0) aligned with slirp
            // network I/O timing.
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .args(["-nic", "user,model=lan9118"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Start QEMU with MPS2-AN385 machine + LAN9118 mcast-socket
    /// networking (Phase 97.4.freertos).
    ///
    /// Two QEMU instances launched with the same `mcast_addr:port`
    /// share a virtual L2 broadcast domain on the host (no `sudo`,
    /// no TAP, no bridge needed). RTPS SPDP / SEDP / ARP all flow
    /// between them. Mirrors the Zephyr A9 mcast pattern Phase 92
    /// uses for cross-instance DDS.
    ///
    /// `mac` must be unique per instance so ARP behaves; the FreeRTOS
    /// board crate's `Config::mac` should match what's passed here.
    pub fn start_mps2_an385_mcast(
        binary: &Path,
        mcast_addr_port: &str,
        mac: &str,
    ) -> TestResult<Self> {
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
            "-icount",
            "shift=auto",
            "-semihosting-config",
            "enable=on,target=native",
            "-kernel",
        ])
        .arg(binary)
        .args([
            "-nic",
            &format!("socket,model=lan9118,mcast={mcast_addr_port},mac={mac}"),
        ])
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
                                // E2E completion markers (finite examples)
                                || output.contains("All service calls completed")
                                || output.contains("Action client finished")
                                || output.contains("Action completed successfully")
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

    /// Wait until the output contains the given pattern, then return all output collected so far.
    /// Kills the process on timeout.
    pub fn wait_for_output_pattern(
        &mut self,
        pattern: &str,
        timeout: Duration,
    ) -> TestResult<String> {
        let start = Instant::now();
        let mut output = String::new();

        let mut stdout = self
            .handle
            .stdout
            .take()
            .ok_or_else(|| TestError::ProcessFailed("No stdout".to_string()))?;

        #[cfg(unix)]
        {
            let fd = stdout.as_raw_fd();
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }

        let mut buffer = [0u8; 4096];
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
                    let _ = stdout.read_to_string(&mut output);
                    break;
                }
                Ok(None) => match stdout.read(&mut buffer) {
                    Ok(0) => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Ok(n) => {
                        output.push_str(&String::from_utf8_lossy(&buffer[..n]));
                        if output.contains(pattern) {
                            // Put stdout back on the handle so subsequent
                            // wait_for_output / kill calls still see it.
                            // (Killing here would break two-phase tests
                            // that wait for a "ready" pattern then expect
                            // the process to keep running.)
                            self.handle.stdout = Some(stdout);
                            return Ok(output);
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(_) => break,
                },
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

    /// Start QEMU with ARM virt machine (Cortex-A7 + virtio-net + slirp networking)
    ///
    /// Used for NuttX QEMU tests. The virt machine provides a virtio-net interface
    /// using QEMU's user-mode (slirp) networking. Each instance gets its own
    /// isolated NAT stack — no TAP devices, no bridge, no sudo needed.
    ///
    /// # Arguments
    /// * `binary` - Path to the NuttX ELF binary (kernel + app)
    /// * `networking` - `true` for slirp networking, `false` for no NIC (boot tests)
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_nuttx_virt(binary: &Path, networking: bool) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = Command::new("qemu-system-arm");
        cmd.args([
            "-M",
            "virt",
            "-cpu",
            "cortex-a7",
            "-nographic",
            // Sync virtual clock with wall-clock so sleep()/timeouts run
            // at real-time rates (matches the interactive `just nuttx
            // talker` recipe in just/nuttx.just).
            "-icount",
            "shift=auto",
            "-kernel",
        ])
        .arg(binary);

        if networking {
            // NuttX's defconfig enables CONFIG_DRIVERS_VIRTIO_MMIO, so the
            // NIC must be attached to the virtio-mmio transport — not
            // virtio-net-pci, which is what `-nic user` defaults to on the
            // ARM virt machine. Use explicit `-netdev user` + `-device
            // virtio-net-device` to force the MMIO path so NuttX's driver
            // actually probes and configures the interface.
            cmd.args([
                "-netdev",
                "user,id=net0",
                "-device",
                "virtio-net-device,netdev=net0",
            ]);
        } else {
            cmd.args(["-nic", "none"]);
        }

        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        #[cfg(unix)]
        set_new_process_group(&mut cmd);
        let handle = cmd.spawn()?;

        Ok(Self { handle })
    }

    /// Start QEMU with RISC-V 64-bit virt machine + virtio-net slirp networking
    ///
    /// Used for ThreadX QEMU RISC-V tests. The virt machine provides a virtio-net
    /// MMIO interface using QEMU's user-mode (slirp) networking. Each instance
    /// gets its own isolated NAT stack — no TAP devices, no bridge, no sudo needed.
    ///
    /// The `peer_index` selects the MAC address:
    /// - 0: MAC 52:54:00:12:34:56 (talker/server)
    /// - 1: MAC 52:54:00:12:34:57 (listener/client)
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

        cmd.args(["-netdev", "user,id=net0"]);
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
