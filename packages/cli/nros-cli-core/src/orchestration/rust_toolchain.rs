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

/// The cargo `bin` directory to put in FRONT of this process's `PATH`, when
/// `PATH` reaches no `cargo` but the place `nros setup` installs rustup does.
///
/// phase-517 W10. `nros setup` installs rustup into `~/.cargo` "where `cmake`
/// finds it without any PATH change" (installation.md), and the C/C++ road
/// honoured that — Corrosion looks in `~/.cargo/bin` itself. The cargo road did
/// not: `nros build` spawns `cargo` by name, so on a fresh machine the very
/// next command after `nros setup` died with `running cargo update …: No such
/// file or directory`, naming no remedy (measured replaying `bare-metal.md` on
/// the installed track). Every `cargo`/`rustc` this process starts — and every
/// one `cmake` starts on its behalf — inherits the fix from one place.
///
/// `None` when `PATH` already has a `cargo` (whichever toolchain a user put
/// there wins, as it always did) or when no candidate holds one.
#[must_use]
pub fn cargo_bin_to_prepend() -> Option<PathBuf> {
    cargo_bin_to_prepend_with(std::env::var_os("PATH").as_deref(), &cargo_bins(), |p| {
        p.is_file()
    })
}

/// Put [`cargo_bin_to_prepend`]'s directory in front of this process's `PATH`
/// — called once by the `nros` binary before it dispatches a command.
pub fn expose_cargo_bin() {
    let Some(dir) = cargo_bin_to_prepend() else {
        return;
    };
    let mut parts = vec![dir];
    if let Some(cur) = std::env::var_os("PATH") {
        parts.extend(std::env::split_paths(&cur));
    }
    crate::cmd::setup::set_process_path(parts);
}

/// The cargo setting `$NROS_OFFLINE` implies, or `None` when there is
/// nothing to set: unset `NROS_OFFLINE`, or a `CARGO_NET_OFFLINE` the user
/// chose already (theirs wins, either way).
fn cargo_offline_for(
    nros_offline: Option<&std::ffi::OsStr>,
    cargo_net_offline: Option<&std::ffi::OsStr>,
) -> Option<&'static str> {
    (nros_offline.is_some() && cargo_net_offline.is_none()).then_some("true")
}

/// issue 2197 / phase-523 W4 — `$NROS_OFFLINE` means no fetch at all, so every
/// cargo this process spawns (directly, or through a cmake it drives) runs with
/// cargo's own `CARGO_NET_OFFLINE`, and resolves git and registry dependencies
/// from `$CARGO_HOME`'s caches. Before this, the index honoured the flag and
/// cargo did not: the offline probe's `nros sync` died updating a git
/// dependency the cache already held. One place, beside [`expose_cargo_bin`],
/// before clap and before any thread.
pub fn expose_offline_to_cargo() {
    let Some(value) = cargo_offline_for(
        std::env::var_os("NROS_OFFLINE").as_deref(),
        std::env::var_os("CARGO_NET_OFFLINE").as_deref(),
    ) else {
        return;
    };
    // SAFETY: called from `main` before clap and before any thread or child
    // exists, so nothing reads the environment concurrently.
    unsafe { std::env::set_var("CARGO_NET_OFFLINE", value) };
}

fn cargo_bin_to_prepend_with(
    path: Option<&std::ffi::OsStr>,
    candidates: &[PathBuf],
    is_file: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let on_path = path.is_some_and(|p| std::env::split_paths(p).any(|d| is_file(&d.join("cargo"))));
    if on_path {
        return None;
    }
    candidates
        .iter()
        .find(|d| is_file(&d.join("cargo")))
        .cloned()
}

