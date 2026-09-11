//! phase-447 D1 (RFC-0099 D5) — is THIS host at or above a dist's floor?
//!
//! A dist is ABI-bound, and `host_key()` is `<os>-<arch>` with no OS version and
//! no libc in it, so the key alone offers every Linux x86_64 host every Linux
//! x86_64 dist — including hosts that cannot run it. Widening the key was
//! rejected (compatibility is a RANGE: a glibc 2.39 host runs a 2.35 binary), so
//! the dist row carries a MEASURED floor and this module compares the host with
//! it before anything is downloaded.
//!
//! Two halves, because the runtime has two halves:
//!
//! * **Forward** — the loader/libc family and the C++ runtime, which
//!   `nano-ros-sdk`'s `bundle.sh` never bundles. A number: `floor.glibc`,
//!   `floor.glibcxx`, `floor.macos`. An OLDER host fails it.
//! * **Backward** — a library the dist links BY NAME that a NEWER host no longer
//!   ships (`libpython3.10.so.1.0` on noble, which has 3.12). Not a number. It
//!   is the tool's `system = [..]` list, and a host is refused only when the
//!   soname is absent AND the prereq's per-release package names say the
//!   manager does not package it on this release (phase-447 D2) — absent but
//!   installable is `nros setup --system`'s job, as it always was.
//!
//! Every probe that cannot answer ABSTAINS. "Could not tell" never refuses a
//! dist: a false refusal costs a source build nobody needed, which is worse than
//! the loader error this exists to pre-empt.

use std::{collections::BTreeSet, sync::OnceLock};

use super::sdk_index::{DistArtifact, PrereqDep, host_key, host_os_release, version_cmp};

/// The C library this host runs, as far as it can be told.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Libc {
    /// Could not be determined — abstain.
    #[default]
    Unknown,
    /// glibc, at this version.
    Glibc(String),
    /// Some other libc (musl): a glibc-linked dist cannot start at all.
    Other(String),
}

/// The host's libstdc++, as far as it can be told.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Libstdcxx {
    #[default]
    Unknown,
    /// The loader can find no `libstdc++.so.6`.
    Absent,
    /// The highest `GLIBCXX_3.4.N` it defines, written `3.4.N`.
    Glibcxx(String),
}

/// What the floor comparison needs to know about a host. Gathered ONCE, at
/// the edge ([`HostFacts::current`]); the comparison itself is pure.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostFacts {
    pub libc: Libc,
    pub libstdcxx: Libstdcxx,
    /// `sw_vers -productVersion`, on macOS.
    pub macos: Option<String>,
    /// The detected package manager (`apt`, `dnf`, …).
    pub manager: Option<String>,
    /// `VERSION_CODENAME` / `VERSION_ID` — the D2 release key.
    pub release: Option<String>,
    /// Every soname `ldconfig -p` lists. `None` = could not ask.
    pub sonames: Option<BTreeSet<String>>,
}

impl HostFacts {
    /// Facts that answer nothing, so every comparison abstains. What a plan for
    /// a FOREIGN host key uses: this machine's glibc says nothing about it.
    #[must_use]
    pub fn unknown() -> Self {
        Self::default()
    }

    /// This machine's facts, probed on first use and cached for the process —
    /// a board setup plans ~20 tools and must not run `ldconfig` twenty times.
    #[must_use]
    pub fn current() -> &'static Self {
        static FACTS: OnceLock<HostFacts> = OnceLock::new();
        FACTS.get_or_init(Self::probe)
    }

    fn probe() -> Self {
        let ldconfig = ldconfig_listing();
        let sonames = ldconfig.as_deref().map(|l| {
            l.lines()
                .filter_map(|line| line.split_whitespace().next())
                .filter(|s| s.contains(".so"))
                .map(str::to_string)
                .collect()
        });
        let libstdcxx = match ldconfig.as_deref() {
            None => Libstdcxx::Unknown,
            Some(l) => match libstdcxx_path(l) {
                None => Libstdcxx::Absent,
                Some(p) => std::fs::read(&p)
                    .ok()
                    .and_then(|bytes| max_glibcxx(&bytes))
                    .map_or(Libstdcxx::Unknown, Libstdcxx::Glibcxx),
            },
        };
        Self {
            libc: probe_libc(),
            libstdcxx,
            macos: probe_macos(),
            manager: crate::cmd::setup::detect_package_manager().map(str::to_string),
            release: host_os_release(),
            sonames,
        }
    }

    /// Whether some library whose soname starts with `lib` is loadable — the
    /// same PREFIX rule `check.sharedlib` uses, so a probe written for the
    /// `--check` path means the same thing here. `None` = could not ask.
    #[must_use]
    pub fn has_soname(&self, lib: &str) -> Option<bool> {
        self.sonames
            .as_ref()
            .map(|set| set.iter().any(|s| s.starts_with(lib)))
    }
}

