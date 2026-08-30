//! phase-339 — where a NuttX consumer finds the kernel it links against.
//!
//! # Why this exists
//!
//! Both architectures build in ONE in-tree NuttX checkout (NuttX holds a single
//! `.config` at a time), so the live `staging/` dir belongs to whichever arch
//! built last. Linking it meant an arm entry could be relinked against riscv
//! archives, and — because `build.rs` correctly declares
//! `cargo:rerun-if-changed=<staging>` to avoid exactly that — every arm entry
//! read STALE the moment riscv built. Its cells then stopped running (issue
//! 0433).
//!
//! `build-nuttx.sh` now writes a per-arch SNAPSHOT of `make export`
//! (`nros-nuttx-export-<arch>/`), which is NuttX's own build-once-link-many
//! mechanism. Consumers resolve that instead, so one arch's build cannot
//! invalidate the other's binaries.
//!
//! # One home for the rule
//!
//! Two consumers need it — `nuttx_ffi_build` (the kernel link) and
//! `nuttx_image_link` (the bootable image). A second spelling of "where is the
//! kernel" is how the two `TierRtosSpec` mirrors drifted, so the resolution
//! lives here once.

use std::path::{Path, PathBuf};

/// The snapshot arch token for a cargo target arch.
///
/// `CARGO_CFG_TARGET_ARCH` is `arm` / `riscv32` / `riscv64`; the snapshot is
/// keyed on NuttX's own `CONFIG_ARCH` with the hyphen dropped (`arm`,
/// `riscv` — NuttX spells it `risc-v`).
fn snapshot_arch(target_arch: &str) -> Option<&'static str> {
    match target_arch {
        "arm" | "aarch64" => Some("arm"),
        a if a.starts_with("riscv") => Some("riscv"),
        _ => None,
    }
}

/// Root of the per-arch export snapshot for the build currently compiling, or
/// `None` when no snapshot exists for this arch.
///
/// Resolution order:
/// 1. `$NUTTX_EXPORT_DIR` — explicit override (a caller that already knows).
/// 2. `$NUTTX_DIR/nros-nuttx-export-<arch>` for this target's arch.
///
/// Emits the `rerun-if-changed` its own inputs need. Callers add more for the
/// specific paths they consume.
///
/// issue 0491 — `NUTTX_EXPORT_DIR` is a PATH, so it is watched by CONTENT and
/// never fingerprinted as a string: cargo compares an env value textually, and
/// the same directory has one spelling per example leaf, another from `just`,
/// and none from a bare build, so rows sharing one `--target-dir` re-ran this
/// forever.
pub fn snapshot_root(nuttx_dir: &Path) -> Option<PathBuf> {
    if let Some(explicit) = nros_build_paths::env_path("NUTTX_EXPORT_DIR") {
        let libs = explicit.join("libs");
        if libs.is_dir() {
            println!("cargo:rerun-if-changed={}", libs.display());
            return Some(explicit);
        }
    }
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok()?;
    let arch = snapshot_arch(&target_arch)?;
    let p = nuttx_dir.join(format!("nros-nuttx-export-{arch}"));
    // issue 0477 — watch the candidate WHETHER OR NOT it exists. A
    // `rerun-if-changed` on a path cargo cannot find still registers: the
    // artifact is invalidated when that path later APPEARS. Without this line
    // the edge is emitted only on whatever won, so a build that resolved to the
    // `staging/` fallback keeps its artifact forever once the snapshot shows
    // up — which is how a NuttX C image ended up linked against another arch's
    // staging tree and overflowed ROM by 448 KB with nothing wrong in the code.
    println!("cargo:rerun-if-changed={}", p.join("libs").display());
    p.join("libs").is_dir().then_some(p)
}

