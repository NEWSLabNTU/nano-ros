//! Phase 187.3 — the install store, provenance, lockfile, and fetch/source-build
//! engine behind `nros setup`.
//!
//! A tool always lands at the same versioned prefix
//! `$NROS_HOME/sdk/<tool>/<version>/`, whether fetched (prebuilt `dist`) or
//! source-built — downstream resolves the prefix, provenance-agnostic
//! (`.nros-provenance` records which). The store is shared across workspaces;
//! `nros-sdk.lock` pins what's installed. See
//! `docs/design/0014-nros-setup-toolchain-management.md`.

use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::Command,
};

use eyre::{Result, WrapErr, bail, eyre};
use serde::{Deserialize, Serialize};

use super::sdk_index::{SourcePackage, SourceProvision, ToolPackage};

/// The lockfile name (written in cwd by `nros setup` / auto-setup — pins the
/// installed toolset for the workspace it's run in). Single source for the name.
pub const LOCK_FILE: &str = "nros-sdk.lock";

/// The shared SDK store root: `$NROS_HOME/sdk`, else `~/.nros/sdk`, else
/// `./.nros/sdk`.
pub fn store_root() -> PathBuf {
    if let Some(h) = std::env::var_os("NROS_HOME") {
        return PathBuf::from(h).join("sdk");
    }
    if let Some(h) = std::env::var_os("HOME") {
        return PathBuf::from(h).join(".nros").join("sdk");
    }
    PathBuf::from(".nros").join("sdk")
}

/// The versioned install prefix for a tool — identical for prebuilt + source.
pub fn tool_prefix(root: &Path, tool: &str, version: &str) -> PathBuf {
    root.join(tool).join(version)
}

/// How a tool was installed; persisted to `<prefix>/.nros-provenance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceKind {
    Prebuilt,
    Source,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub kind: ProvenanceKind,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl Provenance {
    fn marker(prefix: &Path) -> PathBuf {
        prefix.join(".nros-provenance")
    }

    /// Read the provenance marker, if the prefix is populated.
    pub fn read(prefix: &Path) -> Option<Self> {
        let raw = std::fs::read_to_string(Self::marker(prefix)).ok()?;
        toml::from_str(&raw).ok()
    }

    pub fn write(&self, prefix: &Path) -> Result<()> {
        std::fs::create_dir_all(prefix)
            .wrap_err_with(|| format!("create prefix {}", prefix.display()))?;
        std::fs::write(Self::marker(prefix), toml::to_string(self)?)
            .wrap_err("write .nros-provenance")?;
        Ok(())
    }
}

/// `nros-sdk.lock` — the exact installed toolset, for reproducibility.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SdkLock {
    #[serde(default)]
    pub tool: BTreeMap<String, LockEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub version: String,
    pub provenance: ProvenanceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

impl SdkLock {
    /// Load the lockfile; a missing file is an empty lock (not an error).
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(raw) => toml::from_str(&raw).wrap_err("invalid nros-sdk.lock"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).wrap_err_with(|| format!("read {}", path.display())),
        }
    }

    pub fn record(&mut self, tool: &str, p: &Provenance) {
        self.tool.insert(
            tool.to_string(),
            LockEntry {
                version: p.version.clone(),
                provenance: p.kind,
                sha256: p.sha256.clone(),
            },
        );
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        std::fs::write(path, toml::to_string_pretty(self)?)
            .wrap_err_with(|| format!("write {}", path.display()))
    }
}

/// The decided install action for a tool on a host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallAction {
    /// Already at the prefix (idempotent skip).
    Present,
    /// Fetch + verify + unpack the prebuilt artifact.
    Prebuilt { url: String, sha256: String },
    /// Build from source into the same prefix.
    Source {
        git: String,
        git_ref: String,
        configure: Option<String>,
        install: Option<String>,
    },
    /// No prebuilt for this host and no source recipe.
    Unavailable,
}

/// Decide how to install `tool` on `host`, given the prefix's current state.
/// Pure — does no I/O beyond reading the provenance marker.
pub fn plan_install(tool: &ToolPackage, host: &str, prefix: &Path) -> InstallAction {
    if Provenance::read(prefix).is_some() {
        return InstallAction::Present;
    }
    if let Some(d) = tool.dist_for(host) {
        return InstallAction::Prebuilt {
            url: d.url.clone(),
            sha256: d.sha256.clone(),
        };
    }
    if let Some(s) = &tool.source {
        return InstallAction::Source {
            git: s.git.clone(),
            git_ref: s.git_ref.clone(),
            configure: s.configure.clone(),
            install: s.install.clone(),
        };
    }
    InstallAction::Unavailable
}

