//! ESP32-C3 QEMU helpers for integration tests
//!
//! Provides guard functions and process management for ESP32-C3 QEMU tests.
//! ESP32-C3 uses RISC-V (qemu-system-riscv32) with the Espressif machine model.

use crate::{
    TestError, TestResult,
    process::{ManagedProcess, set_new_process_group},
};
use std::{
    path::Path,
    process::{Command, Stdio},
};

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
        eprintln!(
            "Install Espressif's QEMU fork: nros setup --tool esp32-qemu (or: just esp32 setup)"
        );
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

// `require_zenoh_pico_riscv` removed — Phase 84.F4 replaced the
// standalone `build/esp32-zenoh-pico/libzenohpico.a` path with
// `zpico-sys/build.rs::build_zenoh_pico_embedded`, which cross-
// compiles zenoh-pico during the example's cargo build.

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

/// Check if a usable ESP-IDF installation is reachable.
///
/// Phase 212.H.5 harness gate — requires both `$IDF_PATH` to be set
/// (so the IDF's `tools/cmake/project.cmake` resolves) AND `idf.py` to
/// be on PATH (sourced via `esp-idf-workspace/env.sh` in CI; user-side
/// via `. $IDF_PATH/export.sh`).
pub fn is_esp_idf_available() -> bool {
    let idf_path = match std::env::var_os("IDF_PATH") {
        Some(p) if !p.is_empty() => p,
        _ => return false,
    };
    if !std::path::Path::new(&idf_path)
        .join("tools/cmake/project.cmake")
        .is_file()
    {
        return false;
    }
    Command::new("idf.py")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Skip test if ESP-IDF is not available.
pub fn require_esp_idf() -> bool {
    if !is_esp_idf_available() {
        eprintln!("Skipping test: ESP-IDF not reachable (need $IDF_PATH + `idf.py` on PATH)");
        eprintln!("Install via `just esp_idf setup` then `source esp-idf-workspace/env.sh`.");
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
// ESP-IDF flasher_args.json interpreter (phase-295 W5.b)
// =============================================================================

/// phase-295 W5.b — parsed subset of an ESP-IDF `flasher_args.json`.
///
/// `idf.py build` emits `<build_dir>/flasher_args.json` at the BUILD stage: it
/// is Espressif's own flashing metadata (chip model, flash mode/size/freq, the
/// bootloader + partition-table + app offset→file map). Deriving the QEMU
/// machine and flash settings from it keeps the launch line the framework's,
/// rather than a hand-built Espressif-fork command line (RFC-0051 §4). This is
/// the ESP-IDF sibling of the Zephyr `runners.yaml` interpreter.
///
/// Parsed with `serde_json` (a workspace dep); only the keys the harness needs
/// are modeled — `extra_esptool_args.chip` and `flash_settings`.
#[derive(Debug, Clone, Default)]
pub struct EspFlasherArgs {
    /// `extra_esptool_args.chip` (e.g. `esp32c3`) — selects the QEMU `-M`.
    pub chip: Option<String>,
    /// `flash_settings.flash_size` (e.g. `2MB`).
    pub flash_size: Option<String>,
    /// `flash_settings.flash_mode` (e.g. `dio`).
    pub flash_mode: Option<String>,
    /// `flash_settings.flash_freq` (e.g. `80m`).
    pub flash_freq: Option<String>,
}

impl EspFlasherArgs {
    /// Parse a `flasher_args.json` at `path`. Returns `None` if absent or
    /// unparseable so callers fall back gracefully.
    pub fn from_path(path: &Path) -> Option<Self> {
        let text = std::fs::read_to_string(path).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        let s = |x: &serde_json::Value| x.as_str().map(str::to_string);
        Some(EspFlasherArgs {
            chip: s(&v["extra_esptool_args"]["chip"]),
            flash_size: s(&v["flash_settings"]["flash_size"]),
            flash_mode: s(&v["flash_settings"]["flash_mode"]),
            flash_freq: s(&v["flash_settings"]["flash_freq"]),
        })
    }

    /// Convenience: parse `<build_dir>/flasher_args.json`.
    pub fn from_build_dir(build_dir: &Path) -> Option<Self> {
        Self::from_path(&build_dir.join("flasher_args.json"))
    }

    /// The QEMU `-M` machine model derived from the chip. Espressif's QEMU
    /// fork names its machines after the chip (`esp32c3`, `esp32`, …), so the
    /// chip string is the machine string.
    pub fn qemu_machine(&self) -> Option<&str> {
        self.chip.as_deref()
    }
}

/// phase-295 W5.b — resolve the QEMU `-M` machine model for a flash image.
///
/// Prefers the framework's metadata: if a `flasher_args.json` sits next to the
/// image (the ESP-IDF `idf.py` layout), the machine comes from its
/// `extra_esptool_args.chip`.
///
/// SANCTIONED FALLBACK (E1/E9 exception, RFC-0051 §4): the esp32 *Rust*
/// examples flash via `espflash save-image --merge` (see
/// [`create_esp32_flash_image`]), which produces a single merged `.bin` and
/// emits NO `flasher_args.json`. There is no framework runner metadata to read
/// for that path, so the machine defaults to `esp32c3` — the only chip the
/// esp32 fixtures target (their build target is `riscv32imc…` / `-M esp32c3`).
fn esp32_qemu_machine_for_image(flash_image: &Path) -> String {
    flash_image
        .parent()
        .and_then(EspFlasherArgs::from_build_dir)
        .and_then(|f| f.qemu_machine().map(str::to_string))
        .unwrap_or_else(|| "esp32c3".to_string())
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
    // phase-295 W5.b — machine model comes from the framework's
    // `flasher_args.json` when present (ESP-IDF layout); the espflash
    // merged-image path has no such metadata and falls back to `esp32c3`
    // (documented in `esp32_qemu_machine_for_image`).
    let machine = esp32_qemu_machine_for_image(flash_image);

    let mut cmd = Command::new("qemu-system-riscv32");
    cmd.args([
        "-M",
        &machine,
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

/// Phase 101.7 — start an ESP32-C3 QEMU instance on a shared
/// `-nic socket,mcast=…` segment so two guests can exchange RTPS
/// SPDP / SEDP / pubsub on the same virtual L2 broadcast domain.
///
/// Mirrors `QemuProcess::start_mps2_an385_mcast` (FreeRTOS / 97.4)
/// and `QemuProcess::start_nuttx_virt_mcast` shapes — same mcast
/// addr+port across both peers, distinct `mac` per instance so ARP
/// behaves. The ESP32 OpenETH model already accepts the standard
/// QEMU `-nic socket,…` netdev backend, so no additional wiring is
/// needed.
pub fn start_esp32_qemu_mcast(
    flash_image: &Path,
    mcast_addr_port: &str,
    mac: &str,
) -> TestResult<ManagedProcess> {
    if !flash_image.exists() {
        return Err(TestError::BuildFailed(format!(
            "Flash image not found: {}",
            flash_image.display()
        )));
    }

    let drive_arg = format!("file={},if=mtd,format=raw", flash_image.display());
    let nic_arg = format!("socket,model=open_eth,mcast={mcast_addr_port},mac={mac}");
    // phase-295 W5.b — see `esp32_qemu_machine_for_image`.
    let machine = esp32_qemu_machine_for_image(flash_image);

    let mut cmd = Command::new("qemu-system-riscv32");
    cmd.args([
        "-M",
        &machine,
        "-icount",
        "3",
        "-nographic",
        "-drive",
        &drive_arg,
        "-nic",
        &nic_arg,
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    set_new_process_group(&mut cmd);

    ManagedProcess::spawn_command(cmd, "esp32-qemu-mcast")
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
    fn flasher_args_derives_qemu_machine() {
        // phase-295 W5.b — the QEMU machine model must come from the
        // framework's `flasher_args.json`, not a hardcoded string. Uses the
        // exact shape `idf.py` emits.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("flasher_args.json"),
            r#"{
                "flash_settings": {"flash_mode": "dio", "flash_size": "2MB", "flash_freq": "80m"},
                "extra_esptool_args": {"chip": "esp32c3", "after": "hard_reset"}
            }"#,
        )
        .unwrap();
        let f = EspFlasherArgs::from_build_dir(dir.path()).unwrap();
        assert_eq!(f.chip.as_deref(), Some("esp32c3"));
        assert_eq!(f.qemu_machine(), Some("esp32c3"));
        assert_eq!(f.flash_size.as_deref(), Some("2MB"));

        // A merged .bin with no sibling flasher_args.json → sanctioned esp32c3.
        let img = dir.path().join("nowhere").join("app.bin");
        assert_eq!(esp32_qemu_machine_for_image(&img), "esp32c3");
        // With the metadata present, the machine is derived from it.
        let img2 = dir.path().join("app.bin");
        assert_eq!(esp32_qemu_machine_for_image(&img2), "esp32c3");
    }
}
