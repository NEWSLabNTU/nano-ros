//! ESP32-C3 QEMU helpers for integration tests
//!
//! Provides guard functions and process management for ESP32-C3 QEMU tests.
//! ESP32-C3 uses RISC-V (qemu-system-riscv32) with the Espressif machine model.

use crate::process::{ManagedProcess, set_new_process_group};
use crate::{TestError, TestResult};
use std::path::Path;
use std::process::{Command, Stdio};

// =============================================================================
// Guard Functions
// =============================================================================

/// Check if qemu-system-riscv32 (Espressif fork) is available
pub fn is_qemu_riscv32_available() -> bool {
    Command::new("qemu-system-riscv32")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Skip test if qemu-system-riscv32 is not available
pub fn require_qemu_riscv32() -> bool {
    if !is_qemu_riscv32_available() {
        eprintln!("Skipping test: qemu-system-riscv32 not found");
        eprintln!("Install Espressif's QEMU fork: ./scripts/esp32/install-espressif-qemu.sh");
        return false;
    }
    true
}

/// Check if the riscv32imc-unknown-none-elf target is installed
pub fn is_riscv32_target_available() -> bool {
    Command::new("rustup")
        .args(["target", "list", "--installed"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("riscv32imc-unknown-none-elf"))
        .unwrap_or(false)
}

/// Skip test if riscv32imc target is not available
pub fn require_riscv32_target() -> bool {
    if !is_riscv32_target_available() {
        eprintln!("Skipping test: riscv32imc-unknown-none-elf target not installed");
        eprintln!("Install with: rustup target add riscv32imc-unknown-none-elf");
        return false;
    }
    true
}

/// Check if zenoh-pico RISC-V library is available
pub fn is_zenoh_pico_riscv_available() -> bool {
    let lib_path = crate::project_root().join("build/esp32-zenoh-pico/libzenohpico.a");
    lib_path.exists()
}

/// Skip test if zenoh-pico RISC-V library is not available
pub fn require_zenoh_pico_riscv() -> bool {
    if !is_zenoh_pico_riscv_available() {
        eprintln!("Skipping test: libzenohpico.a (RISC-V) not found");
        eprintln!("Build with: just build-zenoh-pico-riscv");
        return false;
    }
    true
}

/// Check if espflash is available
pub fn is_espflash_available() -> bool {
    Command::new("espflash")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Skip test if espflash is not available
pub fn require_espflash() -> bool {
    if !is_espflash_available() {
        eprintln!("Skipping test: espflash not found");
        eprintln!("Install with: cargo install espflash");
        return false;
    }
    true
}

/// Check if a TAP interface exists
pub fn is_tap_available(iface: &str) -> bool {
    Command::new("ip")
        .args(["link", "show", iface])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Skip test if TAP networking (tap-qemu0 and tap-qemu1) is not available
pub fn require_tap_network() -> bool {
    if !is_tap_available("tap-qemu0") || !is_tap_available("tap-qemu1") {
        eprintln!("Skipping test: TAP network not available (need tap-qemu0 and tap-qemu1)");
        eprintln!("Set up with: sudo ./scripts/qemu/setup-network.sh");
        return false;
    }
    true
}

// =============================================================================
// Networking Helpers
// =============================================================================

/// Wait until a TCP port is free (no listener).
///
/// Useful to avoid `EADDRINUSE` when reusing a fixed port (e.g. 7447)
/// across sequential tests.
pub fn wait_for_port_free(port: u16, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();
    let addr = format!("127.0.0.1:{}", port);

    while start.elapsed() < timeout {
        if std::net::TcpStream::connect(&addr).is_err() {
            return true; // port is free
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

/// Wait until a specific IP:port is reachable via TCP.
///
/// Unlike `wait_for_port` (which checks 127.0.0.1), this checks an
/// arbitrary address — e.g. the bridge IP `192.0.3.1:7447` that QEMU
/// instances connect to.
pub fn wait_for_addr(addr: &str, timeout: std::time::Duration) -> bool {
    let start = std::time::Instant::now();

    while start.elapsed() < timeout {
        if std::net::TcpStream::connect(addr).is_ok() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    false
}

// =============================================================================
// ESP32-C3 QEMU Helpers
// =============================================================================

/// Create a flash image from an ELF binary using espflash
///
/// ESP32-C3 QEMU requires a merged flash image (bootloader + partition table + app).
///
/// # Arguments
/// * `elf` - Path to the ELF binary
/// * `output` - Path to write the flash image
pub fn create_esp32_flash_image(elf: &Path, output: &Path) -> TestResult<()> {
    if !elf.exists() {
        return Err(TestError::BuildFailed(format!(
            "ELF binary not found: {}",
            elf.display()
        )));
    }

    // Create output directory if needed
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| TestError::BuildFailed(format!("Failed to create output dir: {}", e)))?;
    }

    eprintln!(
        "Creating flash image: {} -> {}",
        elf.display(),
        output.display()
    );

    let result = duct::cmd!(
        "espflash",
        "save-image",
        "--chip",
        "esp32c3",
        "--flash-size",
        "4mb",
        "--merge",
        elf.to_str().unwrap(),
        output.to_str().unwrap()
    )
    .stderr_to_stdout()
    .stdout_capture()
    .unchecked()
    .run()
    .map_err(|e| TestError::BuildFailed(format!("espflash failed: {}", e)))?;

    if !result.status.success() {
        return Err(TestError::BuildFailed(format!(
            "espflash save-image failed:\n{}",
            String::from_utf8_lossy(&result.stdout)
        )));
    }

    if !output.exists() {
        return Err(TestError::BuildFailed(format!(
            "Flash image not created: {}",
            output.display()
        )));
    }

    Ok(())
}

/// Start an ESP32-C3 QEMU instance
///
/// # Arguments
/// * `flash_image` - Path to the flash image (.bin)
/// * `tap` - Optional TAP interface for networking (e.g., "tap-qemu0")
/// * `mac` - Optional MAC address (e.g., "02:00:00:00:00:01")
pub fn start_esp32_qemu(
    flash_image: &Path,
    tap: Option<&str>,
    mac: Option<&str>,
) -> TestResult<ManagedProcess> {
    if !flash_image.exists() {
        return Err(TestError::BuildFailed(format!(
            "Flash image not found: {}",
            flash_image.display()
        )));
    }

    let drive_arg = format!("file={},if=mtd,format=raw", flash_image.display());

    let mut cmd = Command::new("qemu-system-riscv32");
    cmd.args([
        "-M",
        "esp32c3",
        "-icount",
        "3",
        "-nographic",
        "-drive",
        &drive_arg,
    ]);

    if let Some(tap_iface) = tap {
        let mac_addr = mac.unwrap_or("02:00:00:00:00:01");
        let nic_arg = format!(
            "tap,model=open_eth,ifname={},script=no,downscript=no,mac={}",
            tap_iface, mac_addr
        );
        cmd.args(["-nic", &nic_arg]);
    } else {
        cmd.args(["-nic", "none"]);
    }

    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    set_new_process_group(&mut cmd);

    ManagedProcess::spawn_command(cmd, "esp32-qemu")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_riscv32_detection() {
        let available = is_qemu_riscv32_available();
        eprintln!("qemu-system-riscv32 available: {}", available);
    }

    #[test]
    fn test_riscv32_target_detection() {
        let available = is_riscv32_target_available();
        eprintln!(
            "riscv32imc-unknown-none-elf target available: {}",
            available
        );
    }

    #[test]
    fn test_espflash_detection() {
        let available = is_espflash_available();
        eprintln!("espflash available: {}", available);
    }

    #[test]
    fn test_zenoh_pico_riscv_detection() {
        let available = is_zenoh_pico_riscv_available();
        eprintln!("zenoh-pico RISC-V available: {}", available);
    }
}
