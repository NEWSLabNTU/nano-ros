//! NuttX QEMU ARM virt binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the NuttX Rust / C / C++
//! examples. Moved out of `tests/nuttx_qemu.rs` (Phase 85.5).

use crate::{TestError, TestResult, project_root};
use once_cell::sync::OnceCell;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

// =============================================================================
// Prerequisite detection
// =============================================================================

/// `NUTTX_DIR` env var set and points to a valid kernel source tree.
pub fn is_nuttx_available() -> bool {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("Makefile").exists())
        .unwrap_or(false)
}

/// NuttX has been configured — `$NUTTX_DIR/include/nuttx/config.h` exists.
pub fn is_nuttx_configured() -> bool {
    std::env::var("NUTTX_DIR")
        .ok()
        .map(|dir| Path::new(&dir).join("include/nuttx/config.h").exists())
        .unwrap_or(false)
}

/// `arm-none-eabi-gcc` in PATH.
pub fn is_arm_gcc_available() -> bool {
    Command::new("arm-none-eabi-gcc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Trust that `just setup` installed the pinned NuttX nightly
/// toolchain. The pinned version lives in
/// `examples/qemu-arm-nuttx/rust-toolchain.toml` and is the
/// authoritative source — cargo auto-resolves it when invoked from
/// inside that directory tree. If setup wasn't run, the cargo build
/// will fail with an actionable "toolchain not found" message, which
/// is the correct behaviour per CLAUDE.md "fail on unmet preconditions".
pub fn is_nuttx_toolchain_available() -> bool {
    true
}

/// The board configuration a NuttX kernel was built for.
///
/// Issue 0743. `$NUTTX_DIR/nuttx` is ONE filename written by BOTH the arm
/// (`qemu-armv7a`) and the riscv (`rv-virt`) configurations — the shared kernel
/// tree holds one board config at a time and each `make` reconfigures it (see
/// resolved issue 0405, which fixed what that costs the build LANES). So the
/// file's presence says nothing about which architecture is in it, and a caller
/// that only asks `.exists()` will happily hand an arm test a RISC-V image.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NuttxArch {
    Arm,
    RiscV,
}

impl NuttxArch {
    /// `e_machine` for this architecture: `EM_ARM` / `EM_RISCV`.
    fn elf_machine(self) -> u16 {
        match self {
            NuttxArch::Arm => 0x28,
            NuttxArch::RiscV => 0xF3,
        }
    }

    fn board(self) -> &'static str {
        match self {
            NuttxArch::Arm => "qemu-armv7a",
            NuttxArch::RiscV => "rv-virt",
        }
    }

    fn rebuild_hint(self) -> &'static str {
        match self {
            NuttxArch::Arm => "just nuttx build-fixtures-arm",
            NuttxArch::RiscV => "just nuttx build-fixtures-riscv",
        }
    }

    fn from_elf_machine(m: u16) -> Option<Self> {
        match m {
            0x28 => Some(NuttxArch::Arm),
            0xF3 => Some(NuttxArch::RiscV),
            _ => None,
        }
    }
}

/// `e_machine` out of an ELF header, or `None` if this is not an ELF.
///
/// Bytes 0..4 are the magic, byte 5 is `EI_DATA` (endianness) and 16..18 is
/// `e_type`; `e_machine` is the `u16` at 18. Both NuttX targets are
/// little-endian, but read `EI_DATA` rather than assuming — the whole point of
/// this function is that it asks the file instead of trusting a name.
fn elf_machine(path: &Path) -> Option<u16> {
    let bytes = std::fs::read(path).ok()?;
    if bytes.len() < 20 || &bytes[0..4] != b"\x7fELF" {
        return None;
    }
    let (a, b) = (bytes[18], bytes[19]);
    Some(match bytes[5] {
        2 => u16::from_be_bytes([a, b]), // ELFDATA2MSB
        _ => u16::from_le_bytes([a, b]), // ELFDATA2LSB
    })
}

