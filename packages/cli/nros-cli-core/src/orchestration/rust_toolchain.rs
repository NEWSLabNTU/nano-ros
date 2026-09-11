//! Issue 1304 — the Rust toolchain a BUILD needs, provisioned by `nros setup`.
//!
//! Every board needs one, whatever language the user writes: the runtime a
//! project links (`nros-c` / `nros-cpp`) is compiled out of the SDK root by
//! Corrosion at configure time. A contributor gets rustup from
//! `scripts/bootstrap.sh`. An INSTALLED toolchain had nothing — the book
//! advertises the release as "no checkout, no cargo" — so the first build
//! stopped in Corrosion's `FindRust` with `rustc` not found, after `nros setup`
//! had reported success.
//!
//! What counts as "has Rust" is what Corrosion counts, because Corrosion is the
//! consumer: `rustup` on PATH or in `~/.cargo/bin` (its `find_program(... PATHS
//! "$ENV{HOME}/.cargo/bin")`), else a bare `rustc` on PATH. So a toolchain this
//! installs is one the book's plain `cmake -S . -B build` finds, with no PATH
//! edit and no dotfile touched (`--no-modify-path`).
//!
//! A host that already has rustup keeps every choice it made. The only thing
//! done to one is the case it cannot build with: rustup present, no default
//! toolchain at all.

use std::{
    path::{Path, PathBuf},
    process::Command,
};

use eyre::{Result, WrapErr, bail};

use super::{
    sdk_index::SdkIndex,
    sdk_store::{sh, verify_sha256},
};

/// What this host has, found where Corrosion's `FindRust` looks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostRust {
    /// rustup, and the toolchain it would build with (`None`: it has none).
    Rustup {
        bin: PathBuf,
        default_toolchain: Option<String>,
    },
    /// `rustc` + `cargo` on PATH with no rustup — a distro toolchain, which
    /// FindRust accepts, so this does too.
    BareRustc,
    Absent,
}

/// What `nros setup` will do about it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RustPlan {
    Present(String),
    /// rustup with no default toolchain: install one and make it the default.
    SetDefault {
        rustup: PathBuf,
        channel: String,
    },
    /// No Rust at all: run the index's pinned `rustup-init`.
    Bootstrap {
        url: String,
        sha256: String,
        version: String,
        channel: String,
    },
    /// Nothing the index declares can help on this host.
    Unavailable(String),
}

/// The channel `[rust.rustup].toolchain` names, or why there is none.
fn default_channel(index: &SdkIndex) -> Result<String, String> {
    let r = index.rust.rustup.as_ref().ok_or_else(|| {
        "the SDK index declares no [rust.rustup], so there is no pinned rustup-init to run"
            .to_string()
    })?;
    index
        .rust
        .toolchain
        .get(&r.toolchain)
        .map(|t| t.channel.clone())
        .ok_or_else(|| {
            format!(
                "[rust.rustup] names toolchain '{}', which the index does not define",
                r.toolchain
            )
        })
}

/// Decide — pure, so every arm is testable without a host that lacks Rust.
#[must_use]
pub fn plan(found: &HostRust, index: &SdkIndex, host: &str) -> RustPlan {
    match found {
        HostRust::Rustup {
            default_toolchain: Some(tc),
            ..
        } => RustPlan::Present(format!("rustup, default {tc}")),
        HostRust::BareRustc => {
            RustPlan::Present("rustc + cargo on PATH (not rustup-managed)".to_string())
        }
        HostRust::Rustup {
            bin,
            default_toolchain: None,
        } => match default_channel(index) {
            Ok(channel) => RustPlan::SetDefault {
                rustup: bin.clone(),
                channel,
            },
            Err(why) => RustPlan::Unavailable(format!(
                "rustup at {} has no default toolchain, and {why}. Run `rustup default stable`.",
                bin.display()
            )),
        },
        HostRust::Absent => {
            let channel = match default_channel(index) {
                Ok(c) => c,
                Err(why) => {
                    return RustPlan::Unavailable(format!(
                        "no Rust toolchain on this host, and {why}. Install rustup: https://rustup.rs"
                    ));
                }
            };
            let r = index
                .rust
                .rustup
                .as_ref()
                .expect("default_channel checked it");
            match r.dist.get(host) {
                Some(d) => RustPlan::Bootstrap {
                    url: d.url.clone(),
                    sha256: d.sha256.clone(),
                    version: r.version.clone(),
                    channel,
                },
                None => RustPlan::Unavailable(format!(
                    "no Rust toolchain on this host, and [rust.rustup] has no dist for {host}. \
                     Install rustup: https://rustup.rs"
                )),
            }
        }
    }
}

