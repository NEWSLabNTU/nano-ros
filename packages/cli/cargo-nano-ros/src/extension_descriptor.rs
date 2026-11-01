//! phase-527 W1 / RFC-0119 D2 — the `nros-runner.toml` and
//! `nros-command.toml` descriptors.
//!
//! A `runner` or `command` provider is announced like every other provider
//! (`<nano_ros_provides kind="runner" name="qemu"/>`, RFC-0087 D3), and its
//! descriptor holds only what no convention can produce (RFC-0087 D4):
//!
//! ```toml
//! # nros-runner.toml
//! protocol = 1
//! verbs    = ["run", "console"]   # any of run, flash, console
//! needs    = ["elf", "nic"]       # plan placeholders it consumes (RFC-0118 D5)
//! bin      = "nros-runner-qemu"   # default: the package's single bin target
//! ```
//!
//! ```toml
//! # nros-command.toml
//! protocol = 1
//! summary  = "Print or deploy the zenoh router configuration for a bringup"
//! tier     = "user"               # user | dev
//! bin      = "nros-zenoh-tools"
//! ```
//!
//! The verb or runner name is the announced `name`. The cargo-feature, crate
//! and C-token derivations of [`crate::derived_descriptor`] do not apply,
//! because nothing links against these packages; the one derivable field is
//! `bin`, and a stated `bin` that disagrees with the derived one is refused.
//!
//! **Read only for the provider actually selected** (RFC-0087 D4): the scan
//! reads `package.xml` alone, and [`resolve_runner_in`] /
//! [`resolve_command_in`] read the descriptor of the one package
//! [`resolve_unique`] picked.
//!
//! The vocabularies (verbs, placeholders, protocol versions) are
//! `nros-extension`'s, the SDK crate the runners themselves are written
//! against, so a descriptor and a `describe` reply cannot disagree about what
//! a word means.

use std::{
    fmt,
    path::{Path, PathBuf},
};

use nros_extension::{
    plan::{Placeholder, Verb},
    protocol,
};
use serde::Deserialize;

use crate::{
    package_xml::{BuildRole, COMMAND_KIND, RUNNER_KIND, build_role_of},
    provider_scan::{ProviderPackage, ResolveError, ScanResult, resolve_unique},
};

/// Where a `command` verb sits in the tree (RFC-0117).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommandTier {
    /// `nros <verb>`.
    User,
    /// `nros dev <verb>`.
    Dev,
}

/// `nros-runner.toml`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerDescriptor {
    /// The protocol versions the runner speaks; a number or an array.
    #[serde(deserialize_with = "de_protocol")]
    pub protocol: Vec<u32>,
    /// The verbs it implements.
    pub verbs: Vec<Verb>,
    /// The plan placeholders it consumes.
    #[serde(default)]
    pub needs: Vec<String>,
    /// The executable, when the package has more than one bin target.
    #[serde(default)]
    pub bin: Option<String>,
}

/// `nros-command.toml`, as written.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDescriptor {
    /// The protocol versions the command speaks; a number or an array.
    #[serde(deserialize_with = "de_protocol")]
    pub protocol: Vec<u32>,
    /// One line for `nros help --all`.
    pub summary: String,
    /// Where the verb sits.
    #[serde(default = "default_tier")]
    pub tier: CommandTier,
    /// The executable, when the package has more than one bin target.
    #[serde(default)]
    pub bin: Option<String>,
}

fn default_tier() -> CommandTier {
    CommandTier::User
}

fn de_protocol<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<u32>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(u32),
        Many(Vec<u32>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(v) => vec![v],
        OneOrMany::Many(v) => v,
    })
}