/// Add each of `needs` that rustup does not list as installed; returns the
/// one-line summary `nros setup` prints. Each need names the toolchain it
/// belongs to — `None` is rustup's default (the stable channel [`ensure`]
/// installs), `Some(channel)` a pinned one (a build-std board's nightly), which
/// is installed first when absent.
///
/// phase-517 W10. The chapters promised `nros setup mps2-an385-baremetal`
/// provisions "the Rust `thumbv7m-none-eabi` target", and nothing did: on a
/// fresh machine the next `nros build` stopped at preflight with `rustup target
/// add thumbv7m-none-eabi`; on NuttX it wanted `rust-src` (the toolchain
/// `nros setup` installs is rustup's minimal profile) and then Corrosion
/// refused the configure outright — `Could not find toolchain
/// 'nightly-2026-04-11'` — because the board builds with the pinned nightly
/// and nothing had installed it (all measured on the installed track). Without
/// rustup (a distro toolchain) there is nothing this can install, and the
/// build's own preflight still names the need, so that is a note rather than
/// an error.
pub fn ensure_needs(
    needs: &[(Option<String>, crate::builder::preflight::RustNeed)],
    dry_run: bool,
) -> Result<String, String> {
    use crate::builder::preflight::RustNeed;
    if needs.is_empty() {
        return Ok("no cross target for this board".to_string());
    }
    let label = |(tc, n): &(Option<String>, RustNeed)| {
        let what = match n {
            RustNeed::Target(t) => format!("target {t}"),
            RustNeed::Component(c) => format!("component {c}"),
        };
        match tc {
            Some(c) => format!("{what} ({c})"),
            None => what,
        }
    };
    let all = needs.iter().map(label).collect::<Vec<_>>().join(", ");
    let Some(bin) = find_rustup() else {
        return Ok(format!(
            "{all} — no rustup to install with; the build's preflight checks it"
        ));
    };
    let rustup = |args: &[&str]| -> Result<String, String> {
        let out = Command::new(&bin)
            .args(args)
            .current_dir("/")
            .env_remove("RUSTUP_TOOLCHAIN")
            .output()
            .map_err(|e| format!("run {} {}: {e}", bin.display(), args.join(" ")))?;
        Ok(String::from_utf8_lossy(&out.stdout).to_string())
    };
    let run = |args: &[&str]| -> Result<(), String> {
        let st = Command::new(&bin)
            .args(args)
            .current_dir("/")
            .env_remove("RUSTUP_TOOLCHAIN")
            .status()
            .map_err(|e| format!("run {} {}: {e}", bin.display(), args.join(" ")))?;
        if st.success() {
            Ok(())
        } else {
            Err(format!("`rustup {}` exited {st}", args.join(" ")))
        }
    };
    let toolchains = rustup(&["toolchain", "list"])?;
    let mut done = Vec::new();
    // A pinned channel that is not installed yet: install it, minimal profile,
    // before anything is added to it.
    let mut channels: Vec<&str> = needs.iter().filter_map(|(c, _)| c.as_deref()).collect();
    channels.sort_unstable();
    channels.dedup();
    for c in channels {
        if toolchains.lines().any(|l| {
            l.split_whitespace()
                .next()
                .is_some_and(|t| t.starts_with(c))
        }) {
            continue;
        }
        if dry_run {
            done.push(format!("would install toolchain {c}"));
            continue;
        }
        run(&["toolchain", "install", c, "--profile", "minimal"])?;
        done.push(format!("installed toolchain {c}"));
    }
    for need in needs {
        let (tc, n) = need;
        let mut tc_args: Vec<&str> = Vec::new();
        if let Some(c) = tc.as_deref() {
            tc_args = vec!["--toolchain", c];
        }
        let (verb, name) = match n {
            RustNeed::Target(t) => ("target", t.as_str()),
            RustNeed::Component(c) => ("component", c.as_str()),
        };
        let mut list = vec![verb, "list", "--installed"];
        list.extend(&tc_args);
        // An uninstalled pinned channel (dry run) lists nothing: report it as
        // missing, which is what it is.
        let installed = rustup(&list).unwrap_or_default();
        // `component list --installed` prints `rust-src` bare and host-suffixed
        // components as `<name>-<host>`.
        let present = installed.lines().any(|l| {
            l.trim() == name || (verb == "component" && l.trim().starts_with(&format!("{name}-")))
        });
        if present {
            continue;
        }
        if dry_run {
            done.push(format!("would add {}", label(need)));
            continue;
        }
        let mut add = vec![verb, "add", name];
        add.extend(&tc_args);
        run(&add)?;
        done.push(format!("added {}", label(need)));
    }
    if done.is_empty() {
        Ok(format!("{all} present"))
    } else {
        Ok(done.join(", "))
    }
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

    /// issue 2197 — NROS_OFFLINE turns cargo offline too, and never overrides
    /// a CARGO_NET_OFFLINE the user set (to `false` included).
    #[test]
    fn nros_offline_implies_cargo_offline_unless_the_user_chose() {
        use std::ffi::OsStr;
        assert_eq!(cargo_offline_for(Some(OsStr::new("1")), None), Some("true"));
        assert_eq!(
            cargo_offline_for(Some(OsStr::new("1")), Some(OsStr::new("false"))),
            None
        );
        assert_eq!(cargo_offline_for(None, None), None);
    }

    #[test]
    fn a_path_with_cargo_is_left_alone() {
        let has =
            |p: &Path| p == Path::new("/usr/bin/cargo") || p == Path::new("/h/.cargo/bin/cargo");
        let path = std::ffi::OsString::from("/usr/bin");
        assert_eq!(
            cargo_bin_to_prepend_with(Some(&path), &[PathBuf::from("/h/.cargo/bin")], has),
            None
        );
    }

    #[test]
    fn a_path_without_cargo_gets_the_installed_bin() {
        let has = |p: &Path| p == Path::new("/h/.cargo/bin/cargo");
        let path = std::ffi::OsString::from("/usr/bin");
        let bins = [PathBuf::from("/c/bin"), PathBuf::from("/h/.cargo/bin")];
        assert_eq!(
            cargo_bin_to_prepend_with(Some(&path), &bins, has),
            Some(PathBuf::from("/h/.cargo/bin"))
        );
        // …and nothing when no candidate holds one either.
        assert_eq!(
            cargo_bin_to_prepend_with(Some(&path), &bins, |_| false),
            None
        );
    }

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