/// Path to a pre-built NuttX kernel image **for `arch`**.
///
/// Issue 0743. The predicate is "is there a kernel, and is it the architecture
/// you asked for" — never bare `.exists()`. On 2026-08-21 a sweep died with
/// "qemu-system-arm: … The image is from incompatible architecture" because the
/// old resolver answered the first question and not the second: the tree had
/// been reconfigured for riscv five days earlier, and every arm consumer was
/// still being handed that image.
///
/// The `Err` is a ready-to-print reason naming the rebuild, because "no kernel"
/// and "the wrong kernel" want different actions from whoever reads it.
pub fn nuttx_kernel_path_for(arch: NuttxArch) -> Result<PathBuf, String> {
    let dir = std::env::var("NUTTX_DIR").map_err(|_| "NUTTX_DIR not set".to_string())?;
    let kernel = Path::new(&dir).join("nuttx");
    if !kernel.exists() {
        return Err(format!(
            "NuttX kernel not built ({}) — run: {}",
            kernel.display(),
            arch.rebuild_hint()
        ));
    }
    match elf_machine(&kernel) {
        Some(m) if m == arch.elf_machine() => Ok(kernel),
        Some(m) => {
            let found = match NuttxArch::from_elf_machine(m) {
                Some(a) => format!("a {a:?} image"),
                None => format!("an unknown image (e_machine {m:#x})"),
            };
            Err(format!(
                "the NuttX kernel at {} is {found}, but this lane needs {:?} ({}). The arm and \
                 riscv configurations share that ONE filename and each `make` reconfigures the \
                 tree (issue 0743), so the last build wins. Reconfigure and rebuild: {}",
                kernel.display(),
                arch,
                arch.board(),
                arch.rebuild_hint(),
            ))
        }
        None => Err(format!(
            "{} is not an ELF file — the NuttX build did not produce a kernel",
            kernel.display()
        )),
    }
}

/// `cmake` in PATH (for C / C++ examples).
pub fn is_cmake_available() -> bool {
    Command::new("cmake")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

// =============================================================================
// Rust binary builders (cargo nightly + -Z build-std)
// =============================================================================

static NUTTX_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_rust_example(name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-nuttx/rust/{}", name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX example directory not found: {}",
            example_dir.display()
        )));
    }

    // Phase 177.8.c — NuttX Rust fixtures are built at the carve-out profile
    // (`nros_cargo_profile::NUTTX_RUST_PROFILE`, fat LTO) to dodge the
    // armv7a-nuttx-eabihf cross-CGU codegen miscompile that any `lto = "off"`
    // profile hits. Prefer that artifact; fall back to the ambient profile only
    // if it is all that is present (a fresh carve-out build always wins over a
    // stale broken one).
    let carve_out = nros_cargo_profile::target_dir(nros_cargo_profile::NUTTX_RUST_PROFILE);
    let release_binary_path = example_dir.join(format!(
        "target/armv7a-nuttx-eabihf/{}/{}",
        carve_out, binary_name
    ));
    let binary_path = if release_binary_path.exists() {
        release_binary_path
    } else {
        // Phase 177 / G4 — an image built at the ambient (lto=off) profile hits
        // the 177.8.c CGU miscompile: reboot loop before `main`, zero console
        // output, surfacing as "readiness pattern never observed" with no
        // diagnostics. Warn loudly so a stale/partial local build is recognised
        // instead of silently exercising the known-broken profile. CI builds the
        // carve-out via `just nuttx build-fixtures`, so this is local-dev only.
        let ambient = super::cargo_target_profile_dir();
        eprintln!(
            "[nros-tests] WARNING: no `{}` build of NuttX Rust fixture `{}` \
             found at {}; falling back to the `{}` profile, which hits the \
             177.8.c armv7a-nuttx-eabihf codegen bug (reboot loop before main → \
             no output → boot-readiness failure). Run `just nuttx \
             build-fixtures` to produce the `{}` build.",
            carve_out,
            binary_name,
            release_binary_path.display(),
            ambient,
            carve_out,
        );
        example_dir.join(format!(
            "target/armv7a-nuttx-eabihf/{}/{}",
            ambient, binary_name
        ))
    };
    super::require_prebuilt_binary_fresh(&binary_path)
}