/// Parse an `nros-runner.toml`. `origin` names the file in errors.
///
/// # Errors
///
/// A message naming the file and the fault: malformed TOML, an unknown key, an
/// unknown verb, no verb, an unknown placeholder, no protocol, or a protocol
/// this nros does not speak.
pub fn parse_runner_descriptor(text: &str, origin: &str) -> Result<RunnerDescriptor, String> {
    let d: RunnerDescriptor = toml::from_str(text).map_err(|e| format!("{origin}: {e}"))?;
    check_protocol(&d.protocol, origin)?;
    if d.verbs.is_empty() {
        return Err(format!(
            "{origin}: `verbs` is empty — a runner implements at least one of {}",
            Verb::ALL.map(Verb::as_str).join(", ")
        ));
    }
    if let Some(bad) = d.needs.iter().find(|n| Placeholder::parse(n).is_none()) {
        return Err(format!(
            "{origin}: needs {bad:?}, which is not a plan placeholder (known: {}, \
             tool:<name>; RFC-0118 D5)",
            nros_extension::plan::PLACEHOLDERS.join(", ")
        ));
    }
    Ok(d)
}

/// Parse an `nros-command.toml`. `origin` names the file in errors.
///
/// # Errors
///
/// As [`parse_runner_descriptor`], with an empty `summary` in place of the
/// verb checks.
pub fn parse_command_descriptor(text: &str, origin: &str) -> Result<CommandDescriptor, String> {
    let d: CommandDescriptor = toml::from_str(text).map_err(|e| format!("{origin}: {e}"))?;
    check_protocol(&d.protocol, origin)?;
    if d.summary.trim().is_empty() {
        return Err(format!(
            "{origin}: `summary` is empty — `nros help --all` prints it beside the verb"
        ));
    }
    Ok(d)
}

fn check_protocol(theirs: &[u32], origin: &str) -> Result<(), String> {
    protocol::negotiate(protocol::SUPPORTED, theirs)
        .map(|_| ())
        .map_err(|e| format!("{origin}: {e}"))
}

/// The bin target a package's `Cargo.toml` derives, when there is exactly one.
///
/// `[[bin]]` entries name themselves; a manifest with none has the implicit
/// `src/main.rs` target named after the package. `None` when the count is not
/// one (no binary, or several), or for a package with no `Cargo.toml` — a CMake
/// package's executable is not derivable, so its descriptor must state `bin`.
#[must_use]
pub fn derived_bin(package_dir: &Path) -> Option<String> {
    let bins = cargo_bins(package_dir);
    match bins.as_slice() {
        [one] => Some(one.clone()),
        _ => None,
    }
}

/// Every bin target a package's `Cargo.toml` declares, explicit or implicit.
#[must_use]
pub fn cargo_bins(package_dir: &Path) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(package_dir.join("Cargo.toml")) else {
        return Vec::new();
    };
    let Ok(manifest) = text.parse::<toml::Table>() else {
        return Vec::new();
    };
    let explicit: Vec<String> = manifest
        .get("bin")
        .and_then(toml::Value::as_array)
        .map(|bins| {
            bins.iter()
                .filter_map(|b| b.get("name").and_then(toml::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if !explicit.is_empty() {
        return explicit;
    }
    let package_name = manifest
        .get("package")
        .and_then(|p| p.get("name"))
        .and_then(toml::Value::as_str);
    match package_name {
        Some(n) if package_dir.join("src/main.rs").is_file() => vec![n.to_string()],
        _ => Vec::new(),
    }
}

/// The executable a host-tool package runs: a stated `bin` checked against
/// the derived one, or the derived one alone.
///
/// # Errors
///
/// A stated `bin` that differs from the single derived bin target, a stated
/// `bin` naming none of several, or nothing stated and nothing derivable.
pub fn resolve_bin(
    package_dir: &Path,
    stated: Option<&str>,
    origin: &str,
) -> Result<String, String> {
    let bins = cargo_bins(package_dir);
    match (stated, bins.as_slice()) {
        (Some(s), [one]) if s != one => Err(format!(
            "{origin}: states bin = {s:?}, but the package's only bin target is {one:?} — \
             a stated field must equal its derived value (RFC-0087 D4); drop the line or \
             rename the target"
        )),
        (Some(s), many) if many.len() > 1 && !many.iter().any(|b| b == s) => Err(format!(
            "{origin}: states bin = {s:?}, which is none of the package's bin targets ({})",
            many.join(", ")
        )),
        (Some(s), _) => Ok(s.to_string()),
        (None, [one]) => Ok(one.clone()),
        (None, []) => Err(format!(
            "{origin}: no `bin` stated and none derivable — the package has no Cargo bin \
             target (a CMake package states its executable)"
        )),
        (None, many) => Err(format!(
            "{origin}: no `bin` stated and the package has several bin targets ({}) — \
             state which one nros runs",
            many.join(", ")
        )),
    }
}

/// A runner resolved to its provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRunner {
    /// The canonical runner name (the package's first `runner` announcement).
    pub name: String,
    /// The providing package.
    pub package: String,
    /// The package directory.
    pub dir: PathBuf,
    /// The descriptor as written.
    pub descriptor: RunnerDescriptor,
    /// The executable's name (stated or derived).
    pub bin: String,
}

/// A command resolved to its provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCommand {
    /// The verb.
    pub name: String,
    /// The providing package.
    pub package: String,
    /// The package directory.
    pub dir: PathBuf,
    /// The descriptor as written.
    pub descriptor: CommandDescriptor,
    /// The executable's name (stated or derived).
    pub bin: String,
}

/// A runner or command could not be resolved.
#[derive(Debug)]
pub enum ExtensionResolveError {
    /// No provider, or an ambiguous one.
    Scan(ResolveError),
    /// The provider was found and is unusable.
    Provider {
        /// The provider's directory.
        dir: PathBuf,
        /// What is wrong with it.
        message: String,
    },
}

impl fmt::Display for ExtensionResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExtensionResolveError::Scan(e) => write!(f, "{e}"),
            ExtensionResolveError::Provider { dir, message } => {
                write!(f, "provider at {}: {message}", dir.display())
            }
        }
    }
}

