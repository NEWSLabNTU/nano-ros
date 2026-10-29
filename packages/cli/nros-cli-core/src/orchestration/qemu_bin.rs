//! phase-517 W6/W14 — the CLI's door to the ONE QEMU ladder (issue 1851).
//!
//! The ladder itself — `QEMU_SYSTEM_<ARCH>`, the checkout's pinned project
//! prefix, the pinned store — lives in `nros-qemu-ladder`, which the test
//! harness (`nros_tests::qemu::qemu_system_path`) calls too; the shell twin is
//! `nros_qemu_bin` (`scripts/lib/qemu-bin.sh`). This module only gathers the
//! CLI's inputs ([`for_sdk`]) and fixes its policy:
//!
//! * no `$PATH` rung — this answers "which QEMU should the command `nros
//!   build` PRINTS name", and a printed `qemu-system-arm` from `$PATH` is the
//!   distro build without the LAN9118 flow-control patch the store carries
//!   (issues 0830/0917). When no rung answers, the caller says how to
//!   provision one instead;
//! * a stale project prefix is SKIPPED (the reason joins the note), where the
//!   harness refuses it;
//! * the store's target list is read from the index (`[tool.qemu.source]
//!   configure`'s `--target-list`, which `check-qemu-configure` keeps equal to
//!   the prebuilt's recipe), so a target the store does not build
//!   (`qemu-system-riscv32`, the NuttX rv-virt board) is the host's
//!   ([`QemuBin::Host`]) and is never sent to `nros setup --tool qemu`.
//!
//! Front door: `nros locate tool qemu-system-<arch>`.

use std::path::Path;

pub use nros_qemu_ladder::{Ctx, QemuBin, StalePrefix, env_var, locate, targets_of_configure};

/// The real inputs, for the SDK rooted at `nano_ros_root`.
pub fn for_sdk(nano_ros_root: Option<&Path>) -> Ctx<'static> {
    let index = nano_ros_root.and_then(|r| {
        crate::orchestration::sdk_index::SdkIndex::load(&r.join("nros-sdk-index.toml")).ok()
    });
    let checkout = nano_ros_root
        .filter(|r| r.join(nros_build_paths::CHECKOUT_MARKER).is_file())
        .map(nros_build_paths::build_root);
    Ctx {
        getenv: &real_env,
        project_prefix: checkout.map(|b| b.join("qemu")),
        pin: index
            .as_ref()
            .and_then(|i| i.tool.get("qemu").map(|t| t.version.clone())),
        store_dir: index
            .as_ref()
            .and_then(|i| crate::orchestration::sdk_store::tool_dir(i, "qemu")),
        store_targets: index.as_ref().and_then(store_targets),
        stale_prefix: StalePrefix::Skip,
    }
}

/// The arches `[tool.qemu.source] configure`'s `--target-list` builds —
/// `arm-softmmu,riscv64-softmmu` → `["arm", "riscv64"]`. The prebuilt is held
/// to the same flags by `check-qemu-configure`, so this is the store's set
/// whichever way it was provisioned.
#[must_use]
pub fn store_targets(index: &crate::orchestration::sdk_index::SdkIndex) -> Option<Vec<String>> {
    let configure = index
        .tool
        .get("qemu")?
        .source
        .as_ref()?
        .configure
        .as_deref()?;
    targets_of_configure(configure)
}

fn real_env(k: &str) -> Option<std::ffi::OsString> {
    std::env::var_os(k)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The CLI's policy over the shared ladder: a stale prefix is skipped (and
    /// named in the note), never refused, and nothing answers from `$PATH`.
    #[test]
    fn the_cli_skips_a_stale_prefix_and_has_no_path_rung() {
        let d = std::env::temp_dir().join(format!("nros-qemu-bin-cli-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let bin = d.join("build/qemu/bin/qemu-system-arm");
        std::fs::create_dir_all(bin.parent().unwrap()).unwrap();
        std::fs::write(&bin, "").unwrap();
        std::fs::write(d.join("build/qemu/.nros-provenance"), "version = \"9.0\"\n").unwrap();
        let none = |_: &str| None;
        let ctx = Ctx {
            getenv: &none,
            project_prefix: Some(d.join("build/qemu")),
            pin: Some("11.0".into()),
            store_dir: None,
            store_targets: None,
            stale_prefix: StalePrefix::Skip,
        };
        match locate("qemu-system-arm", &ctx) {
            QemuBin::Missing { why } => {
                assert!(why.contains("is not the pinned QEMU"), "{why}");
                assert!(why.contains("nros setup --tool qemu"), "{why}");
            }
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
