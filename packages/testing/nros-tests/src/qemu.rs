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
    /// # Arguments
    /// * `binary` - Path to the ARM ELF binary to run
    /// * `tap_device` - TAP interface name (e.g., "tap-qemu0")
    /// * `mac_address` - MAC address for the NIC (e.g., "02:00:00:00:00:00")
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_mps2_an385_networked(
        binary: &Path,
        tap_device: &str,
        mac_address: &str,
    ) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

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
    /// # Arguments
    /// * `binary` - Path to the RISC-V ELF binary to run
    /// * `tap_iface` - TAP interface name (e.g., "tap-qemu0")
    ///
    /// # Returns
    /// A managed QEMU process
    pub fn start_riscv64_virt(binary: &Path, tap_iface: &str) -> TestResult<Self> {
        if !binary.exists() {
            return Err(TestError::BuildFailed(format!(
                "Binary not found: {}",
                binary.display()
            )));
        }

        let mut cmd = Command::new("qemu-system-riscv64");
        cmd.args([
            "-M",
            "virt",
            "-m",
            "256M",
            "-bios",
            "none",
            "-nographic",
            "-bios",
            "none",
            "-global",
            "virtio-mmio.force-legacy=false",
            "-kernel",
        ])
        .arg(binary);

        let netdev_arg = format!("tap,id=net0,ifname={},script=no,downscript=no", tap_iface);
        cmd.args(["-netdev", &netdev_arg]);
        cmd.args([
            "-device",
            "virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0",
        ]);

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