// #132 — the role crates (`talker`, `listener`, …) have been LIB-ONLY since
// Phase 212.L.1 ("Component pkg shape — lib only, no [[bin]]"), so
// `build_rust_example` resolved a `[[bin]]` that no longer exists and every
// nuttx-rust rtos_e2e case silently fixture-skipped. The bootable images are
// now the `<role>_entry` ELFs (ffi-linked, locator baked to the NUTTX 7452
// port table by their `[[fixture]]` env). Resolve those instead. `build_rust_example`
// is retained for any caller that still wants the raw staticlib path.
#[allow(dead_code)]
fn _keep_build_rust_example() {
    let _ = build_rust_example;
}

pub fn build_nuttx_talker() -> TestResult<&'static Path> {
    NUTTX_TALKER_BINARY
        .get_or_try_init(|| require_entry_binary("talker", "talker"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_listener() -> TestResult<&'static Path> {
    NUTTX_LISTENER_BINARY
        .get_or_try_init(|| require_entry_binary("listener", "listener"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_service_server() -> TestResult<&'static Path> {
    NUTTX_SERVICE_SERVER_BINARY
        .get_or_try_init(|| require_entry_binary("service-server", "service-server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_service_client() -> TestResult<&'static Path> {
    NUTTX_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| require_entry_binary("service-client", "service-client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_action_server() -> TestResult<&'static Path> {
    NUTTX_ACTION_SERVER_BINARY
        .get_or_try_init(|| require_entry_binary("action-server", "action-server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_action_client() -> TestResult<&'static Path> {
    NUTTX_ACTION_CLIENT_BINARY
        .get_or_try_init(|| require_entry_binary("action-client", "action-client"))
        .map(|p| p.as_path())
}

// =============================================================================
// #127 — per-role Entry-pkg demos (build-assert).
// =============================================================================

/// Resolve a prebuilt NuttX `<role>_entry` bootable ELF.
///
/// Prebuilt by the `[[fixture]]` rows in `examples/fixtures.toml` (built via
/// `just nuttx build-examples` → `fixtures-build.sh nuttx rust` at the
/// `release` profile — the 177.8.c CGU-miscompile dodge — which runs
/// `nros sync` + cargo; the board-centric image link needs `NUTTX_DIR`).
/// `role` is hyphenated (`"service-server"`); since phase-338 W2 collapsed the
/// `-entry` package into the role package, `bin` is that same short name.
/// Mirrors [`build_rust_example`]'s
/// release-first profile resolution.
pub fn require_entry_binary(role: &str, bin: &str) -> TestResult<PathBuf> {
    let dir = project_root().join(format!("examples/qemu-arm-nuttx/rust/{role}"));
    if !dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX entry example not found: {}",
            dir.display()
        )));
    }
    // Same carve-out as `require_example_binary` above — prefer the profile the
    // builder forces, fall back to the ambient one with the artifact that a
    // local hand-build would have left.
    let carve_out = nros_cargo_profile::target_dir(nros_cargo_profile::NUTTX_RUST_PROFILE);
    let forced = dir.join(format!("target/armv7a-nuttx-eabihf/{carve_out}/{bin}"));
    let bin_path = if forced.exists() {
        forced
    } else {
        dir.join(format!(
            "target/armv7a-nuttx-eabihf/{}/{bin}",
            super::cargo_target_profile_dir()
        ))
    };
    super::require_prebuilt_binary_fresh(&bin_path)
}

// =============================================================================
// Phase 169.4b — NuttX Rust DDS fixture builders deleted alongside the
// Rust DDS retirement (Phase 169.2 deleted the example crates).
// =============================================================================

// =============================================================================
// C / C++ binary builders (CMake, via corrosion + nuttx_build_example)
// =============================================================================

static NUTTX_CPP_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_CPP_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

static NUTTX_C_TALKER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_LISTENER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_SERVICE_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_SERVICE_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_ACTION_SERVER_BINARY: OnceCell<PathBuf> = OnceCell::new();
static NUTTX_C_ACTION_CLIENT_BINARY: OnceCell<PathBuf> = OnceCell::new();

fn build_cmake_example(lang: &str, name: &str, binary_name: &str) -> TestResult<PathBuf> {
    let root = project_root();
    let example_dir = root.join(format!("examples/qemu-arm-nuttx/{}/{}", lang, name));

    if !example_dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX {lang} example not found: {}",
            example_dir.display()
        )));
    }

    let build_dir = example_dir.join("build-zenoh");
    let binary_path = build_dir.join(binary_name);
    super::require_prebuilt_binary_fresh_cmake(&binary_path)
}