/// Where the linkable archives live, and whether they came from a snapshot.
///
/// Falls back to the live `staging/` when this arch has no snapshot, so a tree
/// provisioned by an older `build-nuttx.sh` still links. The fallback carries
/// the pre-phase-339 hazard by definition — it is a compatibility path, not a
/// supported configuration.
pub struct KernelLibs {
    /// Directory holding `lib*.a`.
    pub libs: PathBuf,
    /// True when `libs` is a per-arch snapshot (safe to watch), false when it
    /// is the shared live `staging/` (the issue-0433 hazard).
    pub from_snapshot: bool,
    /// Snapshot root, when there is one — for the linker script, startup
    /// objects and headers that live beside `libs/`.
    pub root: Option<PathBuf>,
}

/// Resolve the kernel archives for this build.
pub fn kernel_libs(nuttx_dir: &Path) -> KernelLibs {
    if let Some(root) = snapshot_root(nuttx_dir) {
        return KernelLibs {
            libs: root.join("libs"),
            from_snapshot: true,
            root: Some(root),
        };
    }
    KernelLibs {
        libs: nuttx_dir.join("staging"),
        from_snapshot: false,
        root: None,
    }
}

/// Resolve a path that the snapshot provides but the live tree spells
/// differently.
///
/// `snapshot_rel` is relative to the snapshot root (e.g. `scripts/dramboot.ld`,
/// `startup/arm_vectortab.o`); `tree_rel` is the live-tree spelling used before
/// phase-339 (e.g. `boards/arm/qemu/qemu-armv7a/scripts/dramboot.ld`). The
/// snapshot wins when it has the file, so a half-migrated tree still links.
pub fn snapshot_or_tree(
    libs: &KernelLibs,
    nuttx_dir: &Path,
    snapshot_rel: &str,
    tree_rel: &str,
) -> PathBuf {
    if let Some(root) = &libs.root {
        let p = root.join(snapshot_rel);
        // issue 0477 — same rule as `snapshot_root`: watch the preferred path
        // even on the losing branch, so this artifact rebuilds when the
        // snapshot gains the file rather than staying pinned to the fallback.
        println!("cargo:rerun-if-changed={}", p.display());
        if p.exists() {
            return p;
        }
    }
    nuttx_dir.join(tree_rel)
}

/// The include root whose `nuttx/config.h` describes THIS build's arch.
///
/// Issue 0511 — NuttX is built IN PLACE, so `$NUTTX_DIR/include/nuttx/config.h`
/// belongs to whichever arch the shared tree was configured for LAST. Every
/// input derived from those macros therefore silently takes the other arch's
/// values when two arches share one checkout, which `lane=tier2` does (it
/// builds nuttx and nuttx-riscv, riscv last).
///
/// That is not hypothetical: the ARM Rust image was linked with the RISC-V
/// memory map, whose `CONFIG_FLASH_SIZE` is 0, so `MEMORY { ROM ... LENGTH =
/// CONFIG_FLASH_SIZE }` gave ROM zero bytes and every byte placed in it
/// "overflowed".
///
/// phase-339 W2 already made the LIBS and the linker SCRIPT per-arch via the
/// export snapshot; the headers were left on the shared tree, so the arch
/// selection covered two of the three input classes. This is the third.
///
/// Returns the snapshot's `include/` when it carries a `nuttx/config.h`, else
/// the live tree's — a pre-phase-339 checkout keeps working. Emits
/// `rerun-if-changed` on BOTH spellings whether or not they exist (issue 0477's
/// rule): the config IS the memory map and the ABI, so a reconfigure must
/// invalidate anything derived from it, including on the losing branch.
pub fn include_root(nuttx_dir: &Path) -> PathBuf {
    // Issue 0525 — ONE spelling of this resolution, shared with any standalone
    // build script that cannot reach these board helpers. (`nuttx-sys`'s bindgen
    // script was the original such consumer; phase-400 deleted it as
    // unreferenced, but the shared spelling is the point, not the consumer.)
    nros_build_paths::nuttx_include_root(nuttx_dir)
}
