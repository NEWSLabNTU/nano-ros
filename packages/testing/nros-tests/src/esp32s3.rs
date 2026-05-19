//! Phase 117.4 — ESP32-S3 QEMU test infrastructure.
//!
//! Sibling of `crate::esp32` (which covers the ESP32-C3 / RISC-V
//! path). The ESP32-S3 line uses the Xtensa LX7 instruction set
//! plus Espressif's QEMU `xtensa-softmmu` build (no machine model
//! for `esp32s3` lives in mainline QEMU as of `qemu-system-xtensa`
//! 6.2.x — see `scripts/esp32/install-espressif-qemu.sh` +
//! `book/src/reference/build-commands.md` Phase 117.0 section for
//! the install flow). Toolchain side: Xtensa rustc is out-of-tree
//! (`esp-rs/rust` fork), so the talker / listener binaries require
//! `espup install --targets esp32s3` + `. $HOME/export-esp.sh` in
//! the build shell.
//!
//! These helpers are intentionally narrow:
//! - `is_qemu_xtensa_available` / `require_qemu_xtensa` — match the
//!   pattern from the C3 module, but probe for the `esp32s3`
//!   machine string instead of `esp32c3`.
//! - `start_esp32s3_qemu_mcast` — mirrors `start_esp32_qemu_mcast`
//!   on the Xtensa side; same `-nic socket,model=open_eth,mcast=`
//!   shape because the OpenETH register block lives at the same
//!   `DR_REG_EMAC_BASE` on both chips.
//!
//! The flash-image build helper (`create_esp32_flash_image` in
//! `crate::esp32`) currently calls `espflash save-image` with the
//! `--chip esp32c3` arg. It needs a parameterised chip selector to
//! cover ESP32-S3 too — that's Phase 117.5's responsibility once
//! the E2E test wires the flash-image creation through. Until
//! then, callers can either shell out to `espflash save-image
//! --chip esp32s3 …` directly or extend the helper.

use crate::{
    TestError, TestResult,
    process::{ManagedProcess, set_new_process_group},
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::Duration,
};

/// Probe whether `qemu-system-xtensa` is available AND advertises
/// the `esp32s3` machine model. Stock Debian / Ubuntu's
/// `qemu-system-xtensa` package only ships generic Xtensa boards
/// (`lx60`, `lx200`, etc.); the `esp32s3` model requires
/// Espressif's QEMU fork (built via `just esp32 setup-xtensa`).
pub fn is_qemu_xtensa_available() -> bool {
    let Ok(out) = Command::new("qemu-system-xtensa")
        .arg("-machine")
        .arg("help")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
    else {
        return false;
    };
    if !out.status.success() {
        return false;
    }
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|line| line.split_whitespace().next() == Some("esp32s3"))
}

/// `nros_tests::skip!`-compatible precondition. Returns `true` when
/// the Espressif `qemu-system-xtensa` build with the `esp32s3`
/// machine model is on PATH; otherwise prints the install hint and
/// returns `false`.
pub fn require_qemu_xtensa() -> bool {
    if is_qemu_xtensa_available() {
        return true;
    }
    eprintln!(
        "[esp32s3] qemu-system-xtensa with `esp32s3` machine not found.\n\
         Install via: just esp32 setup-xtensa (Phase 117.0 — wraps\n\
         scripts/esp32/install-espressif-qemu.sh with NROS_ESP32_QEMU_TARGETS=riscv32,xtensa).\n\
         The Espressif QEMU fork ships at third-party/esp32/qemu and\n\
         lands at ~/.local/bin/qemu-system-xtensa after install."
    );
    false
}

/// Probe whether the Xtensa rustc fork (`+esp` toolchain channel)
/// is installed via `espup`. Required to build any
/// `xtensa-esp32s3-none-elf` binary.
pub fn is_xtensa_esp32s3_target_available() -> bool {
    // `rustup +esp target list --installed` would shell out per
    // probe; the lighter check is the directory created by espup.
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from);
    home.map(|p| p.join(".rustup/toolchains/esp"))
        .is_some_and(|p| p.exists())
}

/// `nros_tests::skip!`-compatible precondition for the Xtensa
/// rustc fork. Prints the espup install hint on miss.
pub fn require_xtensa_esp32s3_target() -> bool {
    if is_xtensa_esp32s3_target_available() {
        return true;
    }
    eprintln!(
        "[esp32s3] `+esp` rustc toolchain not installed.\n\
         Install via:\n\
           cargo install espup\n\
           espup install --targets esp32s3\n\
           . $HOME/export-esp.sh\n\
         See book/src/reference/build-commands.md \"ESP32 / ESP32-S3\n\
         QEMU Setup\" for the full flow."
    );
    false
}

/// Start an Espressif `qemu-system-xtensa -M esp32s3` instance on
/// a shared `-nic socket,mcast=…` segment so two sibling guests
/// can exchange RTPS SPDP / SEDP / pubsub on the same virtual L2
/// broadcast domain. Mirrors `start_esp32_qemu_mcast` (C3 side).
pub fn start_esp32s3_qemu_mcast(
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

    let mut cmd = Command::new("qemu-system-xtensa");
    cmd.args([
        "-M",
        "esp32s3",
        // `-m N`: QEMU's `machine->ram_size` controls whether
        // Espressif's ESP32-S3 SoC model instantiates the
        // `SsiPsramState` (verified at
        // `third-party/esp32/qemu/hw/xtensa/esp32s3.c:687-689` —
        // `if (machine->ram_size > 0) { esp32s3_machine_init_psram(
        // ms, machine->ram_size / MiB); }`). Without `-m`, the
        // SoC reports a PSRAM address via `psram_raw_parts` but
        // the address has no backing memory → first write hangs.
        // 4 MiB matches Phase 117's heap budget target; tunable
        // upward to 8 / 16 MiB if dust-dds needs more headroom.
        "-m",
        "4M",
        // `-icount 3` worked on the ESP32-C3 emulator (matched
        // RISC-V HAL timer rates). Xtensa LX7 + Espressif QEMU's
        // ESP32-S3 model uses different timer ratios; start with
        // wall-clock (no icount) and re-evaluate if RTPS timing
        // jitters under instruction-throttled emulation — same
        // approach the NuttX dgram-pair launcher took (Phase
        // 160.K).
        "-nographic",
        "-drive",
        &drive_arg,
        "-nic",
        &nic_arg,
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    #[cfg(unix)]
    set_new_process_group(&mut cmd);

    ManagedProcess::spawn_command(cmd, "esp32s3-qemu-mcast")
}

/// Convenience: poll the QEMU process's combined stdout/stderr for
/// `pattern` up to `timeout`. Returns `true` on first match. Used
/// by the E2E talker / listener tests to wait for boot banners
/// ("Ethernet ready" etc.) before kicking off cross-traffic.
pub fn wait_for_pattern(proc: &mut ManagedProcess, pattern: &str, timeout: Duration) -> bool {
    let combined = proc.wait_for_output_pattern(pattern, timeout);
    matches!(combined, Ok(out) if out.contains(pattern))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_qemu_xtensa_detection() {
        let available = is_qemu_xtensa_available();
        eprintln!("qemu-system-xtensa esp32s3 available: {}", available);
    }

    #[test]
    fn test_xtensa_esp32s3_target_detection() {
        let available = is_xtensa_esp32s3_target_available();
        eprintln!("+esp toolchain available: {}", available);
    }
}
