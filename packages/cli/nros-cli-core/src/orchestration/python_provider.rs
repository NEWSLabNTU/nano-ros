//! Which provider a `[python.*]` remedy names on THIS host — issue 1481.
//!
//! The `ros-<edition>-*` apt packages are built against the dependency versions
//! Ubuntu locks. `pip3 install --user <x>` puts a copy in
//! `~/.local/lib/pythonX/site-packages`, which PRECEDES
//! `/usr/lib/python3/dist-packages` on `sys.path` — so the remedy `nros setup
//! --check` used to print unconditionally SHADOWED the build ROS was compiled
//! against. pip is legitimate only where apt provides nothing.
//!
//! Two decisions are worth stating, because neither is obvious:
//!
//! * **Availability is MEASURED, not tabulated.** `python3-catkin-pkg` and
//!   `python3-colcon-common-extensions` come only from packages.ros.org/ros2;
//!   `python3-empy`/`lark`/`tomli`/`yaml` come from plain Ubuntu. Those are
//!   different fallback stories, and a table saying which would have to know
//!   whether the host configured the ROS repo — which only the host knows. So
//!   the resolver asks `apt-cache policy` (read-only) and falls to pip with the
//!   reason NAMED when there is no candidate. That is also why no `[python.*]`
//!   entry needs a per-release `apt` table: a release that does not package one
//!   simply has no candidate there.
//! * **A version pin makes presence insufficient.** `empy` is pinned to 3.3.4
//!   because 4.x breaks rosidl templates. An apt candidate that does not
//!   satisfy the pin is refused and pip wins — conservatively, since an
//!   unparsable candidate is not satisfaction.

use crate::orchestration::sdk_index::PythonDep;

/// What `apt-cache policy` says about one package.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AptPolicy {
    /// The installed version, or `None` for `(none)`.
    pub installed: Option<String>,
    /// The version apt would install, or `None` when there is none.
    pub candidate: Option<String>,
}

/// Asks the host's apt about a package.
///
/// A trait so the resolution logic is testable without an apt — and without
/// installing anything, which on a host that HAS apt would create the very
/// shadowing this module exists to prevent.
pub trait AptQuery {
    /// `None` when apt knows no such package at all.
    fn policy(&self, package: &str) -> Option<AptPolicy>;
}

/// Why a remedy is pip rather than apt. Always NAMED in the printed remedy: a
/// user who can see the apt package on their host deserves to know why the tool
/// did not name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PipBecause {
    /// The index refuses apt for this entry, with this reason.
    Refused(String),
    /// The index maps apt, but not for this release (`<release> = []`).
    NotPackagedOnThisRelease,
    /// This host has no apt.
    NoAptHost,
    /// apt knows no candidate for this package here — on a host with no ROS 2
    /// apt repo, this is what `python3-catkin-pkg` looks like.
    NoCandidate(String),
    /// apt has it, at a version the index's pin refuses.
    PinUnmet {
        package: String,
        candidate: String,
        want: String,
    },
}

/// Which provider this host should be told to use.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PythonProvider {
    Apt {
        packages: Vec<String>,
        /// Every declared package is already installed by apt. That is what
        /// makes a `~/.local` copy a SHADOW rather than the only copy.
        installed: bool,
    },
    Pip(PipBecause),
}

impl PythonProvider {
    /// True when this host should install with apt.
    #[must_use]
    pub fn is_apt(&self) -> bool {
        matches!(self, Self::Apt { .. })
    }
}

