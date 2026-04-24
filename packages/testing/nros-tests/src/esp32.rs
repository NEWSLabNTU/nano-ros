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

/// Check if qemu-system-riscv32 (Espressif fork) is available.
///
/// The stock Debian/Ubuntu `qemu-system-riscv32` (QEMU ≤ 8.x) is *not*
/// sufficient — it doesn't know about the `esp32c3` machine model and
/// fails at launch with "unsupported machine type". Probe for the
/// model specifically instead of the binary's mere existence.
pub fn is_qemu_riscv32_available() -> bool {
    let output = Command::new("qemu-system-riscv32")
        .args(["-machine", "help"])
        .output();
    match output {
        Ok(o) if o.status.success() => {
            let text = String::from_utf8_lossy(&o.stdout);
            text.contains("esp32c3")
        }
        _ => false,
    }
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

// Removed: `is_zenoh_pico_riscv_available` / `require_zenoh_pico_riscv`
// used to gate ESP32 tests on the standalone
// `build/esp32-zenoh-pico/libzenohpico.a`, produced by
// `scripts/esp32/build-zenoh-pico.sh`. Phase 84.F4 folded that build
// into `zpico-sys/build.rs::build_zenoh_pico_embedded` (via `cc::Build`
// on the `riscv32imc-unknown-none-elf` target) — see the comment at
// `packages/zpico/zpico-sys/build.rs:1387` ("This replaces the external
// scripts/{qemu,esp32}/build-zenoh-pico.sh shell scripts"). The
// standalone artefact is no longer read by any example's cargo build,
// so the precondition was vestigial and only served to skip-panic the
// test when the shell-script path hadn't been run.

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

// =============================================================================
// Networking Helpers
// =============================================================================

/// Wait until a TCP port is free (no listener).
///
/// Useful to avoid `EADDRINUSE` when reusing a fixed port (e.g. 7448)
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
/// arbitrary address — e.g. a host-forwarded endpoint or a veth bridge
/// IP that QEMU instances connect to.
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
/// Uses QEMU's user-mode (slirp) networking when `networking` is true.
/// Each instance gets its own isolated NAT stack.
///
/// # Arguments
/// * `flash_image` - Path to the flash image (.bin)
/// * `networking` - `true` for slirp networking, `false` for no NIC
pub fn start_esp32_qemu(flash_image: &Path, networking: bool) -> TestResult<ManagedProcess> {
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

    if networking {
        cmd.args(["-nic", "user,model=open_eth"]);
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

}