fn below(have: &str, want: &str) -> bool {
    version_cmp(have, want) == std::cmp::Ordering::Less
}

/// Why `dist` must not be downloaded onto a host with `facts`, or `None`.
///
/// `system` is the tool's `system = [..]` list resolved to its prereq rows
/// ([`super::sdk_index::ToolPackage::system_deps`]). The returned text names
/// the failing requirement AND the remedy; the caller adds what happens next
/// (a source build, or nothing to fall back to).
#[must_use]
pub fn refusal(
    dist: &DistArtifact,
    system: &[(String, PrereqDep)],
    facts: &HostFacts,
) -> Option<String> {
    if let Some(floor) = &dist.floor {
        if let Some(want) = &floor.glibc {
            match &facts.libc {
                Libc::Glibc(have) if below(have, want) => {
                    return Some(format!(
                        "it needs glibc >= {want} and this host has glibc {have}. Remedy: \
                         use a host whose distribution ships glibc {want} or newer"
                    ));
                }
                Libc::Other(name) => {
                    return Some(format!(
                        "it is linked against glibc (>= {want}) and this host's C library \
                         is {name}. Remedy: use a glibc-based host"
                    ));
                }
                _ => {}
            }
        }
        if let Some(want) = &floor.glibcxx {
            match &facts.libstdcxx {
                Libstdcxx::Glibcxx(have) if below(have, want) => {
                    return Some(format!(
                        "it needs a libstdc++ defining GLIBCXX_{want} and this host's \
                         newest is GLIBCXX_{have}. Remedy: install a newer libstdc++6 \
                         (a newer distribution, or its toolchain backports)"
                    ));
                }
                Libstdcxx::Absent => {
                    return Some(format!(
                        "it needs libstdc++.so.6 (GLIBCXX_{want}) and the loader finds none \
                         on this host. Remedy: install libstdc++6"
                    ));
                }
                _ => {}
            }
        }
        if let (Some(want), Some(have)) = (&floor.macos, &facts.macos)
            && below(have, want)
        {
            return Some(format!(
                "it needs macOS {want} or newer and this host is {have}. Remedy: \
                 upgrade macOS"
            ));
        }
    }
    // The backward half. Only for Linux artifacts, where `ldconfig` answers.
    let (Some(mgr), Some(release)) = (facts.manager.as_deref(), facts.release.as_deref()) else {
        return None;
    };
    for (key, dep) in system {
        let Some(so) = dep.check.as_ref().and_then(|c| c.sharedlib.as_deref()) else {
            continue;
        };
        if facts.has_soname(so) != Some(false) {
            continue; // present, or cannot tell
        }
        let unpackaged = dep
            .manager_packages(mgr)
            .and_then(|m| m.named_for(Some(release)))
            .is_some_and(<[String]>::is_empty);
        if unpackaged {
            return Some(format!(
                "it links `{so}` ([prereq.{key}]), which this host lacks and {mgr} does not \
                 package on {release} (the index says so: `{mgr}.{release} = []`). Remedy: \
                 a release that ships it, or a source build against this host's own"
            ));
        }
    }
    None
}

/// The facts to plan `host` with: this machine's when `host` IS this machine,
/// otherwise nothing (every comparison abstains).
#[must_use]
pub fn facts_for(host: &str) -> std::borrow::Cow<'static, HostFacts> {
    if host == host_key() {
        std::borrow::Cow::Borrowed(HostFacts::current())
    } else {
        std::borrow::Cow::Owned(HostFacts::unknown())
    }
}

fn ldconfig_listing() -> Option<String> {
    if std::env::consts::OS != "linux" {
        return None;
    }
    for cmd in ["ldconfig", "/sbin/ldconfig", "/usr/sbin/ldconfig"] {
        if let Ok(out) = std::process::Command::new(cmd).arg("-p").output()
            && out.status.success()
        {
            return Some(String::from_utf8_lossy(&out.stdout).into_owned());
        }
    }
    None
}

/// The `libstdc++.so.6` path `ldconfig -p` resolves, for this process's arch.
fn libstdcxx_path(listing: &str) -> Option<String> {
    let tag = match std::env::consts::ARCH {
        "x86_64" => "x86-64",
        "aarch64" => "AArch64",
        _ => "",
    };
    listing
        .lines()
        .filter(|l| l.trim_start().starts_with("libstdc++.so.6 "))
        .filter(|l| tag.is_empty() || l.contains(tag))
        .find_map(|l| l.split("=>").nth(1).map(|p| p.trim().to_string()))
}

