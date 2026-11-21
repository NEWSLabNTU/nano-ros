//! The Arm FVP (`FVP_BaseR_AEMv8R`) is USER-PROVIDED (issue 2802).
//!
//! nano-ros does not download, install or distribute the model. The user
//! installs the AEMv8-R FVP from Arm and points nano-ros at it. This module
//! is the one Rust answer to "where is it, and which setting said so", read by
//! `nros doctor` and by `nros setup board` for a board whose emulator is an
//! FVP. It mirrors `scripts/zephyr/resolve-fvp-bin.sh`, the answer the just
//! recipes and the FVP tests read, in the same order:
//!
//! 1. `ARMFVP_BIN_PATH`, the directory holding the binary (Zephyr's own
//!    `cmake/emu/armfvp.cmake` reads it). Set but wrong is a miss: the script
//!    exits non-zero there rather than fall through, and so does this.
//! 2. `ARM_FVP_DIR`, an install root; the binary is looked for at the root and
//!    under `models/Linux64_GCC-*` and `bin/` (and the `Base_RevC_AEMv8R_pkg/`
//!    variants of both).
//! 3. `PATH`.
//!
//! Nothing here downloads, and nothing here fails a command: a missing FVP is
//! reported with [`install_hint`], never as an error.

use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

/// The model's executable name.
pub const FVP_BIN: &str = "FVP_BaseR_AEMv8R";

/// Where Arm publishes the FVPs.
pub const FVP_DOWNLOAD_URL: &str = "https://developer.arm.com/downloads/-/arm-ecosystem-fvps";

/// Which setting located the model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FvpSource {
    ArmfvpBinPath,
    ArmFvpDir,
    Path,
}

impl FvpSource {
    /// The setting's name as the user writes it.
    #[must_use]
    pub fn setting(self) -> &'static str {
        match self {
            Self::ArmfvpBinPath => "ARMFVP_BIN_PATH",
            Self::ArmFvpDir => "ARM_FVP_DIR",
            Self::Path => "PATH",
        }
    }
}

/// The three settings the lookup reads. A value, not the process
/// environment, so a test passes its own and races nothing.
#[derive(Debug, Clone, Default)]
pub struct FvpEnv {
    pub armfvp_bin_path: Option<OsString>,
    pub arm_fvp_dir: Option<OsString>,
    pub path: Option<OsString>,
}

impl FvpEnv {
    /// This process's settings.
    #[must_use]
    pub fn from_process() -> Self {
        Self {
            armfvp_bin_path: non_empty(std::env::var_os("ARMFVP_BIN_PATH")),
            arm_fvp_dir: non_empty(std::env::var_os("ARM_FVP_DIR")),
            path: std::env::var_os("PATH"),
        }
    }
}

fn non_empty(v: Option<OsString>) -> Option<OsString> {
    v.filter(|v| !v.is_empty())
}

/// Where the model was found: the binary, and the setting that led there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FvpHit {
    pub source: FvpSource,
    pub bin: PathBuf,
}

/// Locate the model through `env`. `Err` carries why it was not found: a
/// setting that points nowhere is named, so the user fixes that one.
pub fn locate(env: &FvpEnv) -> Result<FvpHit, String> {
    if let Some(v) = &env.armfvp_bin_path {
        let dir = PathBuf::from(v);
        let bin = dir.join(FVP_BIN);
        return if is_executable(&bin) {
            Ok(FvpHit {
                source: FvpSource::ArmfvpBinPath,
                bin,
            })
        } else {
            Err(format!(
                "ARMFVP_BIN_PATH={} does not contain {FVP_BIN}",
                dir.display()
            ))
        };
    }
    if let Some(v) = &env.arm_fvp_dir {
        let root = PathBuf::from(v);
        return find_under(&root)
            .map(|bin| FvpHit {
                source: FvpSource::ArmFvpDir,
                bin,
            })
            .ok_or_else(|| {
                format!(
                    "{FVP_BIN} not found under ARM_FVP_DIR={} (looked at the root, \
                     models/Linux64_GCC-*/ and bin/)",
                    root.display()
                )
            });
    }
    if let Some(path) = &env.path {
        for dir in std::env::split_paths(path) {
            let bin = dir.join(FVP_BIN);
            if is_executable(&bin) {
                return Ok(FvpHit {
                    source: FvpSource::Path,
                    bin,
                });
            }
        }
    }
    Err(format!(
        "{FVP_BIN} not found (ARMFVP_BIN_PATH and ARM_FVP_DIR unset, not on PATH)"
    ))
}

/// The binary under an install root, in the layouts Arm ships.
fn find_under(root: &Path) -> Option<PathBuf> {
    let direct = root.join(FVP_BIN);
    if is_executable(&direct) {
        return Some(direct);
    }
    for sub in ["models", "Base_RevC_AEMv8R_pkg/models"] {
        if let Ok(rd) = std::fs::read_dir(root.join(sub)) {
            let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
            dirs.sort();
            for d in dirs {
                let is_gcc = d
                    .file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("Linux64_GCC-"));
                let bin = d.join(FVP_BIN);
                if is_gcc && is_executable(&bin) {
                    return Some(bin);
                }
            }
        }
    }
    for sub in ["bin", "Base_RevC_AEMv8R_pkg/bin"] {
        let bin = root.join(sub).join(FVP_BIN);
        if is_executable(&bin) {
            return Some(bin);
        }
    }
    None
}