/// Execute an install action into `prefix`, returning the recorded provenance.
/// Side-effecting (shells out to curl / sha256sum / tar / git); real-run only.
pub fn execute(
    action: &InstallAction,
    tool: &str,
    version: &str,
    prefix: &Path,
) -> Result<Provenance> {
    match action {
        InstallAction::Present => Provenance::read(prefix)
            .ok_or_else(|| eyre!("{tool}: present but no provenance marker")),
        InstallAction::Prebuilt { url, sha256 } => {
            std::fs::create_dir_all(prefix)
                .wrap_err_with(|| format!("create {}", prefix.display()))?;
            let archive = prefix.with_extension("download");
            sh(
                &[
                    "curl",
                    "-L",
                    "--fail",
                    "--silent",
                    "--show-error",
                    "-o",
                    &archive.to_string_lossy(),
                    url,
                ],
                None,
            )
            .wrap_err_with(|| format!("download {url}"))?;
            verify_sha256(&archive, sha256)?;
            sh(
                &[
                    "tar",
                    "-xf",
                    &archive.to_string_lossy(),
                    "-C",
                    &prefix.to_string_lossy(),
                ],
                None,
            )
            .wrap_err("unpack prebuilt archive")?;
            let _ = std::fs::remove_file(&archive);
            let p = Provenance {
                kind: ProvenanceKind::Prebuilt,
                version: version.to_string(),
                sha256: Some(sha256.clone()),
            };
            p.write(prefix)?;
            Ok(p)
        }
        InstallAction::Source {
            git,
            git_ref,
            configure,
            install,
        } => {
            let src = prefix.with_extension("src");
            let _ = std::fs::remove_dir_all(&src);
            sh(
                &[
                    "git",
                    "clone",
                    "--depth",
                    "1",
                    "--branch",
                    git_ref,
                    git,
                    &src.to_string_lossy(),
                ],
                None,
            )
            .wrap_err_with(|| format!("git clone {git} @ {git_ref}"))?;
            let prefix_abs = prefix.to_string_lossy().to_string();
            if let Some(cfg) = configure {
                sh(
                    &["sh", "-c", &cfg.replace("{prefix}", &prefix_abs)],
                    Some(&src),
                )
                .wrap_err("configure step")?;
            }
            if let Some(inst) = install {
                sh(
                    &["sh", "-c", &inst.replace("{prefix}", &prefix_abs)],
                    Some(&src),
                )
                .wrap_err("install step")?;
            }
            let p = Provenance {
                kind: ProvenanceKind::Source,
                version: version.to_string(),
                sha256: None,
            };
            p.write(prefix)?;
            Ok(p)
        }
        InstallAction::Unavailable => {
            bail!("{tool} {version}: no prebuilt for this host and no source recipe in the index")
        }
    }
}

/// Verify `path`'s sha256 equals `expected` (shells out to `sha256sum`, falling
/// back to `shasum -a 256` on macOS).
fn verify_sha256(path: &Path, expected: &str) -> Result<()> {
    let out = Command::new("sha256sum")
        .arg(path)
        .output()
        .or_else(|_| {
            Command::new("shasum")
                .args(["-a", "256"])
                .arg(path)
                .output()
        })
        .wrap_err("run sha256sum / shasum")?;
    if !out.status.success() {
        bail!("sha256sum failed for {}", path.display());
    }
    let got = String::from_utf8_lossy(&out.stdout);
    let got = got.split_whitespace().next().unwrap_or("");
    if !got.eq_ignore_ascii_case(expected) {
        bail!(
            "sha256 mismatch for {}: expected {expected}, got {got}",
            path.display()
        );
    }
    Ok(())
}

/// Outcome of [`provision_source`] — for the `nros setup` disposition line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SourceDisposition {
    /// Fetched into `dest` (clone or submodule update).
    Provisioned,
    /// `dest` already present — left untouched (idempotent skip).
    AlreadyPresent,
    /// No fetch step declared (version-only `[source.*]`).
    NoFetch,
    /// `--dry-run`: what would have happened.
    Planned,
}