/// The highest `GLIBCXX_3.4.N` string in a libstdc++ image, as `3.4.N`.
fn max_glibcxx(bytes: &[u8]) -> Option<String> {
    let needle = b"GLIBCXX_3.4";
    let mut best: Option<String> = None;
    let mut i = 0;
    while let Some(pos) = bytes[i..].windows(needle.len()).position(|w| w == needle) {
        let start = i + pos + "GLIBCXX_".len();
        let mut end = start;
        while end < bytes.len() && (bytes[end].is_ascii_digit() || bytes[end] == b'.') {
            end += 1;
        }
        let v = String::from_utf8_lossy(&bytes[start..end])
            .trim_end_matches('.')
            .to_string();
        if best.as_deref().is_none_or(|b| below(b, &v)) {
            best = Some(v);
        }
        i = end.max(i + pos + 1);
    }
    best
}

fn probe_libc() -> Libc {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    {
        // SAFETY: returns a pointer to a static NUL-terminated string owned by
        // glibc; it is never freed and never mutated.
        let v = unsafe { std::ffi::CStr::from_ptr(libc::gnu_get_libc_version()) };
        Libc::Glibc(v.to_string_lossy().into_owned())
    }
    #[cfg(not(all(target_os = "linux", target_env = "gnu")))]
    {
        if std::env::consts::OS != "linux" {
            return Libc::Unknown;
        }
        // A non-glibc `nros` (musl-static) may still run on a glibc host.
        if let Ok(out) = std::process::Command::new("getconf")
            .arg("GNU_LIBC_VERSION")
            .output()
            && out.status.success()
            && let Some(v) = String::from_utf8_lossy(&out.stdout)
                .split_whitespace()
                .nth(1)
        {
            return Libc::Glibc(v.to_string());
        }
        if let Ok(out) = std::process::Command::new("ldd").arg("--version").output() {
            let text = format!(
                "{}{}",
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
            if text.to_ascii_lowercase().contains("musl") {
                return Libc::Other("musl".to_string());
            }
        }
        Libc::Unknown
    }
}

fn probe_macos() -> Option<String> {
    if std::env::consts::OS != "macos" {
        return None;
    }
    let out = std::process::Command::new("sw_vers")
        .arg("-productVersion")
        .output()
        .ok()?;
    let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!v.is_empty()).then_some(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orchestration::sdk_index::{CheckProbe, DistFloor, ManagerPackages};
    use std::collections::BTreeMap;

    fn dist(floor: DistFloor) -> DistArtifact {
        DistArtifact {
            url: "u".into(),
            sha256: "h".into(),
            install: None,
            floor: Some(floor),
        }
    }

    fn glibc(v: &str) -> DistFloor {
        DistFloor {
            glibc: Some(v.into()),
            ..DistFloor::default()
        }
    }

    fn host(libc: Libc) -> HostFacts {
        HostFacts {
            libc,
            ..HostFacts::default()
        }
    }

    /// The forward half: an OLDER glibc is refused, an equal or newer one is
    /// not — a range, not an identity (RFC-0099 D5).
    #[test]
    fn glibc_floor_is_a_minimum() {
        let d = dist(glibc("2.35"));
        let old = host(Libc::Glibc("2.31".into()));
        let r = refusal(&d, &[], &old).expect("2.31 < 2.35 must refuse");
        assert!(r.contains("glibc >= 2.35") && r.contains("2.31"), "{r}");
        assert!(r.contains("Remedy"), "a refusal must name the remedy: {r}");
        assert_eq!(refusal(&d, &[], &host(Libc::Glibc("2.35".into()))), None);
        assert_eq!(refusal(&d, &[], &host(Libc::Glibc("2.39".into()))), None);
        // 2.4 vs 2.35 is numeric, not lexical.
        assert!(refusal(&dist(glibc("2.17")), &[], &host(Libc::Glibc("2.4".into()))).is_some());
    }

    /// A glibc-linked dist cannot start on musl at all; a host that cannot be
    /// read abstains rather than refusing.
    #[test]
    fn non_glibc_refuses_and_unknown_abstains() {
        let d = dist(glibc("2.17"));
        assert!(refusal(&d, &[], &host(Libc::Other("musl".into()))).is_some());
        assert_eq!(refusal(&d, &[], &host(Libc::Unknown)), None);
        assert_eq!(refusal(&d, &[], &HostFacts::unknown()), None);
    }

    #[test]
    fn glibcxx_and_macos_floors() {
        let d = dist(DistFloor {
            glibcxx: Some("3.4.30".into()),
            ..DistFloor::default()
        });
        let with = |s: Libstdcxx| HostFacts {
            libstdcxx: s,
            ..HostFacts::default()
        };
        assert!(refusal(&d, &[], &with(Libstdcxx::Glibcxx("3.4.28".into()))).is_some());
        assert!(refusal(&d, &[], &with(Libstdcxx::Absent)).is_some());
        assert_eq!(
            refusal(&d, &[], &with(Libstdcxx::Glibcxx("3.4.32".into()))),
            None
        );
        assert_eq!(refusal(&d, &[], &with(Libstdcxx::Unknown)), None);

        let m = dist(DistFloor {
            macos: Some("11.0".into()),
            ..DistFloor::default()
        });
        let mac = |v: &str| HostFacts {
            macos: Some(v.into()),
            ..HostFacts::default()
        };
        assert!(refusal(&m, &[], &mac("10.15")).is_some());
        assert_eq!(refusal(&m, &[], &mac("14.2")), None);
    }

    /// `none` is a MEASURED absence of a floor and never refuses.
    #[test]
    fn a_none_floor_never_refuses() {
        let d = dist(DistFloor {
            none: Some("static musl".into()),
            ..DistFloor::default()
        });
        assert_eq!(refusal(&d, &[], &host(Libc::Other("musl".into()))), None);
    }

    /// The backward half — the libpython3.10-on-noble shape. Refused only when
    /// the soname is ABSENT and the index says the manager does not package it
    /// on THIS release; absent-but-installable is `--system`'s job.
    #[test]
    fn backward_half_needs_absent_and_unpackaged_here() {
        let dep = PrereqDep {
            apt: ManagerPackages::ByRelease(BTreeMap::from([
                ("jammy".to_string(), vec!["libpython3.10".to_string()]),
                ("noble".to_string(), Vec::new()),
            ])),
            check: Some(CheckProbe {
                sharedlib: Some("libpython3.10.so.1.0".into()),
                ..CheckProbe::default()
            }),
            ..PrereqDep::default()
        };
        let system = vec![("libpython310".to_string(), dep)];
        let d = DistArtifact {
            floor: None,
            ..dist(DistFloor::default())
        };
        let on = |release: &str, libs: &[&str]| HostFacts {
            manager: Some("apt".into()),
            release: Some(release.into()),
            sonames: Some(libs.iter().map(|s| (*s).to_string()).collect()),
            ..HostFacts::default()
        };
        let r = refusal(&d, &system, &on("noble", &["libc.so.6"]))
            .expect("absent and unpackaged on noble must refuse");
        assert!(
            r.contains("libpython3.10.so.1.0") && r.contains("noble"),
            "{r}"
        );
        // Absent on jammy, where apt DOES package it: not a refusal.
        assert_eq!(refusal(&d, &system, &on("jammy", &["libc.so.6"])), None);
        // Present on noble (a deadsnakes PPA): not a refusal.
        assert_eq!(
            refusal(&d, &system, &on("noble", &["libpython3.10.so.1.0"])),
            None
        );
        // A release the table does not name: cannot say, so abstain.
        assert_eq!(refusal(&d, &system, &on("oracular", &["libc.so.6"])), None);
        // `ldconfig` unanswered: abstain.
        let blind = HostFacts {
            sonames: None,
            ..on("noble", &[])
        };
        assert_eq!(refusal(&d, &system, &blind), None);
    }

    #[test]
    fn glibcxx_scan_takes_the_highest() {
        let img = b"..GLIBCXX_3.4\0GLIBCXX_3.4.9\0GLIBCXX_3.4.30\0GLIBCXX_3.4.29\0";
        assert_eq!(max_glibcxx(img).as_deref(), Some("3.4.30"));
        assert_eq!(max_glibcxx(b"nothing here"), None);
    }

    #[test]
    fn libstdcxx_path_picks_this_arch() {
        let listing = "\tlibstdc++.so.6 (libc6) => /usr/lib32/libstdc++.so.6\n\
                       \tlibstdc++.so.6 (libc6,x86-64) => /usr/lib/x86_64-linux-gnu/libstdc++.so.6\n";
        if std::env::consts::ARCH == "x86_64" {
            assert_eq!(
                libstdcxx_path(listing).as_deref(),
                Some("/usr/lib/x86_64-linux-gnu/libstdc++.so.6")
            );
        }
        assert_eq!(
            libstdcxx_path("\tlibc.so.6 (libc6) => /lib/libc.so.6\n"),
            None
        );
    }
}