pub fn build_nuttx_cpp_talker() -> TestResult<&'static Path> {
    NUTTX_CPP_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "talker", "cpp_talker"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_listener() -> TestResult<&'static Path> {
    NUTTX_CPP_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "listener", "cpp_listener"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_service_server() -> TestResult<&'static Path> {
    NUTTX_CPP_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "service-server", "cpp_service_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_service_client() -> TestResult<&'static Path> {
    NUTTX_CPP_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "service-client", "cpp_service_client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_action_server() -> TestResult<&'static Path> {
    NUTTX_CPP_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-server", "cpp_action_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_cpp_action_client() -> TestResult<&'static Path> {
    NUTTX_CPP_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("cpp", "action-client", "cpp_action_client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_talker() -> TestResult<&'static Path> {
    NUTTX_C_TALKER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "talker", "c_talker"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_listener() -> TestResult<&'static Path> {
    NUTTX_C_LISTENER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "listener", "c_listener"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_service_server() -> TestResult<&'static Path> {
    NUTTX_C_SERVICE_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-server", "c_service_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_service_client() -> TestResult<&'static Path> {
    NUTTX_C_SERVICE_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "service-client", "c_service_client"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_action_server() -> TestResult<&'static Path> {
    NUTTX_C_ACTION_SERVER_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-server", "c_action_server"))
        .map(|p| p.as_path())
}

pub fn build_nuttx_c_action_client() -> TestResult<&'static Path> {
    NUTTX_C_ACTION_CLIENT_BINARY
        .get_or_try_init(|| build_cmake_example("c", "action-client", "c_action_client"))
        .map(|p| p.as_path())
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal ELF header: magic, `EI_DATA`, and `e_machine` at offset 18.
    fn elf_header(data: u8, machine: u16) -> Vec<u8> {
        let mut v = vec![0u8; 20];
        v[0..4].copy_from_slice(b"\x7fELF");
        v[5] = data;
        let m = if data == 2 {
            machine.to_be_bytes()
        } else {
            machine.to_le_bytes()
        };
        v[18..20].copy_from_slice(&m);
        v
    }

    fn write(name: &str, bytes: &[u8]) -> PathBuf {
        let p = std::env::temp_dir().join(format!("nros-nuttx-arch-{name}"));
        std::fs::write(&p, bytes).expect("write fixture");
        p
    }

    /// Issue 0743 — the resolver's whole job is telling these two apart, so
    /// prove BOTH directions. Only the riscv half is reproducible against a real
    /// tree (the arm kernel needs a reconfigure), which is exactly why the
    /// positive case is pinned here rather than left to a lane.
    #[test]
    fn elf_machine_distinguishes_arm_from_riscv() {
        let arm = write("arm", &elf_header(1, 0x28));
        let riscv = write("riscv", &elf_header(1, 0xF3));
        assert_eq!(elf_machine(&arm), Some(NuttxArch::Arm.elf_machine()));
        assert_eq!(elf_machine(&riscv), Some(NuttxArch::RiscV.elf_machine()));
        assert_eq!(NuttxArch::from_elf_machine(0x28), Some(NuttxArch::Arm));
        assert_eq!(NuttxArch::from_elf_machine(0xF3), Some(NuttxArch::RiscV));
        assert_eq!(NuttxArch::from_elf_machine(0x3E), None);
    }

    #[test]
    fn elf_machine_reads_big_endian_headers() {
        let be = write("be", &elf_header(2, 0x28));
        assert_eq!(
            elf_machine(&be),
            Some(0x28),
            "EI_DATA=2 is ELFDATA2MSB — reading e_machine little-endian there \
             yields 0x2800 and would call an arm kernel unknown"
        );
    }

    #[test]
    fn a_non_elf_is_not_a_kernel() {
        let junk = write("junk", b"#!/bin/sh\necho not a kernel\n");
        assert_eq!(elf_machine(&junk), None);
        let short = write("short", b"\x7fELF");
        assert_eq!(
            elf_machine(&short),
            None,
            "truncated header must not index past the end"
        );
    }
}