/// The remedy for one `[python.*]` entry on this host.
///
/// `release` is the host's OS release (`jammy`), for a per-release `apt` table;
/// `apt` is `None` on a host with no apt.
#[must_use]
pub fn resolve(
    dep: &PythonDep,
    release: Option<&str>,
    apt: Option<&dyn AptQuery>,
) -> PythonProvider {
    let declared = dep.apt_packages(release);
    if declared.is_empty() {
        // A refusal is the authored answer; an empty per-release override is
        // the index saying "not packaged there", which is a different fact and
        // reads differently in the remedy.
        return match &dep.apt_refused {
            Some(reason) => PythonProvider::Pip(PipBecause::Refused(reason.clone())),
            None => PythonProvider::Pip(PipBecause::NotPackagedOnThisRelease),
        };
    }
    let Some(apt) = apt else {
        return PythonProvider::Pip(PipBecause::NoAptHost);
    };

    let mut installed = true;
    for pkg in declared {
        let Some(policy) = apt.policy(pkg) else {
            return PythonProvider::Pip(PipBecause::NoCandidate(pkg.clone()));
        };
        let Some(candidate) = policy.candidate.as_deref() else {
            return PythonProvider::Pip(PipBecause::NoCandidate(pkg.clone()));
        };
        if let Some(want) = dep.version.as_deref()
            && !candidate_satisfies(candidate, want)
        {
            return PythonProvider::Pip(PipBecause::PinUnmet {
                package: pkg.clone(),
                candidate: candidate.to_string(),
                want: want.to_string(),
            });
        }
        installed &= policy.installed.is_some();
    }
    PythonProvider::Apt {
        packages: declared.to_vec(),
        installed,
    }
}

/// The upstream version inside a Debian version string: `1:3.3.4-2` -> `3.3.4`.
#[must_use]
pub fn upstream_version(debian: &str) -> &str {
    let after_epoch = match debian.split_once(':') {
        Some((epoch, rest)) if !epoch.is_empty() && epoch.bytes().all(|b| b.is_ascii_digit()) => {
            rest
        }
        _ => debian,
    };
    match after_epoch.rfind('-') {
        Some(i) => &after_epoch[..i],
        None => after_epoch,
    }
}

/// Does an apt candidate satisfy the index's exact pin?
///
/// Conservative on purpose — anything this cannot read as the pinned version
/// falls back to pip, which is merely a duplicate, where accepting a wrong
/// major `empy` is broken codegen. `3.3.4+ds1` is accepted (a packaging suffix);
/// `3.3.40` and `3.3.4.1` are not.
#[must_use]
pub fn candidate_satisfies(candidate: &str, want: &str) -> bool {
    let upstream = upstream_version(candidate);
    match upstream.strip_prefix(want) {
        None => false,
        Some("") => true,
        Some(rest) => {
            let c = rest.as_bytes()[0];
            !c.is_ascii_digit() && c != b'.'
        }
    }
}

/// `apt-cache policy` — read-only, and the only thing that can answer "does
/// this host have the ROS 2 apt repo" without being told.
#[derive(Clone, Copy, Debug, Default)]
pub struct AptCache;

impl AptCache {
    /// `Some` when this host has `apt-cache`.
    #[must_use]
    pub fn detect() -> Option<Self> {
        std::process::Command::new("apt-cache")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .ok()
            .filter(std::process::ExitStatus::success)
            .map(|_| Self)
    }
}

impl AptQuery for AptCache {
    fn policy(&self, package: &str) -> Option<AptPolicy> {
        let out = std::process::Command::new("apt-cache")
            .args(["policy", package])
            .output()
            .ok()?;
        parse_policy(&String::from_utf8_lossy(&out.stdout))
    }
}

/// Parse `apt-cache policy` stdout. Empty stdout — which is what an unknown
/// package produces, with a ZERO exit status — is `None`.
#[must_use]
pub fn parse_policy(stdout: &str) -> Option<AptPolicy> {
    let mut policy = AptPolicy::default();
    let mut saw_header = false;
    for line in stdout.lines() {
        let t = line.trim();
        if let Some(v) = t.strip_prefix("Installed:") {
            policy.installed = real_version(v.trim());
        } else if let Some(v) = t.strip_prefix("Candidate:") {
            policy.candidate = real_version(v.trim());
        } else if t.ends_with(':') && !t.starts_with("Version table") {
            saw_header = true;
        }
    }
    (saw_header || policy.candidate.is_some()).then_some(policy)
}