/// The toolchain `rustup toolchain list` marks as the default.
///
/// Both spellings rustup has used: `stable-x86_64-unknown-linux-gnu (default)`
/// and, since 1.28, `… (active, default)`. `no installed toolchains` has none.
#[must_use]
pub fn parse_default(listing: &str) -> Option<String> {
    listing
        .lines()
        .find(|l| l.contains("default)"))
        .and_then(|l| l.split_whitespace().next())
        .map(str::to_string)
}

fn on_path(cmd: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join(cmd))
        .find(|p| p.is_file())
}

/// `$CARGO_HOME/bin` (where `rustup-init` installs when it is set) and
/// `~/.cargo/bin` (where FindRust looks, and where it installs otherwise).
fn cargo_bins() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(h) = std::env::var_os("CARGO_HOME").filter(|h| !h.is_empty()) {
        dirs.push(PathBuf::from(h).join("bin"));
    }
    if let Some(h) = std::env::var_os("HOME").filter(|h| !h.is_empty()) {
        dirs.push(PathBuf::from(h).join(".cargo").join("bin"));
    }
    dirs
}

fn find_rustup() -> Option<PathBuf> {
    on_path("rustup").or_else(|| {
        cargo_bins()
            .into_iter()
            .map(|d| d.join("rustup"))
            .find(|p| p.is_file())
    })
}

fn rustup_default(bin: &Path) -> Option<String> {
    // From `/`: `toolchain list` annotates an override, and a directory's
    // `rust-toolchain.toml` is not what this question is about.
    let out = Command::new(bin)
        .args(["toolchain", "list"])
        .current_dir("/")
        .env_remove("RUSTUP_TOOLCHAIN")
        .output()
        .ok()?;
    parse_default(&String::from_utf8_lossy(&out.stdout))
}

/// Look where Corrosion looks.
#[must_use]
pub fn probe_host() -> HostRust {
    if let Some(bin) = find_rustup() {
        let default_toolchain = rustup_default(&bin);
        return HostRust::Rustup {
            bin,
            default_toolchain,
        };
    }
    if on_path("rustc").is_some() && on_path("cargo").is_some() {
        return HostRust::BareRustc;
    }
    HostRust::Absent
}

/// The `nros setup` line, and whether anything was installed.
pub struct Outcome {
    pub line: String,
    pub changed: bool,
}