impl std::error::Error for ExtensionResolveError {}

/// What every host-tool resolution shares: pick the provider, refuse a mixed
/// package, read the one descriptor.
fn resolve_host(
    scan: &ScanResult,
    kind: &str,
    name: &str,
) -> Result<(ProviderPackage, String, String, PathBuf), ExtensionResolveError> {
    let pkg = resolve_unique(scan, kind, name)
        .map_err(ExtensionResolveError::Scan)?
        .winner;
    let fail = |message: String| ExtensionResolveError::Provider {
        dir: pkg.dir.clone(),
        message,
    };
    match build_role_of(&pkg.provides) {
        Ok(Some(BuildRole::Host)) => {}
        Ok(other) => {
            return Err(fail(format!(
                "resolved as a {kind} provider but classified as {other:?}"
            )));
        }
        Err(e) => return Err(fail(e)),
    }
    let canonical = pkg
        .provides
        .iter()
        .find(|p| p.kind == kind)
        .map(|p| p.name.clone())
        .ok_or_else(|| fail(format!("announces no {kind} provision")))?;
    let path = pkg.descriptor_path(kind);
    let text = std::fs::read_to_string(&path).map_err(|e| {
        fail(format!(
            "read {}: {e} — a {kind} provider needs a descriptor (RFC-0119 D2)",
            path.display()
        ))
    })?;
    Ok((pkg, canonical, text, path))
}

/// Resolve a runner name against a scan and read its descriptor.
///
/// # Errors
///
/// An [`ExtensionResolveError`].
pub fn resolve_runner_in(
    scan: &ScanResult,
    name: &str,
) -> Result<ResolvedRunner, ExtensionResolveError> {
    let (pkg, canonical, text, path) = resolve_host(scan, RUNNER_KIND, name)?;
    let fail = |message: String| ExtensionResolveError::Provider {
        dir: pkg.dir.clone(),
        message,
    };
    let origin = path.display().to_string();
    let descriptor = parse_runner_descriptor(&text, &origin).map_err(fail)?;
    let bin = resolve_bin(&pkg.dir, descriptor.bin.as_deref(), &origin).map_err(fail)?;
    Ok(ResolvedRunner {
        name: canonical,
        package: pkg.package.clone(),
        dir: pkg.dir.clone(),
        descriptor,
        bin,
    })
}