/// Whether a board descriptor launches on an FVP (`[board.run] emulator =
/// "fvp:<model>"`).
#[must_use]
pub fn descriptor_runs_on_fvp(d: &crate::orchestration::board_descriptor::BoardDescriptor) -> bool {
    d.run
        .as_ref()
        .and_then(|r| r.emulator_parts())
        .is_some_and(|(kind, _)| kind == "fvp")
}

/// Whether board `name`'s descriptor launches on an FVP (`[board.run]
/// emulator = "fvp:<model>"`). An unresolvable catalog or board answers no.
pub fn board_runs_on_fvp(workspace: Option<&Path>, name: &str) -> bool {
    let Ok(root) = crate::cmd::board::catalog_root(workspace.map(Path::to_path_buf)) else {
        return false;
    };
    let Ok(catalog) = crate::orchestration::board_descriptor::BoardCatalog::load(&root) else {
        return false;
    };
    catalog
        .descriptors()
        .iter()
        .find(|d| d.answers_to(name))
        .is_some_and(descriptor_runs_on_fvp)
}

/// What to do when the model is missing. Names the runtime dependency the
/// model's libraries link (`libatomic.so.1`), which a fresh host often lacks.
#[must_use]
pub fn install_hint() -> String {
    format!(
        "the Arm FVP is user-provided: nano-ros does not download or distribute it. \
         Install the AEMv8-R FVP (FVP_Base_AEMv8R, x86_64 Linux) from Arm \
         ({FVP_DOWNLOAD_URL}), then set ARMFVP_BIN_PATH to the directory holding \
         {FVP_BIN} (or ARM_FVP_DIR to the install root). The model needs \
         libatomic.so.1 at runtime (apt: libatomic1)."
    )
}

/// The one-line status `nros doctor` and `nros setup board` print.
#[must_use]
pub fn status_line(found: &Result<FvpHit, String>) -> String {
    match found {
        Ok(hit) => format!(
            "[OK] arm-fvp: {} found via {} ({})",
            FVP_BIN,
            hit.source.setting(),
            hit.bin.display()
        ),
        Err(why) => format!("[WARN] arm-fvp: {why}.\n         Fix: {}", install_hint()),
    }
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.is_file()
        && std::fs::metadata(path)
            .map(|m| m.permissions().mode() & 0o111 != 0)
            .unwrap_or(false)
}

#[cfg(not(unix))]
fn is_executable(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_model(dir: &Path) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let bin = dir.join(FVP_BIN);
        crate::test_support::write_executable_stub(&bin, "#!/bin/sh\nexit 0\n");
        bin
    }

    /// Issue 2802 -- the status names the setting that found the model, for
    /// each of the three, in the resolver's order.
    #[test]
    fn the_status_names_the_setting_that_found_the_model() {
        let tmp = tempfile::tempdir().unwrap();
        let bin_dir = tmp.path().join("fvp/bin");
        let bin = fake_model(&bin_dir);
        let models = tmp.path().join("root/models/Linux64_GCC-9.3");
        let under_root = fake_model(&models);

        let env = FvpEnv {
            armfvp_bin_path: Some(bin_dir.clone().into()),
            arm_fvp_dir: Some(tmp.path().join("root").into()),
            path: Some(bin_dir.clone().into()),
        };
        let hit = locate(&env).unwrap();
        assert_eq!(
            hit,
            FvpHit {
                source: FvpSource::ArmfvpBinPath,
                bin: bin.clone()
            }
        );
        assert!(status_line(&Ok(hit)).contains("via ARMFVP_BIN_PATH"));

        let env = FvpEnv {
            armfvp_bin_path: None,
            ..env
        };
        let hit = locate(&env).unwrap();
        assert_eq!(
            hit,
            FvpHit {
                source: FvpSource::ArmFvpDir,
                bin: under_root
            }
        );
        assert!(status_line(&Ok(hit)).contains("via ARM_FVP_DIR"));

        let env = FvpEnv {
            arm_fvp_dir: None,
            ..env
        };
        let hit = locate(&env).unwrap();
        assert_eq!(
            hit,
            FvpHit {
                source: FvpSource::Path,
                bin
            }
        );
        assert!(status_line(&Ok(hit)).contains("via PATH"));
    }

    /// A missing model is a WARN naming the user install and libatomic1, and
    /// a setting that points nowhere is named rather than skipped.
    #[test]
    fn a_missing_model_warns_with_the_user_install_hint() {
        let tmp = tempfile::tempdir().unwrap();
        let none = locate(&FvpEnv {
            path: Some(tmp.path().into()),
            ..FvpEnv::default()
        });
        let line = status_line(&none);
        assert!(line.starts_with("[WARN]"), "{line}");
        for needle in [
            "user-provided",
            "ARMFVP_BIN_PATH",
            "ARM_FVP_DIR",
            "libatomic1",
        ] {
            assert!(line.contains(needle), "`{needle}` missing: {line}");
        }
        assert!(!line.contains("nros setup --tool"), "{line}");

        // Set but wrong does not fall through to PATH, as in the script.
        let on_path = tmp.path().join("p");
        fake_model(&on_path);
        let wrong = locate(&FvpEnv {
            armfvp_bin_path: Some(tmp.path().join("nope").into()),
            arm_fvp_dir: None,
            path: Some(on_path.into()),
        })
        .unwrap_err();
        assert!(wrong.contains("ARMFVP_BIN_PATH="), "{wrong}");
    }
}