fn real_version(v: &str) -> Option<String> {
    (!v.is_empty() && v != "(none)").then(|| v.to_string())
}

/// The `python3` a build will actually import from — issue 1481's second half.
///
/// The shadow is not a curiosity: with apt's copy installed, a `~/.local` copy
/// is a SECOND build of the same library that wins every `import`, and the
/// versions coinciding today is the only reason it has gone unnoticed. So the
/// check NAMES it, the way a reported skip is honest where a silent one is not.
#[derive(Clone, Debug)]
pub struct Interpreter {
    exe: String,
    user_site: Option<String>,
}

/// What `find_spec` resolves a module to, and where the interpreter's per-user
/// directory is — the two halves of the shadow test.
const ORIGIN_SCRIPT: &str = "\
import importlib.util, sys
try:
    s = importlib.util.find_spec(sys.argv[1])
except Exception:
    s = None
p = ''
if s is not None:
    if s.origin:
        p = s.origin
    elif s.submodule_search_locations:
        p = list(s.submodule_search_locations)[0]
print(p)
";

impl Default for Interpreter {
    fn default() -> Self {
        Self::detect()
    }
}

impl Interpreter {
    /// This host's `python3`, with its per-user site directory resolved once.
    #[must_use]
    pub fn detect() -> Self {
        let exe = "python3".to_string();
        let user_site = std::process::Command::new(&exe)
            .args(["-m", "site", "--user-site"])
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty());
        Self { exe, user_site }
    }

    /// Where `import <module>` resolves from, as the interpreter answers it —
    /// never as the filesystem guesses (RFC-0097 D12's rule for
    /// `python_import`).
    #[must_use]
    pub fn module_origin(&self, module: &str) -> Option<String> {
        let out = std::process::Command::new(&self.exe)
            .args(["-c", ORIGIN_SCRIPT, module])
            .output()
            .ok()?;
        let origin = String::from_utf8_lossy(&out.stdout).trim().to_string();
        (!origin.is_empty()).then_some(origin)
    }

    /// The `~/.local` copy that is winning the import, if there is one.
    #[must_use]
    pub fn shadowing_user_copy(&self, module: &str) -> Option<String> {
        let user_site = self.user_site.as_deref()?;
        let origin = self.module_origin(module)?;
        is_under(&origin, user_site).then_some(origin)
    }
}