/// Phase 195.B — provision a `[source.*]` package into its workspace-relative
/// `dest` from index data (never a path baked into the binary). Idempotent: an
/// already-present `dest` is left untouched. Two modes (see
/// [`SourcePackage::provision`]):
///
/// Both modes honor the source's `shallow` (default true → `--depth 1`,
/// fetch-by-SHA so a lagging pin is still a real depth-1 checkout) and
/// `recursive` (default true → descend the source's own nested submodules).
///
/// - **Clone:** shallow → `git init` + `git fetch --depth 1 origin <ref>` +
///   `git checkout FETCH_HEAD` (`ref` may be a sha, so `--branch` can't be
///   used); full → `git clone <git> <dest>` + `git checkout <ref>`. Then, if
///   recursive, `git submodule update --init --recursive [--depth 1]`.
/// - **Submodule:** `git -C <workspace> submodule update --init [--recursive]
///   [--depth 1] -- <submodule>` (inherently idempotent; checks out the
///   superproject's recorded commit, kept in lockstep with the index ref via
///   the SSOT rule).
pub fn provision_source(
    name: &str,
    src: &SourcePackage,
    workspace: &Path,
    dry_run: bool,
    shallow_override: Option<bool>,
) -> Result<SourceDisposition> {
    // `--full` / `--shallow` (per-invocation) wins over the index `shallow`.
    let shallow = shallow_override.unwrap_or(src.shallow);
    match src.provision() {
        SourceProvision::None => Ok(SourceDisposition::NoFetch),
        SourceProvision::Submodule => {
            let path = src.submodule.as_deref().expect("submodule mode has a path");
            if dry_run {
                return Ok(SourceDisposition::Planned);
            }
            // Fast path: `git submodule update --init [--recursive] [--depth 1]`.
            // CAVEAT: `--depth 1` shallow-fetches the submodule's BRANCH TIP, not
            // the pinned gitlink SHA — so a pin that lags its tip and isn't an
            // advertised ref (e.g. PX4-Autopilot's 1.15.x sha vs `main`) fails the
            // checkout here. We catch that and fall back to an explicit
            // depth-1 fetch-by-SHA (GitHub serves reachable SHAs), which `git
            // clone --branch` / `submodule update` can't express directly.
            let workspace_s = workspace.to_string_lossy();
            let mut args: Vec<&str> =
                vec!["git", "-C", &workspace_s, "submodule", "update", "--init"];
            if src.recursive {
                args.push("--recursive");
            }
            if shallow {
                args.push("--depth");
                args.push("1");
            }
            args.push("--");
            args.push(path);
            let fast = sh(&args, None);
            if let Err(e) = fast {
                if !shallow {
                    return Err(e)
                        .wrap_err_with(|| format!("git submodule update {path} (source {name})"));
                }
                // By-SHA fallback. `submodule update` already initialised the
                // gitdir + worktree (only its checkout of the lagging pin failed),
                // so fetch the exact gitlink SHA at depth 1 and check it out, then
                // descend if recursive.
                let sha = sh_capture(&["git", "-C", &workspace_s, "ls-tree", "HEAD", path], None)
                    .wrap_err_with(|| format!("read gitlink sha for {path} (source {name})"))?
                    .split_whitespace()
                    .nth(2)
                    .map(str::to_owned)
                    .ok_or_else(|| eyre!("no gitlink sha for {path} (source {name})"))?;
                let subdir = workspace.join(path);
                let subdir_s = subdir.to_string_lossy();
                sh(
                    &[
                        "git", "-C", &subdir_s, "fetch", "--depth", "1", "origin", &sha,
                    ],
                    None,
                )
                .wrap_err_with(|| format!("git fetch --depth 1 {sha} ({path}, source {name})"))?;
                sh(&["git", "-C", &subdir_s, "checkout", "-q", &sha], None)
                    .wrap_err_with(|| format!("git checkout {sha} ({path}, source {name})"))?;
                if src.recursive {
                    sh(
                        &[
                            "git",
                            "-C",
                            &subdir_s,
                            "submodule",
                            "update",
                            "--init",
                            "--recursive",
                            "--depth",
                            "1",
                            "--recommend-shallow",
                        ],
                        None,
                    )
                    .wrap_err_with(|| {
                        format!("git submodule update --recursive ({path}, source {name})")
                    })?;
                }
            }
            Ok(SourceDisposition::Provisioned)
        }
        SourceProvision::Clone => {
            let git = src.git.as_deref().expect("clone mode has a git url");
            let git_ref = src.git_ref.as_deref().expect("clone mode has a ref");
            let dest = src.dest.as_deref().expect("clone mode has a dest");
            let dest_abs = workspace.join(dest);
            // Idempotent: a present, non-empty dest is left as-is (don't clobber
            // a contributor checkout / in-progress work). `nros setup` on a
            // fresh tree provisions; a populated tree is a skip.
            let present = dest_abs
                .read_dir()
                .map(|mut d| d.next().is_some())
                .unwrap_or(false);
            if present {
                return Ok(SourceDisposition::AlreadyPresent);
            }
            if dry_run {
                return Ok(SourceDisposition::Planned);
            }
            if let Some(parent) = dest_abs.parent() {
                std::fs::create_dir_all(parent)
                    .wrap_err_with(|| format!("create source dest parent {}", parent.display()))?;
            }
            let dest_str = dest_abs.to_string_lossy();
            if shallow {
                // Shallow clone of a possibly-SHA `ref`: `git clone --branch`
                // can't take a sha, so init + fetch-by-ref at depth 1 (works for
                // sha/tag/branch via the server's reachable-SHA support) + check
                // out the fetched commit (detached, same as a sha checkout).
                sh(&["git", "init", "-q", &dest_str], None)
                    .wrap_err_with(|| format!("git init {dest_str} (source {name})"))?;
                sh(
                    &["git", "-C", &dest_str, "remote", "add", "origin", git],
                    None,
                )
                .wrap_err_with(|| format!("git remote add (source {name})"))?;
                sh(
                    &[
                        "git", "-C", &dest_str, "fetch", "-q", "--depth", "1", "origin", git_ref,
                    ],
                    None,
                )
                .wrap_err_with(|| format!("git fetch --depth 1 {git_ref} (source {name})"))?;
                sh(
                    &["git", "-C", &dest_str, "checkout", "-q", "FETCH_HEAD"],
                    None,
                )
                .wrap_err_with(|| format!("git checkout FETCH_HEAD (source {name})"))?;
            } else {
                sh(&["git", "clone", git, &dest_str], None)
                    .wrap_err_with(|| format!("git clone {git} (source {name})"))?;
                sh(&["git", "-C", &dest_str, "checkout", git_ref], None)
                    .wrap_err_with(|| format!("git checkout {git_ref} (source {name})"))?;
            }
            if src.recursive {
                // A cloned source may carry its own nested submodules.
                let mut sub: Vec<&str> = vec![
                    "git",
                    "-C",
                    &dest_str,
                    "submodule",
                    "update",
                    "--init",
                    "--recursive",
                ];
                if shallow {
                    sub.push("--depth");
                    sub.push("1");
                }
                sh(&sub, None).wrap_err_with(|| {
                    format!("git submodule update --recursive (source {name})")
                })?;
            }
            Ok(SourceDisposition::Provisioned)
        }
    }
}

