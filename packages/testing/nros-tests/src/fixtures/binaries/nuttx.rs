//! NuttX QEMU ARM virt binary builders.
//!
//! Cached `OnceCell<PathBuf>` fixtures for the NuttX Rust / C / C++
//! examples. Moved out of `tests/nuttx_qemu.rs` (Phase 85.5).

use crate::{TestError, TestResult, fixtures::groups, project_root};
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

    /// The export-snapshot directory this arch's images link, relative to
    /// `$NUTTX_DIR` — `build-nuttx.sh`'s `nros-nuttx-export-<config-id>`.
    ///
    /// Keyed on the CONFIG id, which today equals the arch for both live
    /// configurations (`nuttx-config/arm/`, `nuttx-config/riscv/`). A second
    /// config of one arch (`arm-smp`) gets its own directory and would need its
    /// own token here; the `e_machine` probe below cannot tell two configs of
    /// one arch apart, which `build-nuttx.sh` says in as many words.
    fn snapshot_id(self) -> &'static str {
        match self {
            NuttxArch::Arm => "arm",
            NuttxArch::RiscV => "riscv",
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

/// Root of the pre-built NuttX kernel export **for `arch`** — the artifact this
/// lane's images actually link.
///
/// # Why this reads the SNAPSHOT and not `$NUTTX_DIR/nuttx` (issue 1007)
///
/// It used to read the shared tree's `nuttx` ELF. That made the build contract
/// and the test precondition disagree about which artifact is authoritative,
/// and the disagreement had a name: `build-nuttx.sh`'s snapshot short-circuit
/// says outright that it "guarantees the snapshot, not `$NUTTX_DIR`", so a
/// clean `just nuttx build-fixtures-arm` on a riscv-configured tree exited 0
/// while every arm cell skipped — and the skip named that same command as the
/// remedy.
///
/// The disagreement was resolvable in one direction only, because
/// `$NUTTX_DIR/nuttx` is consumed by NOTHING on the path this precondition
/// guards:
///
/// * the arm cells boot the FIXTURE binary (`QemuProcess::start_nuttx_virt`),
///   never the shared tree's kernel ELF;
/// * that fixture links `nros-nuttx-export-<arch>/{libs,startup,scripts}` and
///   compiles against its `include/` — phase-339's build-once-link-many
///   snapshot (`nros_board_common::nuttx_export`,
///   `nros_build_paths::nuttx_include_root`, `nano-ros-nuttx.cmake`), with
///   `check-nuttx-links-snapshot.sh` gating that no consumer reaches back into
///   the live tree.
///
/// So this was the last consumer still keyed on the shared tree, checking an
/// artifact no image links. Making the SHORT-CIRCUIT validate the shared tree
/// instead (the other candidate direction) would have reverted issue 0433: the
/// tree holds one `.config`, so demanding it hold arm forces a full
/// reconfigure + kernel rebuild on every lane alternation, which is exactly
/// what the snapshot exists to stop — and it would break the riscv lane
/// symmetrically, one lane at a time, forever.
///
/// With this, ONE command — `just nuttx build-fixtures-arm` — makes an arm cell
/// runnable from any tree state, because that command's C/C++ half
/// self-provisions through `build-nuttx.sh`, whose short-circuit guarantees
/// precisely the snapshot checked here.
///
/// # Still content-checked (issue 0743)
///
/// The predicate stays "is there a kernel export, and is it the architecture
/// you asked for" — never bare `.exists()`. The snapshot dirs are per-arch, so
/// the 0743 failure (one filename, two arches, last build wins) cannot recur
/// structurally; the `e_machine` probe on `startup/crt0.o` is kept anyway
/// because a mis-keyed or half-moved snapshot is still possible and asking the
/// file costs one read.
///
/// The `Err` is a ready-to-print reason naming the rebuild, because "no export"
/// and "the wrong export" want different actions from whoever reads it.
pub fn nuttx_kernel_path_for(arch: NuttxArch) -> Result<PathBuf, String> {
    let dir = std::env::var("NUTTX_DIR").map_err(|_| "NUTTX_DIR not set".to_string())?;
    let snapshot = Path::new(&dir).join(format!("nros-nuttx-export-{}", arch.snapshot_id()));
    snapshot_for_arch(&snapshot, arch).map_err(|why| {
        format!(
            "{why} Run: {}{}",
            arch.rebuild_hint(),
            shared_tree_note(&dir, arch)
        )
    })?;
    Ok(snapshot)
}

/// Is `snapshot` a usable kernel export for `arch`?
///
/// Split out from [`nuttx_kernel_path_for`] so the decision is testable without
/// a provisioned NuttX tree — the arm half of issue 0743 was untestable for
/// exactly that reason, and the reason is the environment lookup, not the rule.
/// The returned string is a sentence; the caller appends the remedy.
fn snapshot_for_arch(snapshot: &Path, arch: NuttxArch) -> Result<(), String> {
    if !snapshot.join("libs").is_dir() {
        return Err(format!(
            "no NuttX {arch:?} kernel export at {} — this lane's images link \
             `<export>/libs` and nothing has built it.",
            snapshot.display(),
        ));
    }
    // `make export` ships the startup objects; `crt0.o` is present for both
    // configurations. When it is not, the export's own presence is the whole
    // contract: the directory is arch-KEYED, so there is no last-build-wins
    // ambiguity left to resolve, and refusing here would fail a host whose
    // export is fine.
    let probe = snapshot.join("startup/crt0.o");
    match elf_machine(&probe) {
        None => Ok(()),
        Some(m) if m == arch.elf_machine() => Ok(()),
        Some(m) => {
            let found = match NuttxArch::from_elf_machine(m) {
                Some(a) => format!("a {a:?} export"),
                None => format!("an unknown export (e_machine {m:#x})"),
            };
            Err(format!(
                "the NuttX kernel export at {} is {found}, but this lane needs {arch:?} ({}) — \
                 the per-arch snapshot holds the wrong architecture, which means it was written \
                 by a build whose defconfig disagrees with its directory.",
                snapshot.display(),
                arch.board(),
            ))
        }
    }
}

/// A one-line note about the shared tree, for the reader who is about to check
/// it and be misled.
///
/// Issue 1007's whole cost was a diagnostic that pointed at the shared tree. It
/// is genuinely normal for `$NUTTX_DIR` to hold the OTHER architecture — the
/// tree carries one `.config`, and since phase-339 nothing links it — so say so
/// rather than leaving the next reader to rediscover it. Empty when the tree
/// holds this arch (or no kernel at all), so the message stays short in the
/// common case.
fn shared_tree_note(nuttx_dir: &str, arch: NuttxArch) -> String {
    let kernel = Path::new(nuttx_dir).join("nuttx");
    match elf_machine(&kernel).and_then(NuttxArch::from_elf_machine) {
        Some(held) if held != arch => format!(
            "\n  (The shared tree at {} currently holds a {held:?} kernel. That is expected and \
             is NOT the problem: since phase-339 every consumer links the per-arch export \
             snapshot, not the tree.)",
            kernel.display(),
        ),
        _ => String::new(),
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

// #132 — the role crates (`talker`, `listener`, …) have been LIB-ONLY since
// Phase 212.L.1 ("Component pkg shape — lib only, no [[bin]]"), so the old
// `build_rust_example` resolved a `[[bin]]` that no longer exists and every
// nuttx-rust rtos_e2e case silently fixture-skipped. The bootable images are
// now the `<role>` ELFs (ffi-linked, locator baked to the NUTTX 7452 port table
// by their `[[fixture]]` env), resolved by [`require_entry_binary`].
//
// Issue 1027 deleted `build_rust_example`. It was dead (`#[allow(dead_code)]`,
// kept alive only by a `_keep_` shim) and it was a SECOND spelling of the same
// artifact path, so the leaf-`target/` defect had to be fixed in two places
// that nothing kept in agreement — which is exactly how the fallback arm ended
// up looking in the wrong directory. One resolver, one spelling.

/// The target triple the NuttX **arm** Rust fixtures are built for.
///
/// Not a profile and not an artifact root: it is the `--target` cargo was
/// given, and it is the one component of a fixture's relative path that no
/// manifest row carries — `GroupRow` holds the row's artifact root, platform,
/// group and coordinate, while the triple lives in the leaf's
/// `.cargo/config.toml` (and, for the sibling platforms, in the row's `target`
/// key). Everything else below is asked, not spelled.
const NUTTX_ARM_TARGET: &str = "armv7a-nuttx-eabihf";

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

/// Resolve a prebuilt NuttX `<role>` bootable ELF.
///
/// Prebuilt by the `[[fixture]]` rows in `examples/fixtures.toml` (built via
/// `just nuttx build-examples` → `fixtures-build.sh nuttx rust` at the NuttX
/// carve-out profile — the 177.8.c CGU-miscompile dodge — which runs
/// `nros sync` + cargo; the board-centric image link needs `NUTTX_DIR`).
/// `role` is hyphenated (`"service-server"`); since phase-338 W2 collapsed the
/// `-entry` package into the role package, `bin` is that same short name.
///
/// # Why this asks the manifest instead of building a path (issue 1027)
///
/// It used to probe `<leaf>/target/<triple>/<carve-out>/<bin>` and, when that
/// missed, `<leaf>/target/<triple>/<ambient>/<bin>`. Phase-340 moved the NuttX
/// build into a shared cargo group dir, so under a clean
/// `just nuttx build-fixtures-arm` NEITHER leaf path exists — MEASURED: no
/// `examples/qemu-arm-nuttx/rust/talker/target` at all, while
/// `build/cargo-fixtures/nuttx-*/armv7a-nuttx-eabihf/nros-minsizerel/talker`
/// was freshly built. The first `.exists()` therefore always missed, the
/// ambient arm always won, and the path route's root redirect then reported the
/// binary missing at the AMBIENT profile — a working image, one directory over,
/// reported as not prebuilt, with the miscompile warning firing for a reason
/// that had nothing to do with the profile.
///
/// So the row answers both questions the literals were guessing at:
/// [`groups::select_sole_row`] + [`groups::row_resolved_dir`] give the artifact
/// ROOT (leaf or group, decided by the row's own `shared`/`slug`), and
/// [`super::row_profile_dir`] gives the PROFILE from the row's coordinate. The
/// only literal left is the target triple, which no row carries.
///
/// The miscompile warning is KEPT and now means what it says: the artifact is
/// at the WRONG PROFILE — the ambient one is on disk and the carve-out is not —
/// rather than "the artifact is not where I looked".
pub fn require_entry_binary(role: &str, bin: &str) -> TestResult<PathBuf> {
    let dir_rel = format!("examples/qemu-arm-nuttx/rust/{role}");
    let dir = project_root().join(&dir_rel);
    if !dir.exists() {
        return Err(TestError::BuildFailed(format!(
            "NuttX entry example not found: {}",
            dir.display()
        )));
    }
    let row = groups::select_sole_row(&dir_rel)?;
    let rel = |profile: &str| PathBuf::from(format!("{NUTTX_ARM_TARGET}/{profile}/{bin}"));

    // Phase 177.8.c — NuttX Rust fixtures are built at the carve-out profile
    // (`nros_cargo_profile::platform_profile("nuttx")`, fat LTO) to dodge the
    // armv7a-nuttx-eabihf cross-CGU codegen miscompile that any `lto = "off"`
    // profile hits.
    let carve_out = super::row_profile_dir(row);
    let ambient = super::cargo_target_profile_dir();
    let root = groups::row_resolved_dir(row);

    if carve_out != ambient
        && !root.join(rel(&carve_out)).exists()
        && root.join(rel(&ambient)).exists()
    {
        // Phase 177 / G4 — an image built at the ambient (lto=off) profile hits
        // the 177.8.c CGU miscompile: reboot loop before `main`, zero console
        // output, surfacing as "readiness pattern never observed" with no
        // diagnostics. Warn loudly so a local hand-build at the ambient profile
        // is recognised instead of silently exercising the known-broken one. CI
        // builds the carve-out via `just nuttx build-fixtures`, so this is
        // local-dev only.
        //
        // The condition is now three-part on purpose: the ambient artifact must
        // actually BE there. "Carve-out absent" alone is also what an unbuilt
        // fixture looks like, and warning about a codegen bug on a tree that
        // simply has not been built is the cry-wolf issue 1027 measured.
        eprintln!(
            "[nros-tests] WARNING: NuttX Rust fixture `{bin}` is present at the \
             ambient `{ambient}` profile but NOT at the `{carve_out}` carve-out \
             ({}); running the ambient build, which hits the 177.8.c \
             armv7a-nuttx-eabihf codegen bug (reboot loop before main → no \
             output → boot-readiness failure). Run `just nuttx build-fixtures` \
             to produce the `{carve_out}` build.",
            root.join(rel(&carve_out)).display(),
        );
        // The PATH route for this arm, not the row route: `rel_at_row_profile`
        // rewrites a rel's profile component to the platform's carve-out, which
        // is exactly what this arm must NOT do. `groups::resolved` redirects the
        // artifact ROOT only, so the ambient profile survives — and the row's
        // own `artifact_root` is what the lane narrowing attributes by.
        return super::require_prebuilt_binary_fresh(
            &project_root().join(&row.artifact_root).join(rel(&ambient)),
        );
    }
    super::require_prebuilt_row_binary_fresh(row, &rel(&carve_out))
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

    /// A snapshot dir for issue 1007's tests: `libs/` plus an optional
    /// `startup/crt0.o` of a chosen machine.
    fn snapshot(name: &str, libs: bool, crt0: Option<u16>) -> PathBuf {
        let root = std::env::temp_dir().join(format!("nros-nuttx-export-test-{name}"));
        let _ = std::fs::remove_dir_all(&root);
        if libs {
            std::fs::create_dir_all(root.join("libs")).expect("libs");
        } else {
            std::fs::create_dir_all(&root).expect("root");
        }
        if let Some(m) = crt0 {
            std::fs::create_dir_all(root.join("startup")).expect("startup");
            std::fs::write(root.join("startup/crt0.o"), elf_header(1, m)).expect("crt0");
        }
        root
    }

    /// Issue 1007 — the precondition now asks the artifact the images link.
    /// A riscv-configured SHARED tree is irrelevant to it, which is the whole
    /// point: that state used to skip every arm cell while naming as the remedy
    /// the command that had just short-circuited.
    #[test]
    fn a_present_export_satisfies_its_own_arch() {
        let arm = snapshot("arm-ok", true, Some(0x28));
        assert!(snapshot_for_arch(&arm, NuttxArch::Arm).is_ok());
        let riscv = snapshot("riscv-ok", true, Some(0xF3));
        assert!(snapshot_for_arch(&riscv, NuttxArch::RiscV).is_ok());
    }

    #[test]
    fn a_missing_export_names_the_export_not_the_tree() {
        let none = snapshot("absent", false, None);
        let why = snapshot_for_arch(&none, NuttxArch::Arm).expect_err("no libs ⇒ no export");
        assert!(
            why.contains("kernel export") && why.contains("libs"),
            "the message must point at the export the images link, not at \
             $NUTTX_DIR/nuttx: {why}"
        );
    }

    #[test]
    fn a_wrong_arch_export_is_still_caught() {
        let mismatched = snapshot("arm-holding-riscv", true, Some(0xF3));
        let why = snapshot_for_arch(&mismatched, NuttxArch::Arm).expect_err("riscv crt0 in arm/");
        assert!(why.contains("RiscV"), "must name what it found: {why}");
    }

    /// The export's PRESENCE is the contract when no startup object can be
    /// read — the directory is arch-keyed, so there is no last-build-wins
    /// ambiguity left, and failing here would fail a host whose export is fine.
    #[test]
    fn an_export_without_a_startup_probe_is_accepted() {
        let no_probe = snapshot("no-crt0", true, None);
        assert!(snapshot_for_arch(&no_probe, NuttxArch::Arm).is_ok());
    }

    /// The shared tree's architecture is a NOTE, never the verdict — and it is
    /// silent when it agrees, so the common failure stays one line.
    #[test]
    fn the_shared_tree_note_fires_only_on_a_mismatch() {
        let tree = std::env::temp_dir().join("nros-nuttx-shared-tree-note");
        std::fs::create_dir_all(&tree).expect("tree");
        std::fs::write(tree.join("nuttx"), elf_header(1, 0xF3)).expect("kernel");
        let dir = tree.to_str().expect("utf8");
        assert!(shared_tree_note(dir, NuttxArch::Arm).contains("RiscV"));
        assert_eq!(shared_tree_note(dir, NuttxArch::RiscV), "");
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