/// Is `origin` inside `dir`? A prefix test with a separator boundary, so
/// `/site-root/.local/lib/python3.10/site-packages-x` is not "under"
/// `.../site-packages`.
#[must_use]
pub fn is_under(origin: &str, dir: &str) -> bool {
    let dir = dir.trim_end_matches('/');
    origin
        .strip_prefix(dir)
        .is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    struct FakeApt(BTreeMap<String, AptPolicy>);

    impl FakeApt {
        fn new(rows: &[(&str, Option<&str>, Option<&str>)]) -> Self {
            Self(
                rows.iter()
                    .map(|(name, installed, candidate)| {
                        (
                            (*name).to_string(),
                            AptPolicy {
                                installed: installed.map(str::to_string),
                                candidate: candidate.map(str::to_string),
                            },
                        )
                    })
                    .collect(),
            )
        }
    }

    impl AptQuery for FakeApt {
        fn policy(&self, package: &str) -> Option<AptPolicy> {
            self.0.get(package).cloned()
        }
    }

    fn dep(pip: &str, apt: &[&str], version: Option<&str>) -> PythonDep {
        PythonDep {
            pip: pip.to_string(),
            apt: apt
                .iter()
                .map(|s| (*s).to_string())
                .collect::<Vec<_>>()
                .into(),
            version: version.map(str::to_string),
            ..PythonDep::default()
        }
    }

    /// The whole point: on a host whose apt has it, the remedy is apt.
    #[test]
    fn an_apt_provided_entry_resolves_to_apt() {
        let apt = FakeApt::new(&[("python3-lark", Some("1.1.1-1"), Some("1.1.1-1"))]);
        assert_eq!(
            resolve(
                &dep("lark", &["python3-lark"], None),
                Some("jammy"),
                Some(&apt)
            ),
            PythonProvider::Apt {
                packages: vec!["python3-lark".into()],
                installed: true,
            }
        );
    }

    /// Issue 1481's sharpest case, MEASURED on jammy/arm64: the index pins
    /// 3.3.4 and apt's candidate IS 3.3.4-2, so the pip remedy was installing a
    /// shadowing duplicate of the correct version.
    #[test]
    fn empys_hand_pin_is_met_by_ubuntus_own_package() {
        let apt = FakeApt::new(&[("python3-empy", Some("3.3.4-2"), Some("3.3.4-2"))]);
        assert!(
            resolve(
                &dep("empy", &["python3-empy"], Some("3.3.4")),
                Some("jammy"),
                Some(&apt)
            )
            .is_apt()
        );
    }

    /// ...and a release that moved to the incompatible major falls to pip, by
    /// measurement rather than by a table anyone has to keep current.
    #[test]
    fn an_apt_candidate_that_misses_the_pin_falls_back_to_pip() {
        let apt = FakeApt::new(&[("python3-empy", None, Some("4.0.1-1"))]);
        assert_eq!(
            resolve(
                &dep("empy", &["python3-empy"], Some("3.3.4")),
                Some("noble"),
                Some(&apt)
            ),
            PythonProvider::Pip(PipBecause::PinUnmet {
                package: "python3-empy".into(),
                candidate: "4.0.1-1".into(),
                want: "3.3.4".into(),
            })
        );
    }

    /// A host with no ROS 2 apt repo — the case that produced this layer
    /// (issue 0368 / phase-327). `python3-catkin-pkg` exists nowhere else, so
    /// apt has no candidate and pip is right, with the reason named.
    #[test]
    fn a_ros_repo_only_package_falls_back_to_pip_on_a_ros_less_host() {
        let apt = FakeApt::new(&[("python3-lark", Some("1.1.1-1"), Some("1.1.1-1"))]);
        assert_eq!(
            resolve(
                &dep("catkin_pkg", &["python3-catkin-pkg"], None),
                Some("jammy"),
                Some(&apt)
            ),
            PythonProvider::Pip(PipBecause::NoCandidate("python3-catkin-pkg".into()))
        );
    }

    /// Fedora, Arch, macOS: no apt at all, so pip, which is what the book
    /// already tells those hosts to do.
    #[test]
    fn a_host_with_no_apt_gets_pip() {
        assert_eq!(
            resolve(&dep("lark", &["python3-lark"], None), None, None),
            PythonProvider::Pip(PipBecause::NoAptHost)
        );
    }

    /// `west` and `clang-format` are decisions, and the reason travels with the
    /// remedy rather than being left to look like an oversight.
    #[test]
    fn a_refused_entry_carries_its_reason() {
        let py = PythonDep {
            pip: "west".into(),
            apt_refused: Some("PyPI only".into()),
            ..PythonDep::default()
        };
        let apt = FakeApt::new(&[]);
        assert_eq!(
            resolve(&py, Some("jammy"), Some(&apt)),
            PythonProvider::Pip(PipBecause::Refused("PyPI only".into()))
        );
    }

    /// `<release> = []` is a real answer and a DIFFERENT one from "not named"
    /// (`ManagerPackages`' own rule) — so it must not read as a refusal.
    #[test]
    fn an_empty_release_override_is_not_packaged_there() {
        let mut py = dep("tomli", &[], None);
        py.apt = crate::orchestration::sdk_index::ManagerPackages::ByRelease(BTreeMap::from([
            ("default".to_string(), vec!["python3-tomli".to_string()]),
            ("plucky".to_string(), Vec::new()),
        ]));
        let apt = FakeApt::new(&[("python3-tomli", None, Some("1.2.2-2"))]);
        assert!(resolve(&py, Some("jammy"), Some(&apt)).is_apt());
        assert_eq!(
            resolve(&py, Some("plucky"), Some(&apt)),
            PythonProvider::Pip(PipBecause::NotPackagedOnThisRelease)
        );
    }

    #[test]
    fn debian_versions_reduce_to_their_upstream() {
        assert_eq!(upstream_version("1:14.0-55~exp2"), "14.0");
        assert_eq!(upstream_version("3.3.4-2"), "3.3.4");
        assert_eq!(upstream_version("5.4.1-1ubuntu1"), "5.4.1");
        assert_eq!(upstream_version("1.1.0-101"), "1.1.0");
        assert_eq!(upstream_version("2.0"), "2.0");
        // Not an epoch: a colon with a non-numeric left side stays put.
        assert_eq!(upstream_version("a:b"), "a:b");
    }

    /// The direction that matters is the conservative one: anything unreadable
    /// as the pin loses, because a duplicate is cheap and a wrong major is not.
    #[test]
    fn pin_matching_accepts_packaging_suffixes_and_nothing_numeric() {
        assert!(candidate_satisfies("3.3.4-2", "3.3.4"));
        assert!(candidate_satisfies("3.3.4", "3.3.4"));
        assert!(candidate_satisfies("3.3.4+ds1-1", "3.3.4"));
        assert!(!candidate_satisfies("3.3.40-1", "3.3.4"));
        assert!(!candidate_satisfies("3.3.4.1-1", "3.3.4"));
        assert!(!candidate_satisfies("4.0.1-1", "3.3.4"));
    }

    /// An unknown package produces EMPTY stdout and a ZERO exit status, so the
    /// status cannot be the discriminator — measured on jammy.
    #[test]
    fn policy_parsing_reads_apt_caches_own_shape() {
        let installed = parse_policy(
            "python3-empy:\n  Installed: 3.3.4-2\n  Candidate: 3.3.4-2\n  Version table:\n     3.3.4-2 500\n",
        )
        .expect("a known package parses");
        assert_eq!(installed.installed.as_deref(), Some("3.3.4-2"));
        assert_eq!(installed.candidate.as_deref(), Some("3.3.4-2"));

        let uninstalled =
            parse_policy("python3-tomli:\n  Installed: (none)\n  Candidate: 1.2.2-2\n")
                .expect("an uninstalled package parses");
        assert_eq!(uninstalled.installed, None);
        assert_eq!(uninstalled.candidate.as_deref(), Some("1.2.2-2"));

        assert_eq!(parse_policy(""), None, "unknown package prints nothing");
    }

    /// The shadow test is a containment question, and the boundary matters:
    /// a sibling directory sharing a prefix is not inside.
    #[test]
    fn a_user_site_copy_is_recognised_by_containment_not_by_prefix() {
        let site = "/site-root/.local/lib/python3.10/site-packages";
        assert!(is_under(&format!("{site}/catkin_pkg/__init__.py"), site));
        assert!(is_under(
            &format!("{site}/yaml/__init__.py"),
            &format!("{site}/")
        ));
        assert!(!is_under("/usr/lib/python3/dist-packages/em.py", site));
        assert!(!is_under(&format!("{site}-old/x.py"), site));
        assert!(!is_under(site, site), "the directory is not a module in it");
    }

    /// RFC-0097 D12 one module over — the origin comes from the INTERPRETER, so
    /// the probe answers about the `python3` a build will really use.
    #[test]
    fn module_origin_asks_the_interpreter() {
        let py = Interpreter::detect();
        assert!(
            py.module_origin("nros_definitely_not_installed").is_none(),
            "a module that does not exist has no origin"
        );
        let json = py
            .module_origin("json")
            .expect("every CPython has json in its stdlib");
        assert!(json.ends_with(".py"), "unexpected origin for json: {json}");
    }
}
