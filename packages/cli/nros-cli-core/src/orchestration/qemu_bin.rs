//! phase-517 W6 — the CLI's spelling of the ONE QEMU ladder (issue 1851).
//!
//! Which `qemu-system-<arch>` runs an image is decided by one ladder, whose
//! twins are `nros_tests::qemu::qemu_system_path` (tests) and `nros_qemu_bin`
//! (`scripts/lib/qemu-bin.sh`, shell and `just`):
//!
//!   1. `QEMU_SYSTEM_<ARCH>` — the developer override / CI pin;
//!   2. a checkout's project prefix `<build root>/qemu/bin/` (made by
//!      `just qemu setup-qemu`), only when its `.nros-provenance` names the
//!      version the index pins (issue 0930);
//!   3. the `nros setup` store at the PINNED `[tool.qemu]` version.
//!
//! The twins' fourth rung, `$PATH`, is deliberately NOT here: this answers
//! "which QEMU should the command `nros build` PRINTS name", and a printed
//! `qemu-system-arm` from `$PATH` is exactly the distro build without the
//! LAN9118 flow-control patch that the store carries (issues 0830/0917). When
//! no rung answers, the caller says how to provision one instead.
//!
//! Front door: `nros locate tool qemu-system-<arch>`.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// Where the QEMU came from, or why there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QemuBin {
    /// A rung answered.
    Found {
        path: PathBuf,
        /// Which rung (`env`, `project`, `store`) — for `--why`.
        rung: &'static str,
    },
    /// No rung answered; `why` says what was looked at and how to fix it.
    Missing { why: String },
}

/// The inputs the ladder reads, gathered by the caller so the ladder itself is
/// testable without an environment, a checkout or a store.
pub struct Ctx<'a> {
    /// Environment lookup (rung 1).
    pub getenv: &'a dyn Fn(&str) -> Option<OsString>,
    /// The checkout's build root when the SDK root is a checkout (rung 2).
    pub build_root: Option<PathBuf>,
    /// The version `[tool.qemu]` pins, when an index was readable.
    pub pin: Option<String>,
    /// The store's pinned qemu ROOT (`sdk_store::tool_dir`), rung 3.
    pub store_dir: Option<PathBuf>,
}

impl Ctx<'static> {
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
            build_root: checkout,
            pin: index
                .as_ref()
                .and_then(|i| i.tool.get("qemu").map(|t| t.version.clone())),
            store_dir: index
                .as_ref()
                .and_then(|i| crate::orchestration::sdk_store::tool_dir(i, "qemu")),
        }
    }
}

fn real_env(k: &str) -> Option<OsString> {
    std::env::var_os(k)
}

/// `QEMU_SYSTEM_ARM` for `qemu-system-arm` — the twins' variable name.
#[must_use]
pub fn env_var(exe: &str) -> String {
    let arch = exe.strip_prefix("qemu-system-").unwrap_or(exe);
    format!(
        "QEMU_SYSTEM_{}",
        arch.to_ascii_uppercase().replace('-', "_")
    )
}

/// Resolve `exe` (`qemu-system-arm`, `qemu-system-riscv64`, …).
#[must_use]
pub fn locate(exe: &str, ctx: &Ctx<'_>) -> QemuBin {
    let var = env_var(exe);
    if let Some(v) = (ctx.getenv)(&var).filter(|v| !v.is_empty()) {
        return QemuBin::Found {
            path: PathBuf::from(v),
            rung: "env",
        };
    }
    let mut stale = None;
    if let Some(root) = &ctx.build_root {
        let prefix = root.join("qemu");
        let cand = prefix.join("bin").join(exe);
        if cand.is_file() {
            match prefix_matches_pin(&prefix, ctx.pin.as_deref()) {
                Ok(()) => {
                    return QemuBin::Found {
                        path: cand,
                        rung: "project",
                    };
                }
                Err(why) => {
                    stale = Some(format!("{} is not the pinned QEMU: {why}", cand.display()))
                }
            }
        }
    }
    if let Some(dir) = &ctx.store_dir {
        let cand = dir.join("bin").join(exe);
        if cand.is_file() {
            return QemuBin::Found {
                path: cand,
                rung: "store",
            };
        }
    }
    let mut why = format!(
        "no {exe} from ${var}, the project prefix or the store{} — provision it with \
         `nros setup --tool qemu`",
        ctx.pin
            .as_deref()
            .map(|p| format!(" (pinned {p})"))
            .unwrap_or_default()
    );
    if let Some(s) = stale {
        why = format!("{s}; {why}");
    }
    QemuBin::Missing { why }
}

/// Issue 0930 — the project prefix is ONE directory across pin bumps, so it
/// counts only when its provenance marker names the pinned version. With no
/// pin to compare against, it is taken as chosen (the twins' rule).
fn prefix_matches_pin(prefix: &Path, pin: Option<&str>) -> Result<(), String> {
    let Some(pin) = pin else {
        return Ok(());
    };
    let marker = prefix.join(".nros-provenance");
    let Ok(text) = std::fs::read_to_string(&marker) else {
        return Err(format!(
            "it has no {}; the index pins {pin}",
            marker.display()
        ));
    };
    let held = text.lines().find_map(|l| {
        let (k, v) = l.split_once('=')?;
        (k.trim() == "version").then(|| v.trim().trim_matches('"').to_string())
    });
    match held {
        Some(v) if v == pin => Ok(()),
        Some(v) => Err(format!("it holds {v}; the index pins {pin}")),
        None => Err(format!("{} names no version", marker.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("nros-qemu-bin-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn touch(p: &Path) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, "").unwrap();
    }

    #[test]
    fn the_ladder_is_env_then_current_prefix_then_store_and_never_path() {
        let d = tmp("ladder");
        let store = d.join("store/qemu/11.0");
        touch(&store.join("bin/qemu-system-arm"));
        let none = |_: &str| None;
        let ctx = Ctx {
            getenv: &none,
            build_root: Some(d.join("build")),
            pin: Some("11.0".into()),
            store_dir: Some(store.clone()),
        };
        // No prefix: the store answers.
        assert_eq!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Found {
                path: store.join("bin/qemu-system-arm"),
                rung: "store"
            }
        );
        // A STALE prefix is skipped, not taken.
        touch(&d.join("build/qemu/bin/qemu-system-arm"));
        std::fs::write(d.join("build/qemu/.nros-provenance"), "version = \"9.0\"\n").unwrap();
        assert!(matches!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Found { rung: "store", .. }
        ));
        // A current one wins over the store.
        std::fs::write(
            d.join("build/qemu/.nros-provenance"),
            "version = \"11.0\"\n",
        )
        .unwrap();
        assert!(matches!(
            locate("qemu-system-arm", &ctx),
            QemuBin::Found {
                rung: "project",
                ..
            }
        ));
        // The env override beats both.
        let env = |k: &str| (k == "QEMU_SYSTEM_ARM").then(|| OsString::from("/opt/q"));
        let ctx_env = Ctx {
            getenv: &env,
            ..ctx
        };
        assert_eq!(
            locate("qemu-system-arm", &ctx_env),
            QemuBin::Found {
                path: "/opt/q".into(),
                rung: "env"
            }
        );
        // Nothing provisioned for riscv64: missing, naming the remedy — never PATH.
        match locate("qemu-system-riscv64", &ctx_env) {
            QemuBin::Missing { why } => assert!(why.contains("nros setup --tool qemu"), "{why}"),
            other => panic!("{other:?}"),
        }
        let _ = std::fs::remove_dir_all(&d);
    }
}