fn sh(args: &[&str], cwd: Option<&Path>) -> Result<()> {
    let (cmd, rest) = args.split_first().ok_or_else(|| eyre!("empty command"))?;
    let mut c = Command::new(cmd);
    c.args(rest);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let status = c.status().wrap_err_with(|| format!("spawn {cmd}"))?;
    if !status.success() {
        bail!("`{}` failed ({status})", args.join(" "));
    }
    Ok(())
}

/// Run a command and capture trimmed stdout (for reading a gitlink SHA, etc.).
fn sh_capture(args: &[&str], cwd: Option<&Path>) -> Result<String> {
    let (cmd, rest) = args.split_first().ok_or_else(|| eyre!("empty command"))?;
    let mut c = Command::new(cmd);
    c.args(rest);
    if let Some(d) = cwd {
        c.current_dir(d);
    }
    let out = c.output().wrap_err_with(|| format!("spawn {cmd}"))?;
    if !out.status.success() {
        bail!("`{}` failed ({})", args.join(" "), out.status);
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::sdk_index::SdkIndex;

    fn tmp(tag: &str) -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("nros_store_{tag}_{n}"))
    }

    #[test]
    fn prefix_layout_is_tool_then_version() {
        let p = tool_prefix(Path::new("/store"), "qemu", "11.0-nros1");
        assert_eq!(p, Path::new("/store/qemu/11.0-nros1"));
    }

    #[test]
    fn provenance_roundtrips_and_marks_present() {
        let prefix = tmp("prov");
        assert!(Provenance::read(&prefix).is_none());
        let p = Provenance {
            kind: ProvenanceKind::Prebuilt,
            version: "11.0".into(),
            sha256: Some("abc".into()),
        };
        p.write(&prefix).unwrap();
        assert_eq!(Provenance::read(&prefix).as_ref(), Some(&p));
        std::fs::remove_dir_all(&prefix).ok();
    }

    #[test]
    fn lockfile_records_and_roundtrips() {
        let path = tmp("lock").join("nros-sdk.lock");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        assert!(SdkLock::load(&path).unwrap().tool.is_empty()); // missing ⇒ empty
        let mut lock = SdkLock::default();
        lock.record(
            "qemu",
            &Provenance {
                kind: ProvenanceKind::Source,
                version: "11.0".into(),
                sha256: None,
            },
        );
        lock.save(&path).unwrap();
        let back = SdkLock::load(&path).unwrap();
        assert_eq!(back.tool["qemu"].version, "11.0");
        assert_eq!(back.tool["qemu"].provenance, ProvenanceKind::Source);
        std::fs::remove_dir_all(path.parent().unwrap()).ok();
    }

    /// Verifies SDK planning prefers present tools, then prebuilt, then source, then unavailable.
    #[test]
    fn plan_picks_present_prebuilt_source() {
        let idx = SdkIndex::parse(
            "[tool.qemu]\nversion=\"11.0\"\ndist.linux-x86_64={url=\"u\",sha256=\"h\"}\n\
             [tool.qemu.source]\ngit=\"g\"\nref=\"r\"\n\
             [tool.bare]\nversion=\"1\"\n",
        )
        .unwrap();
        let qemu = &idx.tool["qemu"];
        let bare = &idx.tool["bare"];
        let fresh = tmp("plan-fresh");

        // prebuilt host → Prebuilt; non-prebuilt host with source → Source.
        assert!(matches!(
            plan_install(qemu, "linux-x86_64", &fresh),
            InstallAction::Prebuilt { .. }
        ));
        assert!(matches!(
            plan_install(qemu, "macos-arm64", &fresh),
            InstallAction::Source { .. }
        ));
        // no dist + no source → Unavailable.
        assert_eq!(
            plan_install(bare, "macos-arm64", &fresh),
            InstallAction::Unavailable
        );

        // a populated prefix → Present (idempotent skip).
        let present = tmp("plan-present");
        Provenance {
            kind: ProvenanceKind::Prebuilt,
            version: "11.0".into(),
            sha256: None,
        }
        .write(&present)
        .unwrap();
        assert_eq!(
            plan_install(qemu, "linux-x86_64", &present),
            InstallAction::Present
        );
        std::fs::remove_dir_all(&present).ok();
    }

    #[test]
    fn provision_source_no_fetch_present_and_dry_run() {
        // Version-only source → no fetch step.
        let none = SourcePackage {
            version: "1".into(),
            ..Default::default()
        };
        assert_eq!(
            provision_source("x", &none, Path::new("/ws"), false, None).unwrap(),
            SourceDisposition::NoFetch
        );

        // Clone mode, dest already populated → AlreadyPresent (no git run).
        let ws = tmp("prov-src");
        let dest_rel = "third-party/lwip";
        let dest_abs = ws.join(dest_rel);
        std::fs::create_dir_all(&dest_abs).unwrap();
        std::fs::write(dest_abs.join("CMakeLists.txt"), "x").unwrap();
        let clone = SourcePackage {
            version: "2.2.0".into(),
            git: Some("https://example/lwip.git".into()),
            git_ref: Some("STABLE-2_2_0".into()),
            dest: Some(dest_rel.into()),
            submodule: None,
            shallow: true,
            recursive: true,
        };
        assert_eq!(
            provision_source("lwip", &clone, &ws, false, None).unwrap(),
            SourceDisposition::AlreadyPresent
        );

        // Clone mode, empty dest, dry-run → Planned (no git run).
        let empty = SourcePackage {
            dest: Some("third-party/empty".into()),
            ..clone.clone()
        };
        assert_eq!(
            provision_source("lwip", &empty, &ws, true, None).unwrap(),
            SourceDisposition::Planned
        );
        std::fs::remove_dir_all(&ws).ok();
    }
}
