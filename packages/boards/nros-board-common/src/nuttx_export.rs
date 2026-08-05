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
/// Emits the `rerun-if-env-changed` lines its own inputs need. Callers add
/// `rerun-if-changed` for the specific paths they consume.
pub fn snapshot_root(nuttx_dir: &Path) -> Option<PathBuf> {
    println!("cargo:rerun-if-env-changed=NUTTX_EXPORT_DIR");
    if let Ok(explicit) = std::env::var("NUTTX_EXPORT_DIR") {
        let p = PathBuf::from(explicit);
        if p.join("libs").is_dir() {
            return Some(p);
        }
    }
    let target_arch = std::env::var("CARGO_CFG_TARGET_ARCH").ok()?;
    let arch = snapshot_arch(&target_arch)?;
    let p = nuttx_dir.join(format!("nros-nuttx-export-{arch}"));
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
        if p.exists() {
            return p;
        }
    }
    nuttx_dir.join(tree_rel)
}