/// Resolve a command verb against a scan and read its descriptor.
///
/// # Errors
///
/// An [`ExtensionResolveError`].
pub fn resolve_command_in(
    scan: &ScanResult,
    name: &str,
) -> Result<ResolvedCommand, ExtensionResolveError> {
    let (pkg, canonical, text, path) = resolve_host(scan, COMMAND_KIND, name)?;
    let fail = |message: String| ExtensionResolveError::Provider {
        dir: pkg.dir.clone(),
        message,
    };
    let origin = path.display().to_string();
    let descriptor = parse_command_descriptor(&text, &origin).map_err(fail)?;
    let bin = resolve_bin(&pkg.dir, descriptor.bin.as_deref(), &origin).map_err(fail)?;
    Ok(ResolvedCommand {
        name: canonical,
        package: pkg.package.clone(),
        dir: pkg.dir.clone(),
        descriptor,
        bin,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::provider_scan::scan_roots;

    fn write(path: &Path, body: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, body).unwrap();
    }

    fn pkg_xml(name: &str, provides: &str) -> String {
        format!(
            "<?xml version=\"1.0\"?><package format=\"3\"><name>{name}</name>\
             <version>0.1.0</version><description>d</description>\
             <maintainer email=\"a@b.c\">m</maintainer><license>MIT</license>\
             <export><build_type>nros_cargo</build_type>{provides}</export></package>"
        )
    }

    fn cargo(dir: &Path, package: &str, bins: &[&str]) {
        let mut s = format!("[package]\nname = \"{package}\"\nversion = \"0.1.0\"\n");
        for b in bins {
            s.push_str(&format!(
                "[[bin]]\nname = \"{b}\"\npath = \"src/main.rs\"\n"
            ));
        }
        write(&dir.join("Cargo.toml"), &s);
        write(&dir.join("src/main.rs"), "fn main() {}\n");
    }

    #[test]
    fn runner_descriptor_parses_and_validates() {
        let d = parse_runner_descriptor(
            "protocol = 1\nverbs = [\"run\", \"console\"]\nneeds = [\"elf\", \"tool:qemu\"]\n",
            "x",
        )
        .unwrap();
        assert_eq!(d.protocol, vec![1]);
        assert_eq!(d.verbs, vec![Verb::Run, Verb::Console]);
        assert_eq!(d.bin, None);
        assert_eq!(
            parse_runner_descriptor("protocol = [1, 2]\nverbs = [\"flash\"]\n", "x")
                .unwrap()
                .protocol,
            vec![1, 2]
        );
        for (bad, why) in [
            ("verbs = [\"run\"]\n", "missing field `protocol`"),
            ("protocol = 9\nverbs = [\"run\"]\n", "protocol mismatch"),
            ("protocol = 1\nverbs = []\n", "`verbs` is empty"),
            ("protocol = 1\nverbs = [\"debug\"]\n", "unknown variant"),
            (
                "protocol = 1\nverbs = [\"run\"]\nneeds = [\"home\"]\n",
                "not a plan placeholder",
            ),
            (
                "protocol = 1\nverbs = [\"run\"]\nname = \"qemu\"\n",
                "unknown field",
            ),
        ] {
            let e = parse_runner_descriptor(bad, "nros-runner.toml").unwrap_err();
            assert!(e.contains(why), "{bad:?}: {e}");
            assert!(e.starts_with("nros-runner.toml"), "{e}");
        }
    }

    #[test]
    fn command_descriptor_parses_and_validates() {
        let d = parse_command_descriptor("protocol = 1\nsummary = \"s\"\n", "x").unwrap();
        assert_eq!(d.tier, CommandTier::User);
        let d = parse_command_descriptor("protocol = 1\nsummary = \"s\"\ntier = \"dev\"\n", "x")
            .unwrap();
        assert_eq!(d.tier, CommandTier::Dev);
        for (bad, why) in [
            ("protocol = 1\nsummary = \"  \"\n", "`summary` is empty"),
            (
                "protocol = 1\nsummary = \"s\"\ntier = \"admin\"\n",
                "unknown variant",
            ),
            ("protocol = 1\n", "missing field `summary`"),
        ] {
            let e = parse_command_descriptor(bad, "x").unwrap_err();
            assert!(e.contains(why), "{bad:?}: {e}");
        }
    }

    #[test]
    fn bin_is_derived_and_a_stated_one_must_agree() {
        let tmp = tempfile::tempdir().unwrap();
        let one = tmp.path().join("one");
        cargo(&one, "one", &["nros-runner-one"]);
        assert_eq!(derived_bin(&one).as_deref(), Some("nros-runner-one"));
        assert_eq!(resolve_bin(&one, None, "x").unwrap(), "nros-runner-one");
        assert_eq!(
            resolve_bin(&one, Some("nros-runner-one"), "x").unwrap(),
            "nros-runner-one"
        );
        let e = resolve_bin(&one, Some("other"), "x").unwrap_err();
        assert!(e.contains("must equal its derived value"), "{e}");

        let implicit = tmp.path().join("implicit");
        cargo(&implicit, "implicit_tool", &[]);
        assert_eq!(derived_bin(&implicit).as_deref(), Some("implicit_tool"));

        let two = tmp.path().join("two");
        cargo(&two, "two", &["a", "b"]);
        assert_eq!(derived_bin(&two), None);
        assert!(
            resolve_bin(&two, None, "x")
                .unwrap_err()
                .contains("several bin targets")
        );
        assert_eq!(resolve_bin(&two, Some("b"), "x").unwrap(), "b");
        assert!(
            resolve_bin(&two, Some("c"), "x")
                .unwrap_err()
                .contains("none of")
        );

        let cmake = tmp.path().join("cmake");
        std::fs::create_dir_all(&cmake).unwrap();
        assert!(
            resolve_bin(&cmake, None, "x")
                .unwrap_err()
                .contains("none derivable")
        );
        assert_eq!(resolve_bin(&cmake, Some("tool"), "x").unwrap(), "tool");
    }

    #[test]
    fn resolves_runner_and_command_from_a_scan() {
        let tmp = tempfile::tempdir().unwrap();
        let r = tmp.path().join("runner");
        write(
            &r.join("package.xml"),
            &pkg_xml(
                "my_runner",
                r#"<nano_ros_provides kind="runner" name="acme"/>"#,
            ),
        );
        write(
            &r.join("nros-runner.toml"),
            "protocol = 1\nverbs = [\"flash\"]\n",
        );
        cargo(&r, "my_runner", &["nros-runner-acme"]);
        let c = tmp.path().join("command");
        write(
            &c.join("package.xml"),
            &pkg_xml(
                "my_cmd",
                r#"<nano_ros_provides kind="command" name="greet"/>"#,
            ),
        );
        write(
            &c.join("nros-command.toml"),
            "protocol = 1\nsummary = \"hi\"\nbin = \"greet\"\n",
        );

        let scan = scan_roots(&[tmp.path().to_path_buf()]).unwrap();
        assert!(scan.errors.is_empty(), "{:?}", scan.errors);
        let runner = resolve_runner_in(&scan, "acme").unwrap();
        assert_eq!(runner.package, "my_runner");
        assert_eq!(runner.bin, "nros-runner-acme");
        let cmd = resolve_command_in(&scan, "greet").unwrap();
        assert_eq!(
            (cmd.package.as_str(), cmd.bin.as_str()),
            ("my_cmd", "greet")
        );
        assert!(matches!(
            resolve_runner_in(&scan, "nope"),
            Err(ExtensionResolveError::Scan(_))
        ));

        // The descriptor is read only for the provider selected: a broken one
        // elsewhere does not stop this resolution, and fails its own.
        std::fs::remove_file(c.join("nros-command.toml")).unwrap();
        assert!(resolve_runner_in(&scan, "acme").is_ok());
        let e = resolve_command_in(&scan, "greet").unwrap_err().to_string();
        assert!(e.contains("needs a descriptor"), "{e}");
    }
}