/// Make sure a build on this host has a Rust toolchain.
pub fn ensure(index: &SdkIndex, host: &str, dry_run: bool) -> Result<Outcome> {
    match plan(&probe_host(), index, host) {
        RustPlan::Present(what) => Ok(Outcome {
            line: format!("already present ({what})"),
            changed: false,
        }),
        RustPlan::Unavailable(why) => bail!("{why}"),
        RustPlan::SetDefault { rustup, channel } => {
            if dry_run {
                return Ok(Outcome {
                    line: format!("would install toolchain {channel} as rustup's default"),
                    changed: false,
                });
            }
            let r = rustup.to_string_lossy();
            sh(
                &[&r, "toolchain", "install", &channel, "--profile", "minimal"],
                None,
            )
            .wrap_err_with(|| format!("rustup toolchain install {channel}"))?;
            sh(&[&r, "default", &channel], None)
                .wrap_err_with(|| format!("rustup default {channel}"))?;
            Ok(Outcome {
                line: format!("installed toolchain {channel} (rustup default)"),
                changed: true,
            })
        }
        RustPlan::Bootstrap {
            url,
            sha256,
            version,
            channel,
        } => {
            if dry_run {
                return Ok(Outcome {
                    line: format!("would run rustup-init {version} (default toolchain {channel})"),
                    changed: false,
                });
            }
            // The store's download cache (RFC-0095 D2, `fetch/`), keyed by
            // version so two pins cannot collide.
            let dir = super::store::root().join("fetch");
            std::fs::create_dir_all(&dir).wrap_err_with(|| format!("create {}", dir.display()))?;
            let init = dir.join(format!("rustup-init-{version}"));
            let init_s = init.to_string_lossy().into_owned();
            sh(
                &[
                    "curl",
                    "-L",
                    "--fail",
                    "--silent",
                    "--show-error",
                    "--proto",
                    "=https",
                    "--tlsv1.2",
                    "-o",
                    &init_s,
                    &url,
                ],
                None,
            )
            .wrap_err_with(|| format!("download {url}"))?;
            verify_sha256(&init, &sha256)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&init, std::fs::Permissions::from_mode(0o755))
                    .wrap_err_with(|| format!("chmod {}", init.display()))?;
            }
            sh(
                &[
                    &init_s,
                    "-y",
                    "--no-modify-path",
                    "--profile",
                    "minimal",
                    "--default-toolchain",
                    &channel,
                ],
                None,
            )
            .wrap_err_with(|| format!("rustup-init {version}"))?;
            let _ = std::fs::remove_file(&init);
            // Prove it landed where the build will look, rather than trusting
            // the installer's exit code.
            match probe_host() {
                HostRust::Rustup {
                    bin,
                    default_toolchain: Some(tc),
                } => Ok(Outcome {
                    line: format!(
                        "installed rustup {version}, default {tc} ({}; cmake finds it there — \
                         for cargo in a shell: . \"$HOME/.cargo/env\")",
                        bin.display()
                    ),
                    changed: true,
                }),
                other => bail!(
                    "rustup-init {version} finished, but no rustup with a default toolchain \
                     is where Corrosion looks (PATH, $CARGO_HOME/bin, ~/.cargo/bin): {other:?}"
                ),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(extra: &str) -> SdkIndex {
        SdkIndex::parse(&format!(
            "[rust.toolchain.stable]\nchannel = \"stable\"\n{extra}"
        ))
        .unwrap()
    }

    const RUSTUP: &str = "[rust.rustup]\nversion = \"1.29.1\"\ntoolchain = \"stable\"\n\
        dist.linux-x86_64 = { url = \"https://example/rustup-init\", sha256 = \"abc\" }\n";

    #[test]
    fn a_host_with_a_default_toolchain_is_left_alone() {
        let found = HostRust::Rustup {
            bin: "/r".into(),
            default_toolchain: Some("nightly-x".into()),
        };
        assert_eq!(
            plan(&found, &index(RUSTUP), "linux-x86_64"),
            RustPlan::Present("rustup, default nightly-x".into())
        );
        assert!(matches!(
            plan(&HostRust::BareRustc, &index(RUSTUP), "linux-x86_64"),
            RustPlan::Present(_)
        ));
    }

    #[test]
    fn rustup_with_no_default_gets_the_indexed_channel() {
        let found = HostRust::Rustup {
            bin: "/r".into(),
            default_toolchain: None,
        };
        assert_eq!(
            plan(&found, &index(RUSTUP), "linux-x86_64"),
            RustPlan::SetDefault {
                rustup: "/r".into(),
                channel: "stable".into()
            }
        );
    }

    /// The installed path's case — issue 1304.
    #[test]
    fn a_host_with_no_rust_runs_the_pinned_installer_for_its_host() {
        assert_eq!(
            plan(&HostRust::Absent, &index(RUSTUP), "linux-x86_64"),
            RustPlan::Bootstrap {
                url: "https://example/rustup-init".into(),
                sha256: "abc".into(),
                version: "1.29.1".into(),
                channel: "stable".into(),
            }
        );
        // No dist for this host, and no section at all: both named, never a guess.
        assert!(matches!(
            plan(&HostRust::Absent, &index(RUSTUP), "linux-riscv64"),
            RustPlan::Unavailable(m) if m.contains("linux-riscv64")
        ));
        assert!(matches!(
            plan(&HostRust::Absent, &index(""), "linux-x86_64"),
            RustPlan::Unavailable(m) if m.contains("[rust.rustup]")
        ));
    }

    #[test]
    fn the_default_is_read_in_both_rustup_spellings() {
        assert_eq!(
            parse_default("stable-x86_64-unknown-linux-gnu (default)\nnightly-x\n").as_deref(),
            Some("stable-x86_64-unknown-linux-gnu")
        );
        assert_eq!(
            parse_default("nightly-x\nstable-x86_64-unknown-linux-gnu (active, default)\n")
                .as_deref(),
            Some("stable-x86_64-unknown-linux-gnu")
        );
        assert_eq!(parse_default("no installed toolchains\n"), None);
    }

    /// The index this tree ships must actually carry an installer for the one
    /// host a release is cut for today.
    #[test]
    fn the_repo_index_pins_an_installer_for_the_release_host() {
        let idx = SdkIndex::load(Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../nros-sdk-index.toml"
        )))
        .unwrap();
        assert!(matches!(
            plan(&HostRust::Absent, &idx, "linux-x86_64"),
            RustPlan::Bootstrap { .. }
        ));
    }

    #[test]
    fn an_installer_naming_an_undefined_toolchain_is_refused() {
        let bad = "[rust.rustup]\nversion = \"1\"\ntoolchain = \"ghost\"\n\
            dist.linux-x86_64 = { url = \"u\", sha256 = \"h\" }\n";
        let err = SdkIndex::parse(&format!(
            "[rust.toolchain.stable]\nchannel = \"stable\"\n{bad}"
        ))
        .and_then(|i| i.validate().map(|()| i))
        .unwrap_err();
        assert!(format!("{err:#}").contains("ghost"));
    }
}
